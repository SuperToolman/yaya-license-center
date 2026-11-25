use super::*;
use super::{
    engine,
    local_containers::{inspect, valid_id},
};
use axum::{
    extract::{
        Query,
        ws::{Message, WebSocketUpgrade},
    },
    http::{HeaderMap, header},
};
use futures_util::SinkExt;
use http_body_util::BodyExt;
use serde_json::{Value, json};

#[derive(Deserialize, Default, Clone)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Options {
    since: Option<i64>,
    until: Option<i64>,
    tail: Option<u16>,
    stream: Option<String>,
    #[serde(default)]
    download: bool,
}
impl Options {
    fn path(&self, id: &str, follow: bool) -> Result<String, String> {
        if !valid_id(id) {
            return Err("容器 ID 无效".into());
        }
        let tail = self.tail.unwrap_or(200);
        if tail == 0 || tail > 1000 {
            return Err("日志行数须为 1–1000".into());
        }
        if self.since.is_some_and(|v| v < 0)
            || self.until.is_some_and(|v| v < 0)
            || matches!((self.since,self.until),(Some(a),Some(b)) if b<a)
        {
            return Err("日志时间范围无效".into());
        }
        let stream = self.stream.as_deref().unwrap_or("all");
        if !["all", "stdout", "stderr"].contains(&stream) {
            return Err("日志输出类型无效".into());
        }
        Ok(format!(
            "/containers/{id}/logs?stdout={}&stderr={}&timestamps=true&tail={tail}&follow={follow}{}{}",
            stream != "stderr",
            stream != "stdout",
            self.since
                .map(|v| format!("&since={v}"))
                .unwrap_or_default(),
            self.until
                .map(|v| format!("&until={v}"))
                .unwrap_or_default()
        ))
    }
}
struct Entries {
    decoder: engine::Decoder,
    buffers: std::collections::HashMap<String, Vec<u8>>,
}
impl Entries {
    fn new(tty: bool) -> Self {
        Self {
            decoder: engine::Decoder::new(tty),
            buffers: Default::default(),
        }
    }
    fn push(&mut self, bytes: &[u8]) -> Result<Vec<Value>, String> {
        let mut entries = vec![];
        for (stream, bytes) in self.decoder.push(bytes)? {
            let buffer = self.buffers.entry(stream.clone()).or_default();
            buffer.extend(bytes);
            while let Some(n) = buffer
                .iter()
                .position(|v| *v == b'\n')
                .map(|n| n + 1)
                .or_else(|| (buffer.len() >= 65536).then_some(65536))
            {
                entries.push(entry(&stream, &buffer[..n]));
                buffer.drain(..n);
            }
        }
        Ok(entries)
    }
    fn finish(&mut self) -> Vec<Value> {
        self.buffers
            .drain()
            .filter(|(_, v)| !v.is_empty())
            .map(|(k, v)| entry(&k, &v))
            .collect()
    }
}
fn entry(stream: &str, bytes: &[u8]) -> Value {
    let text = String::from_utf8_lossy(bytes);
    let ansi = regex::Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").unwrap();
    let clean = ansi.replace_all(&text, "");
    let (timestamp, text) = clean
        .split_once(' ')
        .filter(|(time, _)| chrono::DateTime::parse_from_rfc3339(time).is_ok())
        .map(|(a, b)| (Some(a), b))
        .unwrap_or((None, &clean));
    json!({"stream":stream,"timestamp":timestamp,"text":text.trim_end_matches(['\r','\n'])})
}
async fn tty(id: &str) -> Result<bool, String> {
    // Config.Tty is needed to interpret Docker's stream framing; never return Config.Env.
    let item = engine::json("GET", &format!("/containers/{id}/json"), None).await?;
    Ok(item["Config"]["Tty"].as_bool().unwrap_or(false))
}
pub(super) async fn snapshot(
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
    Query(options): Query<Options>,
) -> Response {
    if let Err(e) = options.path(&id, false) {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_QUERY",
            &e,
            &context.request_id,
        );
    }
    let result = async {
        let path = options.path(&id, false)?;
        let tty = tty(&id).await?;
        let mut body = engine::request("GET", &path, None, false)
            .await?
            .into_body();
        let mut decoder = Entries::new(tty);
        let mut entries = vec![];
        let mut size = 0;
        let mut truncated = false;
        while let Some(frame) = tokio::time::timeout(Duration::from_secs(10), body.frame())
            .await
            .map_err(|_| "日志读取超时")?
        {
            let frame = frame.map_err(|e| e.to_string())?;
            if let Some(bytes) = frame.data_ref() {
                size += bytes.len();
                if size > 4 * 1024 * 1024 {
                    truncated = true;
                    break;
                }
                entries.extend(decoder.push(bytes)?);
            }
        }
        entries.extend(decoder.finish());
        let text = entries
            .iter()
            .map(|e| {
                format!(
                    "{} [{}] {}",
                    e["timestamp"].as_str().unwrap_or(""),
                    e["stream"].as_str().unwrap_or(""),
                    e["text"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        Ok::<_, String>((
            json!({"entries":entries,"text":text,"tty":tty,"truncated":truncated}),
            text,
        ))
    }
    .await;
    match result {
        Ok((_data, text)) if options.download => (
            [
                (
                    header::CONTENT_TYPE,
                    "text/plain; charset=utf-8".to_string(),
                ),
                (
                    header::CONTENT_DISPOSITION,
                    format!("attachment; filename=\"container-{}.log\"", &id[..12]),
                ),
            ],
            text,
        )
            .into_response(),
        Ok((data, _)) => ok(data).into_response(),
        Err(e) => failure(
            StatusCode::BAD_GATEWAY,
            "LOGS_FAILED",
            &e,
            &context.request_id,
        ),
    }
}
pub(super) async fn live(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(options): Query<Options>,
    ws: WebSocketUpgrade,
) -> Response {
    if !headers.contains_key("origin") || crate::platform_versions::check_origin(&headers).is_err()
    {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            "日志连接来源不允许",
            &context.request_id,
        );
    }
    let path = match options.path(&id, true) {
        Ok(v) => v,
        Err(e) => {
            return failure(
                StatusCode::BAD_REQUEST,
                "INVALID_QUERY",
                &e,
                &context.request_id,
            );
        }
    };
    let tty = match tty(&id).await {
        Ok(v) => v,
        Err(e) => {
            return failure(
                StatusCode::BAD_GATEWAY,
                "LOGS_FAILED",
                &e,
                &context.request_id,
            );
        }
    };
    // At most 16 simultaneous live log streams across this API process.
    static LIMIT: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(16);
    let Ok(permit) = LIMIT.try_acquire() else {
        return failure(
            StatusCode::TOO_MANY_REQUESTS,
            "LOGS_BUSY",
            "实时日志连接已达上限",
            &context.request_id,
        );
    };
    ws.max_message_size(1024).on_upgrade(move|mut socket|async move{
        let _permit=permit;
        let result=async{
            let mut body=engine::request("GET",&path,None,false).await?.into_body();let mut decoder=Entries::new(tty);
            let mut tick=tokio::time::interval(Duration::from_secs(15));let started=Instant::now();let mut total=0usize;
            socket.send(Message::Text(json!({"type":"connected","tty":tty}).to_string().into())).await.map_err(|_|"浏览器已断开")?;
            loop{tokio::select!{
                frame=body.frame()=>{
                    match frame{
                        Some(Ok(frame))=>if let Some(bytes)=frame.data_ref(){total+=bytes.len();if total>64*1024*1024{return Err("单次日志流已达 64 MiB，请重新连接".into())}for e in decoder.push(bytes)?{tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Text(json!({"type":"entry","entry":e}).to_string().into()))).await.map_err(|_|"日志接收超时")?.map_err(|_|"浏览器已断开")?;}},
                        Some(Err(e))=>return Err(e.to_string()),None=>break,
                    }
                },
                message=socket.recv()=>match message{Some(Ok(Message::Ping(v)))=>{socket.send(Message::Pong(v)).await.map_err(|_|"浏览器已断开")?;},Some(Ok(Message::Pong(_)))=>{},_=>break},
                _=tick.tick()=>{
                    let user=crate::require_authenticated(&headers,&state).await.map_err(|_|"登录已失效")?;
                    if user.role!="platform_admin"{return Err("管理员权限已撤销".into())}
                    if started.elapsed().as_secs()>3600{return Err("日志连接已达 1 小时，请重新连接".into())}
                    inspect(&[id.clone()]).await?;
                    tokio::time::timeout(Duration::from_secs(5),socket.send(Message::Ping(vec![].into()))).await.map_err(|_|"日志接收超时")?.map_err(|_|"浏览器已断开")?;
                }
            }}
            for e in decoder.finish(){let _=socket.send(Message::Text(json!({"type":"entry","entry":e}).to_string().into())).await;}
            Ok::<_,String>(())
        }.await;
        let _=socket.send(Message::Text(json!({"type":"closed","reason":result.err().unwrap_or("日志流已结束".into())}).to_string().into())).await;
        let _=socket.close().await;
    }).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_times_and_stream_and_preserves_utf8() {
        assert!(
            Options {
                since: Some(10),
                until: Some(1),
                ..Default::default()
            }
            .path(&"a".repeat(64), false)
            .is_err()
        );
        assert!(
            Options {
                stream: Some("cmd".into()),
                ..Default::default()
            }
            .path(&"a".repeat(64), true)
            .is_err()
        );
        let mut e = Entries::new(true);
        let bytes = "2026-11-10T00:00:00Z 中文\n".as_bytes();
        assert!(e.push(&bytes[..22]).unwrap().is_empty());
        let values = e.push(&bytes[22..]).unwrap();
        assert_eq!(values[0]["text"], "中文");
        assert_eq!(values[0]["stream"], "combined");
    }
}
