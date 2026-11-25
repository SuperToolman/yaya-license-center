//! Generic local container creation. Only existing images and named volumes; no bind mounts or privileged flags.
use super::*;
use axum::http::HeaderMap;
use rusqlite::OptionalExtension;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    creation_id: String,
    name: String,
    image_id: String,
    network_mode: String,
    network_name: String,
    #[serde(default)]
    ports: Vec<Port>,
    #[serde(default)]
    volumes: Vec<Volume>,
    #[serde(default)]
    environment: Vec<String>,
    restart_policy: String,
    start: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Port {
    host_port: u16,
    container_port: u16,
    protocol: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Volume {
    name: String,
    destination: String,
    #[serde(default)]
    read_only: bool,
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$")
            .unwrap()
            .is_match(name)
}
fn validate(v: &Input) -> Result<(), String> {
    if uuid::Uuid::parse_str(&v.creation_id).is_err() || !valid_name(&v.name) {
        return Err("创建标识或容器名称无效".into());
    }
    if v.name.contains("yaya-operation-center") {
        return Err("不能使用运营平台的保留容器名称".into());
    }
    if !v
        .image_id
        .strip_prefix("sha256:")
        .is_some_and(local_containers::valid_id)
    {
        return Err("请选择当前 Docker 已有的镜像".into());
    }
    if v.network_mode == "host" {
        if v.network_name != "host" || !v.ports.is_empty() {
            return Err("host 网络不支持端口映射".into());
        }
    } else {
        crate::platform_delivery::networks::validate(&v.network_mode, &v.network_name)
            .map_err(|_| "网络模式或名称无效")?;
    }
    if !["no", "unless-stopped", "always", "on-failure"].contains(&v.restart_policy.as_str()) {
        return Err("重启策略无效".into());
    }
    if v.ports.len() > 32 || v.volumes.len() > 16 || v.environment.len() > 128 {
        return Err("端口、存储或环境变量条目过多".into());
    }
    let mut ports = std::collections::HashSet::new();
    let mut targets = std::collections::HashSet::new();
    for p in &v.ports {
        if p.host_port < 1024
            || p.container_port == 0
            || !["tcp", "udp"].contains(&p.protocol.as_str())
            || !ports.insert((p.host_port, p.protocol.as_str()))
            || !targets.insert((p.container_port, p.protocol.as_str()))
        {
            return Err("端口范围、协议无效或存在重复映射；宿主端口需为 1024–65535".into());
        }
    }
    let mut destinations = std::collections::HashSet::new();
    for volume in &v.volumes {
        if !valid_name(&volume.name)
            || volume.name.starts_with("yaya-")
            || !volume.destination.starts_with('/')
            || volume.destination == "/"
            || volume.destination.len() > 512
            || volume.destination.split('/').any(|s| s == ".." || s == ".")
            || volume
                .destination
                .chars()
                .any(|c| c.is_control() || c == '\\')
            || !destinations.insert(volume.destination.trim_end_matches('/'))
        {
            return Err("请选择有效的独立命名卷和容器绝对路径；yaya- 前缀保留给客户实例".into());
        }
    }
    let mut names = std::collections::HashSet::new();
    let mut size = 0;
    for entry in &v.environment {
        size += entry.len();
        let (name, _) = entry
            .split_once('=')
            .ok_or("环境变量须使用 NAME=value 格式")?;
        if !regex::Regex::new(r"^[a-zA-Z_][a-zA-Z0-9_]{0,127}$")
            .unwrap()
            .is_match(name)
            || !names.insert(name)
            || entry.chars().any(|c| matches!(c, '\0' | '\n' | '\r'))
        {
            return Err("环境变量名无效、重复或包含换行".into());
        }
    }
    if size > 32768 {
        return Err("环境变量不得超过 32 KiB".into());
    }
    Ok(())
}
fn body(v: &Input) -> Value {
    let mut bindings = serde_json::Map::new();
    let mut exposed = serde_json::Map::new();
    for p in &v.ports {
        let key = format!("{}/{}", p.container_port, p.protocol);
        bindings.insert(
            key.clone(),
            json!([{"HostIp":"0.0.0.0","HostPort":p.host_port.to_string()}]),
        );
        exposed.insert(key, json!({}));
    }
    json!({"Image":v.image_id,"Env":v.environment,"Labels":{"com.yaya.creation-id":v.creation_id},"ExposedPorts":exposed,
        "HostConfig":{"NetworkMode":v.network_name,"RestartPolicy":{"Name":v.restart_policy},"PortBindings":bindings,
        "Mounts":v.volumes.iter().map(|m|json!({"Type":"volume","Source":m.name,"Target":m.destination,"ReadOnly":m.read_only})).collect::<Vec<_>>()}})
}
async fn saved(state: &AppState, id: String) -> Result<Value, String> {
    db::blocking(state.database_path.clone(),move |db|db.query_row("SELECT id,name,container_id,status,error,created_at_ms,finished_at_ms FROM ops_container_creations WHERE id=?",[id],|r| Ok(json!({"creationId":r.get::<_,String>(0)?,"name":r.get::<_,String>(1)?,"containerId":r.get::<_,Option<String>>(2)?,"status":r.get::<_,String>(3)?,"error":r.get::<_,Option<String>>(4)?,"createdAtMs":r.get::<_,i64>(5)?,"finishedAtMs":r.get::<_,Option<i64>>(6)?}))).map_err(|e|e.to_string())).await
}
pub(super) async fn task(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    match saved(&state, id).await {
        Ok(v) => ok(v).into_response(),
        Err(_) => failure(
            StatusCode::NOT_FOUND,
            "CREATION_MISSING",
            "创建记录不存在",
            &context.request_id,
        ),
    }
}
pub(super) async fn create(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Response {
    if !headers.contains_key("origin") || crate::platform_versions::check_origin(&headers).is_err()
    {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_DENIED",
            "操作来源不允许",
            &context.request_id,
        );
    }
    if let Err(e) = validate(&input) {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_CONTAINER",
            &e,
            &context.request_id,
        );
    }
    // Reserve a durable id before contacting Docker. Retrying this id never recreates a container.
    let id = input.creation_id.clone();
    let name = input.name.clone();
    let image = input.image_id.clone();
    let actor = context.user.user_id.clone();
    let reserved=db::blocking(state.database_path.clone(),move|db| {
        let tx=db.transaction().map_err(|e|e.to_string())?;
        if tx.query_row("SELECT id FROM ops_container_creations WHERE id=?",[&id],|r|r.get::<_,String>(0)).optional().map_err(|e|e.to_string())?.is_some() {return Ok(false);}
        tx.execute("INSERT INTO ops_container_creations(id,name,image_id,actor_user_id,status,created_at_ms) VALUES(?,?,?,?,'running',?)",params![id,name,image,actor,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())?;Ok(true)
    }).await;
    match reserved {
        Ok(false) => {
            return match saved(&state, input.creation_id).await {
                Ok(v) => ok(v).into_response(),
                Err(_) => failure(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "CREATION_READ_FAILED",
                    "无法读取创建任务",
                    &context.request_id,
                ),
            };
        }
        Err(_) => {
            return failure(
                StatusCode::CONFLICT,
                "CREATION_BUSY",
                "同名容器正在创建或待核对，请检查创建记录和 Docker 实际状态",
                &context.request_id,
            );
        }
        _ => {}
    }
    let id = input.creation_id.clone();
    let result_id = id.clone();
    tokio::spawn(async move {
        let mut container_id = None;
        let result = async {
            let user = crate::require_authenticated(&headers, &state)
                .await
                .map_err(|_| "登录会话失效，创建已取消")?;
            if user.role != "platform_admin" {
                return Err("当前账号无容器创建权限".to_string());
            }
            if input.network_mode != "host" {
                crate::platform_delivery::networks::verify(
                    &input.network_mode,
                    &input.network_name,
                )
                .await?;
            }
            let image =
                engine::json("GET", &format!("/images/{}/json", input.image_id), None).await?;
            if image["Id"] != input.image_id {
                return Err("镜像已变化，请重新选择".into());
            }
            let created = engine::json(
                "POST",
                &format!("/containers/create?name={}", input.name),
                Some(body(&input)),
            )
            .await?;
            let target = created["Id"]
                .as_str()
                .filter(|v| local_containers::valid_id(v))
                .ok_or("Docker 未返回有效容器 ID，请核对实际状态")?
                .to_string();
            container_id = Some(target.clone());
            let cid = target.clone();
            let key = id.clone();
            db::blocking(state.database_path.clone(), move |db| {
                db.execute(
                    "UPDATE ops_container_creations SET container_id=? WHERE id=?",
                    params![cid, key],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
            .await?;
            if input.start {
                engine::json("POST", &format!("/containers/{target}/start"), None).await?;
            }
            Ok::<(), String>(())
        }
        .await;
        let status = if result.is_ok() {
            "succeeded"
        } else if container_id.is_some() {
            "start_failed"
        } else {
            "failed"
        };
        let secrets: Vec<String> = input
            .environment
            .iter()
            .filter_map(|s| s.split_once('=').map(|(_, v)| v.to_owned()))
            .filter(|s| !s.is_empty())
            .collect();
        let error = result.err().map(|mut e| {
            for secret in &secrets {
                e = e.replace(secret, "[REDACTED]");
            }
            crate::platform_delivery::docker_diagnostic(e.as_bytes(), &[])
        });
        let key = id.clone();
        let _=db::blocking(state.database_path.clone(),move|db| {
            let tx=db.transaction().map_err(|e|e.to_string())?;
            tx.execute("UPDATE ops_container_creations SET container_id=?,status=?,error=?,finished_at_ms=? WHERE id=?",params![container_id,status,error,chrono::Utc::now().timestamp_millis(),key]).map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES(?,?,?,?,?,?,?)",params![uuid::Uuid::new_v4().to_string(),context.user.user_id,format!("container.create.{status}"),input.name,if status=="succeeded"{200}else{500},context.request_id,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())
        }).await;
    });
    (
        StatusCode::ACCEPTED,
        ok(json!({"creationId":result_id,"status":"running"})),
    )
        .into_response()
}

/// Generic operations require both persisted full ID and our creation label; customer binding remains separate.
pub(super) async fn managed(state: &AppState, item: &Value) -> bool {
    let Some(id) = item["id"].as_str().map(str::to_owned) else {
        return false;
    };
    let Some(key) = item["creationId"].as_str().map(str::to_owned) else {
        return false;
    };
    db::blocking(state.database_path.clone(),move |db|db.query_row("SELECT EXISTS(SELECT 1 FROM ops_container_creations WHERE id=? AND container_id=? AND status IN ('succeeded','start_failed','interrupted'))",params![key,id],|r|r.get::<_,bool>(0)).map_err(|e|e.to_string())).await.unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Input {
        serde_json::from_value(json!({"creationId":uuid::Uuid::new_v4().to_string(),"name":"demo","imageId":format!("sha256:{}","a".repeat(64)),"networkMode":"bridge","networkName":"bridge","ports":[{"hostPort":18080,"containerPort":80,"protocol":"tcp"}],"volumes":[{"name":"demo-data","destination":"/data"}],"environment":["SECRET=a=b"],"restartPolicy":"unless-stopped","start":true})).unwrap()
    }
    #[test]
    fn validation_and_engine_request_do_not_allow_privileged_or_business_bindings() {
        let mut v = input();
        assert!(validate(&v).is_ok());
        let b = body(&v);
        assert_eq!(
            b["HostConfig"]["PortBindings"]["80/tcp"][0]["HostPort"],
            "18080"
        );
        assert_eq!(b["Env"][0], "SECRET=a=b");
        assert!(b["Labels"].get("com.yaya.instance-id").is_none());
        assert!(b["HostConfig"].get("Privileged").is_none());
        v.volumes[0].name = "yaya-customer-postgres".into();
        assert!(validate(&v).is_err());
        v = input();
        v.volumes[0].destination = "/data/../etc".into();
        assert!(validate(&v).is_err());
        v = input();
        v.network_mode = "host".into();
        v.network_name = "host".into();
        assert!(validate(&v).is_err());
        v.ports.clear();
        assert!(validate(&v).is_ok());
        v = input();
        v.environment.push("SECRET=duplicate".into());
        assert!(validate(&v).is_err());
        let mut bad = body(&input());
        bad["privileged"] = json!(true);
        assert!(serde_json::from_value::<Input>(bad).is_err());
    }
}
