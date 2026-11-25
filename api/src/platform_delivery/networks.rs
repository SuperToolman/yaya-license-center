use super::*;
use serde_json::{Value, json};

pub(crate) fn validate(mode: &str, name: &str) -> Result<(), Failure> {
    if !["bridge", "macvlan"].contains(&mode) {
        return Err(bad(
            "当前平台镜像仅支持 bridge / macvlan 网络，host 会共享内置服务固定端口，暂不支持",
        ));
    }
    if name.len() > 128
        || !regex::Regex::new(r"^[a-zA-Z0-9][a-zA-Z0-9_.-]*$")
            .unwrap()
            .is_match(name)
    {
        return Err(bad("请选择有效的 Docker 网络"));
    }
    Ok(())
}
async fn inspect(name: &str) -> Result<Value, String> {
    let text=crate::local_docker::read(&["network","inspect","--format",r#"{"name":{{json .Name}},"driver":{{json .Driver}},"scope":{{json .Scope}},"internal":{{json .Internal}},"ipam":{{json .IPAM.Config}}}"#,name],10,262144).await?;
    serde_json::from_str(&text).map_err(|_| "Docker 网络信息格式无效".into())
}
pub(crate) async fn verify(mode: &str, name: &str) -> Result<(), String> {
    validate(mode, name).map_err(|_| "网络模式或网络名称无效".to_string())?;
    let item = inspect(name).await?;
    if item["driver"] != mode || item["scope"] != "local" {
        return Err("Docker 网络驱动或作用域已变化，请重新选择网络".into());
    }
    Ok(())
}
pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Value>>, Failure> {
    admin(&headers, &state).await?;
    let names = crate::local_docker::read(&["network", "ls", "--format", "{{.Name}}"], 10, 262144)
        .await
        .map_err(|e| error(StatusCode::SERVICE_UNAVAILABLE, e))?;
    let mut items = vec![];
    for name in names.lines().take(100) {
        let item = inspect(name)
            .await
            .map_err(|e| error(StatusCode::SERVICE_UNAVAILABLE, e))?;
        if ["bridge", "macvlan"].contains(&item["driver"].as_str().unwrap_or(""))
            && item["scope"] == "local"
        {
            items.push(json!({"name":item["name"],"driver":item["driver"],"internal":item["internal"],"subnets":item["ipam"].as_array().map(|v|v.iter().filter_map(|i|i["Subnet"].as_str()).collect::<Vec<_>>()).unwrap_or_default()}));
        }
    }
    Ok(Json(success(
        "本机 Docker 网络",
        json!({"items":items,"hostSupported":false,"hostReason":"当前离线镜像内置数据库、缓存等固定端口服务，暂不支持 host 共享网络"}),
    )))
}
