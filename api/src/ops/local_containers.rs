use super::*;
use crate::local_docker::read;
use axum::{extract::Path, http::HeaderMap};
use serde_json::{Value, json};
use yaya_ops_protocol::CONTAINER_LIMIT;

fn lines(text: &str) -> Result<Vec<Value>, String> {
    text.lines()
        .filter(|v| !v.trim().is_empty())
        .map(|v| serde_json::from_str(v).map_err(|_| "Docker 返回了无效的数据".into()))
        .collect()
}
pub(super) fn valid_id(id: &str) -> bool {
    id.len() == 64 && id.bytes().all(|v| v.is_ascii_hexdigit())
}
fn percent(value: &Value) -> Option<f64> {
    value.as_str()?.trim_end_matches('%').parse().ok()
}
const INSPECT: &str = r#"{"id":{{json .Id}},"name":{{json .Name}},"image":{{json .Config.Image}},"state":{{json .State.Status}},"createdAt":{{json .Created}},"ports":{{json .NetworkSettings.Ports}},"networks":{{json .NetworkSettings.Networks}},"project":{{json (index .Config.Labels "com.docker.compose.project")}},"service":{{json (index .Config.Labels "com.docker.compose.service")}},"instanceId":{{json (index .Config.Labels "com.yaya.instance-id")}},"creationId":{{json (index .Config.Labels "com.yaya.creation-id")}},"exitCode":{{.State.ExitCode}},"health":{{with (index .State "Health")}}{{json .Status}}{{else}}null{{end}},"startedAt":{{json .State.StartedAt}},"finishedAt":{{json .State.FinishedAt}},"mounts":{{json .Mounts}},"protectedLabel":{{json (index .Config.Labels "com.yaya.protected")}}}"#;
// These fields do not contain environment variables, command lines or registry credentials.
pub(crate) async fn inspect(ids: &[String]) -> Result<Vec<Value>, String> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let mut args = vec!["container", "inspect", "--format", INSPECT];
    args.extend(ids.iter().map(String::as_str));
    lines(&read(&args, 15, 4 * 1024 * 1024).await?)
}
pub(crate) fn protected(item: &Value) -> bool {
    let name = item["name"].as_str().unwrap_or("").trim_start_matches('/');
    let image = item["image"].as_str().unwrap_or("");
    item["protected"] == true
        || item["protectedLabel"] == "true"
        || name.contains("yaya-operation-center")
        || image.contains("yaya-operation-center")
        || std::env::var("HOSTNAME")
            .ok()
            .filter(|v| v.len() >= 12)
            .is_some_and(|v| item["id"].as_str().unwrap_or("").starts_with(&v))
        || std::env::var("YAYA_DOCKER_PROTECTED_CONTAINERS")
            .unwrap_or_default()
            .split(',')
            .filter(|v| !v.trim().is_empty())
            .any(|v| v.trim() == name || item["id"] == v.trim())
}
fn operations(state: &str) -> Vec<&'static str> {
    match state {
        "running" => vec!["stop", "restart", "kill", "pause"],
        "paused" => vec!["unpause", "stop", "kill"],
        "exited" | "created" => vec!["start"],
        "restarting" => vec!["stop", "kill"],
        _ => vec![],
    }
}
fn display(item: Value, stat: Option<&Value>) -> Value {
    let mut addresses = Vec::new();
    if let Some(networks) = item["networks"].as_object() {
        for (name, network) in networks {
            for key in ["IPAddress", "GlobalIPv6Address"] {
                if let Some(ip) = network[key].as_str().filter(|s| !s.is_empty()) {
                    addresses.push(json!({"network":name,"address":ip}));
                }
            }
        }
    }
    let mut ports = Vec::new();
    if let Some(bindings) = item["ports"].as_object() {
        for (port, values) in bindings {
            if let Some(values) = values.as_array() {
                for value in values {
                    ports.push(json!({"containerPort":port,"hostIp":value["HostIp"],"hostPort":value["HostPort"]}));
                }
            } else {
                ports.push(json!({"containerPort":port,"hostIp":null,"hostPort":null}));
            }
        }
    }
    let is_protected = protected(&item);
    let created = item["createdAt"]
        .as_str()
        .and_then(|v| chrono::DateTime::parse_from_rfc3339(v).ok())
        .map(|v| v.timestamp_millis());
    json!({"id":item["id"],"name":item["name"].as_str().unwrap_or("").trim_start_matches('/'),"image":item["image"],"state":item["state"],"createdAtMs":created,
      "project":item["project"],"service":item["service"],"addresses":addresses,"ports":ports,"protected":is_protected,
      "instanceId":item["instanceId"],"creationId":item["creationId"],"exitCode":item["exitCode"],"health":item["health"],"startedAt":item["startedAt"],"finishedAt":item["finishedAt"],
      "mounts":item["mounts"].as_array().map(|values|values.iter().map(|v|json!({"type":v["Type"],"name":v["Name"],"destination":v["Destination"],"rw":v["RW"]})).collect::<Vec<_>>()),
      "allowedActions":if is_protected {vec![]} else {operations(item["state"].as_str().unwrap_or(""))},
      "cpuPercent":stat.and_then(|v|percent(&v["CPUPerc"])),"memoryPercent":stat.and_then(|v|percent(&v["MemPerc"])),"memoryUsage":stat.and_then(|v|v["MemUsage"].as_str()),"networkIO":stat.and_then(|v|v["NetIO"].as_str())})
}
async fn containers() -> Result<Value, String> {
    let ids = read(&["container", "ls", "-aq", "--no-trunc"], 10, 1024 * 1024).await?;
    let all: Vec<String> = ids
        .lines()
        .filter(|v| valid_id(v))
        .map(str::to_owned)
        .collect();
    let selected: Vec<String> = all.iter().take(CONTAINER_LIMIT).cloned().collect();
    let inspected = inspect(&selected).await?;
    let running: Vec<&str> = inspected
        .iter()
        .filter(|v| v["state"] == "running")
        .filter_map(|v| v["id"].as_str())
        .collect();
    let mut stats_error = None;
    let stats = if running.is_empty() {
        vec![]
    } else {
        let mut args = vec![
            "stats",
            "--no-stream",
            "--no-trunc",
            "--format",
            "{{json .}}",
        ];
        args.extend(running);
        match read(&args, 12, 4 * 1024 * 1024)
            .await
            .and_then(|text| lines(&text))
        {
            Ok(values) => values,
            Err(e) => {
                stats_error = Some(e);
                vec![]
            }
        }
    };
    let mut items: Vec<Value> = inspected
        .into_iter()
        .map(|item| {
            let stat = stats.iter().find(|v| v["Container"] == item["id"]);
            display(item, stat)
        })
        .collect();
    items.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    Ok(
        json!({"items":items,"total":all.len(),"limit":CONTAINER_LIMIT,"truncated":all.len()>CONTAINER_LIMIT,"statsError":stats_error,"capturedAtMs":chrono::Utc::now().timestamp_millis()}),
    )
}
pub(super) async fn list(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
) -> Response {
    match containers().await {
        Ok(mut data) => {
            if let Some(items) = data["items"].as_array_mut() {
                for item in items {
                    attach(&state, item).await;
                }
            }
            ok(data).into_response()
        }
        Err(e) => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "DOCKER_UNAVAILABLE",
            &e,
            &context.request_id,
        ),
    }
}
async fn resource(args: &[&str]) -> Result<Vec<Value>, String> {
    lines(&read(args, 15, 8 * 1024 * 1024).await?)
}
pub(super) async fn inventory(Extension(context): Extension<RequestContext>) -> Response {
    let result=async {
        let info:Value=serde_json::from_str(&read(&["info","--format",r#"{"serverVersion":{{json .ServerVersion}},"operatingSystem":{{json .OperatingSystem}},"architecture":{{json .Architecture}},"cpus":{{.NCPU}},"memory":{{.MemTotal}}}"#],10,65536).await?).map_err(|_|"Docker 配置读取失败".to_string())?;
        let (images,networks,volumes)=tokio::try_join!(
            resource(&["image","ls","--no-trunc","--format","{{json .}}"]),
            resource(&["network","ls","--no-trunc","--format","{{json .}}"]),
            resource(&["volume","ls","--format","{{json .}}"])
        )?;
        let disk=resource(&["system","df","--format","{{json .}}"] ).await;
        Ok::<_,String>(json!({"info":info,"images":images.iter().map(|v|json!({"id":v["ID"],"repository":v["Repository"],"tag":v["Tag"],"size":v["Size"],"created":v["CreatedSince"]})).collect::<Vec<_>>(),
         "networks":networks.iter().map(|v|json!({"id":v["ID"],"name":v["Name"],"driver":v["Driver"],"scope":v["Scope"]})).collect::<Vec<_>>(),
         "volumes":volumes.iter().map(|v|json!({"name":v["Name"],"driver":v["Driver"],"scope":v["Scope"]})).collect::<Vec<_>>(),
         "disk":disk.as_ref().ok(),"diskError":disk.err(),"capturedAtMs":chrono::Utc::now().timestamp_millis()}))
    }.await;
    match result {
        Ok(v) => ok(v).into_response(),
        Err(e) => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "DOCKER_UNAVAILABLE",
            &e,
            &context.request_id,
        ),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ActionInput {
    action: String,
    expected_state: String,
}
pub(super) async fn action(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<ActionInput>,
) -> Response {
    if let Err((status, _)) = crate::platform_versions::check_origin(&headers) {
        return failure(
            status,
            "ORIGIN_DENIED",
            "操作来源不允许",
            &context.request_id,
        );
    }
    if !headers.contains_key("origin") {
        return failure(
            StatusCode::FORBIDDEN,
            "ORIGIN_REQUIRED",
            "操作缺少来源校验",
            &context.request_id,
        );
    }
    if !valid_id(&id)
        || !["start", "stop", "restart", "kill", "pause", "unpause"]
            .contains(&input.action.as_str())
    {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_ACTION",
            "容器或操作无效",
            &context.request_id,
        );
    }
    let target = match inspect(&[id.clone()]).await {
        Ok(mut v) if v.len() == 1 => v.remove(0),
        _ => {
            return failure(
                StatusCode::NOT_FOUND,
                "TARGET_MISSING",
                "容器不存在或 Docker 不可用",
                &context.request_id,
            );
        }
    };
    if !controllable(&state, &target).await {
        return failure(
            StatusCode::FORBIDDEN,
            "UNMANAGED_CONTAINER",
            "客户容器的实例绑定无效、已归档或正在部署，暂不可操作",
            &context.request_id,
        );
    }
    if protected(&target) {
        return failure(
            StatusCode::FORBIDDEN,
            "PROTECTED_CONTAINER",
            "此容器是受保护的管理组件，禁止操作",
            &context.request_id,
        );
    }
    if target["state"] != input.expected_state
        || !operations(&input.expected_state).contains(&input.action.as_str())
    {
        return failure(
            StatusCode::CONFLICT,
            "STATE_CHANGED",
            "容器状态已变化，请刷新后重新操作",
            &context.request_id,
        );
    }
    let task_id = uuid::Uuid::new_v4().to_string();
    let saved_task = task_id.clone();
    let saved_id = id.clone();
    let saved_action = input.action.clone();
    let actor = context.user.user_id.clone();
    let saved_request = context.request_id.clone();
    let queued=db::blocking(state.database_path.clone(),move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(|e|e.to_string())?;
        if let Ok(instance)=tx.query_row("SELECT instance_id FROM platform_instances WHERE container_id=?",[&saved_id],|r|r.get::<_,String>(0)) {crate::platform_delivery::lifecycle::ensure_idle(&tx,&instance)?;}
        tx.execute("INSERT INTO ops_container_actions(id,container_id,action,actor_user_id,status,created_at_ms) VALUES(?,?,?,?,'running',?)",params![saved_task,saved_id,saved_action,actor,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES(?,?,?,?,202,?,?)",params![uuid::Uuid::new_v4().to_string(),actor,format!("container.{}",saved_action),saved_id,saved_request,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }).await;
    if queued.is_err() {
        return failure(
            StatusCode::CONFLICT,
            "ACTION_BUSY",
            "此容器已有操作进行中，或操作记录无法保存",
            &context.request_id,
        );
    }
    let result_id = task_id.clone();
    tokio::spawn(async move {
        let result = async {
            let user = crate::require_authenticated(&headers, &state)
                .await
                .map_err(|_| "登录会话失效，操作已取消".to_string())?;
            if user.role != "platform_admin" {
                return Err("当前账号无容器操作权限".into());
            }
            let target = inspect(&[id.clone()])
                .await?
                .into_iter()
                .next()
                .ok_or("目标容器不存在")?;
            if !controllable(&state, &target).await
                || protected(&target)
                || target["state"] != input.expected_state
            {
                return Err("目标容器状态或保护规则已变化，请刷新核对".into());
            }
            read(&["container", &input.action, &id], 45, 65536).await?;
            let actual = inspect(&[id.clone()])
                .await?
                .into_iter()
                .next()
                .ok_or("目标容器不存在")?;
            let expected = match input.action.as_str() {
                "start" | "restart" | "unpause" => "running",
                "pause" => "paused",
                _ => "exited",
            };
            if actual["state"] != expected {
                return Err(format!(
                    "命令已返回，但容器实际状态为 {}，请检查容器日志",
                    actual["state"]
                ));
            }
            Ok::<_, String>(())
        }
        .await;
        let _ =
            crate::platform_delivery::runtime_state::reconcile(state.database_path.clone()).await;
        let status = if result.is_ok() {
            "succeeded"
        } else {
            "failed"
        };
        let error = result.err();
        let _=db::blocking(state.database_path.clone(),move|db|{
            let tx=db.transaction().map_err(|e|e.to_string())?;
            tx.execute("UPDATE ops_container_actions SET status=?,error=?,finished_at_ms=? WHERE id=?",params![status,error,chrono::Utc::now().timestamp_millis(),task_id]).map_err(|e|e.to_string())?;
            tx.execute("INSERT INTO ops_audit_events(id,actor_user_id,action,resource_path,outcome_status,request_id,created_at_ms) VALUES(?,?,?,?,?,?,?)",params![uuid::Uuid::new_v4().to_string(),context.user.user_id,format!("container.{}.{}",input.action,status),id,if status=="succeeded"{200}else{500},context.request_id,chrono::Utc::now().timestamp_millis()]).map_err(|e|e.to_string())?;
            tx.commit().map_err(|e|e.to_string())
        }).await;
    });
    (StatusCode::ACCEPTED, ok(json!({"taskId":result_id}))).into_response()
}
pub(super) async fn task(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    match db::blocking(state.database_path.clone(),move|db|{
        db.query_row("SELECT id,container_id,action,status,error,created_at_ms,finished_at_ms,reconciliation FROM ops_container_actions WHERE id=?",[id],|r|Ok(json!({"reconciliation":r.get::<_,Option<String>>(7)?.and_then(|v|serde_json::from_str::<Value>(&v).ok()),"taskId":r.get::<_,String>(0)?,"containerId":r.get::<_,String>(1)?,"action":r.get::<_,String>(2)?,"status":r.get::<_,String>(3)?,"error":r.get::<_,Option<String>>(4)?,"createdAtMs":r.get::<_,i64>(5)?,"finishedAtMs":r.get::<_,Option<i64>>(6)?}))).map_err(|e|e.to_string())
    }).await {Ok(v)=>ok(v).into_response(),Err(_)=>failure(StatusCode::NOT_FOUND,"TASK_MISSING","操作记录不存在",&context.request_id)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resource_fields_do_not_include_secrets_and_protection_is_enforced() {
        let item = json!({"id":"a".repeat(64),"name":"/demo","image":"demo:1","state":"running","createdAt":"2026-11-09T10:11:12Z","networks":{"bridge":{"IPAddress":"172.17.0.2"}},"ports":{"80/tcp":[{"HostIp":"0.0.0.0","HostPort":"8080"}]},"Config":{"Env":["PASSWORD=secret"]}});
        let view = display(
            item.clone(),
            Some(&json!({"CPUPerc":"1.23%","MemPerc":"0.40%","MemUsage":"12MiB / 1GiB"})),
        );
        assert_eq!(view["cpuPercent"], 1.23);
        assert_eq!(view["ports"][0]["hostPort"], "8080");
        assert!(!view.to_string().contains("PASSWORD"));
        let mut protected_item = item;
        protected_item["name"] = json!("/yaya-operation-center");
        assert!(
            display(protected_item, None)["allowedActions"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert!(!operations("paused").contains(&"start"));
        assert!(valid_id(&"a".repeat(64)));
        assert!(!valid_id("--all"));
    }

    #[tokio::test]
    async fn writes_require_exact_database_and_label_binding_and_inactive_instances_are_readonly() {
        let path =
            std::env::temp_dir().join(format!("container-binding-{}.sqlite", uuid::Uuid::new_v4()));
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch("CREATE TABLE customers(customer_id TEXT,name TEXT); INSERT INTO customers VALUES('c','客户'); CREATE TABLE platform_instances(instance_id TEXT,name TEXT,customer_id TEXT,status TEXT,deployment_status TEXT,container_id TEXT); INSERT INTO platform_instances VALUES('i','实例','c','active','running','container');").unwrap();
        let state = AppState {
            signing_key: std::sync::Arc::new(jsonwebtoken::EncodingKey::from_secret(b"test")),
            verification_key: std::sync::Arc::new(jsonwebtoken::DecodingKey::from_secret(b"test")),
            database_path: path.clone(),
            sessions: std::sync::Arc::new(tokio::sync::RwLock::new(
                std::collections::HashMap::new(),
            )),
            ops: std::sync::Arc::new(OpsState::new(false, PathBuf::new())),
        };
        assert!(managed(&state, &json!({"id":"container","instanceId":"i"})).await);
        let mut external = display(
            json!({"id":"e".repeat(64),"name":"external","state":"running"}),
            None,
        );
        attach(&state, &mut external).await;
        assert_eq!(external["terminalAllowed"], true);
        assert!(
            external["allowedActions"]
                .as_array()
                .unwrap()
                .contains(&json!("stop"))
        );
        external["state"] = json!("exited");
        attach(&state, &mut external).await;
        assert_eq!(external["terminalAllowed"], false);
        let mut protected_external = display(
            json!({"id":"e".repeat(64),"name":"external","state":"running","protectedLabel":"true"}),
            None,
        );
        attach(&state, &mut protected_external).await;
        assert_eq!(protected_external["terminalAllowed"], false);
        assert_eq!(protected_external["allowedActions"], json!([]));
        let mut orphan = display(
            json!({"id":"o".repeat(64),"name":"orphan","instanceId":"missing","state":"running"}),
            None,
        );
        attach(&state, &mut orphan).await;
        assert_eq!(orphan["terminalAllowed"], false);
        assert_eq!(orphan["allowedActions"], json!([]));

        for item in [
            json!({"id":"other","instanceId":"i"}),
            json!({"id":"container","instanceId":"other"}),
            json!({"id":"container"}),
        ] {
            assert!(!managed(&state, &item).await);
        }
        db.execute(
            "UPDATE platform_instances SET deployment_status='deploying'",
            [],
        )
        .unwrap();
        assert!(!managed(&state, &json!({"id":"container","instanceId":"i"})).await);
        db.execute(
            "UPDATE platform_instances SET deployment_status='running',status='archived'",
            [],
        )
        .unwrap();
        let mut item = json!({"id":"container","instanceId":"i","allowedActions":["stop"]});
        attach(&state, &mut item).await;
        assert_eq!(item["association"]["customerName"], "客户");
        assert_eq!(item["allowedActions"], json!([]));
        drop(db);
        std::fs::remove_file(path).unwrap();
    }
}

async fn binding(state: &AppState, item: &Value) -> Option<Value> {
    let id = item["id"].as_str()?.to_string();
    let instance = item["instanceId"].as_str()?.to_string();
    db::blocking(state.database_path.clone(), move |db| {
        db.query_row("SELECT i.instance_id,i.name,i.customer_id,c.name,i.status,i.deployment_status FROM platform_instances i JOIN customers c ON c.customer_id=i.customer_id WHERE i.container_id=? AND i.instance_id=?", params![id,instance], |r| Ok(json!({"instanceId":r.get::<_,String>(0)?,"instanceName":r.get::<_,String>(1)?,"customerId":r.get::<_,String>(2)?,"customerName":r.get::<_,String>(3)?,"active":r.get::<_,String>(4)? != "archived" && r.get::<_,String>(5)? != "deploying"}))).map_err(|e|e.to_string())
    }).await.ok()
}
pub(super) async fn managed(state: &AppState, item: &Value) -> bool {
    binding(state, item)
        .await
        .is_some_and(|v| v["active"] == true)
}
pub(super) async fn controllable(state: &AppState, item: &Value) -> bool {
    // Customer-labelled containers must match the exact active instance binding.
    item["instanceId"].as_str().is_none_or(|s| s.is_empty()) || managed(state, item).await
}
async fn attach(state: &AppState, item: &mut Value) {
    let association = binding(state, item).await;
    let permitted = controllable(state, item).await && !protected(item);
    item["terminalAllowed"] = json!(permitted && item["state"] == "running");
    if !permitted {
        item["allowedActions"] = json!([]);
    }
    item["association"] = association.unwrap_or(Value::Null);
}
pub(super) async fn detail(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_TARGET",
            "容器 ID 无效",
            &context.request_id,
        );
    }
    match inspect(&[id]).await {
        Ok(values) if values.len() == 1 => {
            let mut item = display(values[0].clone(), None);
            attach(&state, &mut item).await;
            ok(item).into_response()
        }
        _ => failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "CONTAINER_DETAIL_FAILED",
            "容器不存在或 Docker 无法连接，请刷新核对",
            &context.request_id,
        ),
    }
}

pub(super) async fn history(
    State(state): State<AppState>,
    Extension(context): Extension<RequestContext>,
    Path(id): Path<String>,
) -> Response {
    if !valid_id(&id) {
        return failure(
            StatusCode::BAD_REQUEST,
            "INVALID_TARGET",
            "容器 ID 无效",
            &context.request_id,
        );
    }
    match db::blocking(state.database_path.clone(), move |db| {
        let mut q=db.prepare("SELECT id,action,status,error,created_at_ms,finished_at_ms,reconciliation FROM ops_container_actions WHERE container_id=? ORDER BY created_at_ms DESC LIMIT 30").map_err(|e|e.to_string())?;
        q.query_map([id],|r|Ok(json!({"taskId":r.get::<_,String>(0)?,"action":r.get::<_,String>(1)?,"status":r.get::<_,String>(2)?,"error":r.get::<_,Option<String>>(3)?,"createdAtMs":r.get::<_,i64>(4)?,"finishedAtMs":r.get::<_,Option<i64>>(5)?,"reconciliation":r.get::<_,Option<String>>(6)?.and_then(|v|serde_json::from_str::<Value>(&v).ok())}))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }).await {
        Ok(v)=>ok(v).into_response(),
        Err(_)=>failure(StatusCode::INTERNAL_SERVER_ERROR,"TASK_HISTORY_FAILED","操作记录读取失败",&context.request_id),
    }
}
