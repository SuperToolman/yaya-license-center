use serde_json::Value;
use std::{io, path::Path};
use yaya_ops_protocol::{CONTAINER_LIMIT, ContainerList, ContainerSummary, local::get_json};

pub async fn api_prefix(socket: &Path) -> io::Result<String> {
    let version: Value = get_json(socket, "/version").await?;
    let minor = version["ApiVersion"]
        .as_str()
        .and_then(|v| v.strip_prefix("1."))
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|v| *v >= 41)
        .ok_or_else(|| io::Error::other("Docker API 1.41 or newer required"))?;
    Ok(format!("/v1.{}", minor.min(47)))
}

pub fn summarize(values: Vec<Value>) -> ContainerList {
    let truncated = values.len() > CONTAINER_LIMIT;
    let mut items = values
        .into_iter()
        .filter_map(|value| {
            let id = value["Id"].as_str()?;
            Some(ContainerSummary {
                id: id.into(),
                name: value["Names"]
                    .as_array()
                    .and_then(|names| names.first())
                    .and_then(Value::as_str)
                    .unwrap_or(id)
                    .trim_start_matches('/')
                    .into(),
                image: value["Image"].as_str().unwrap_or("").into(),
                state: value["State"].as_str().unwrap_or("unknown").into(),
                status: value["Status"].as_str().unwrap_or("").into(),
                created_at_ms: value["Created"].as_i64().unwrap_or(0).saturating_mul(1000),
            })
        })
        .collect::<Vec<_>>();
    items.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    items.truncate(CONTAINER_LIMIT);
    ContainerList {
        items,
        limit: CONTAINER_LIMIT,
        truncated,
        captured_at_ms: crate::now_ms(),
    }
}

pub async fn list(socket: &Path) -> io::Result<ContainerList> {
    let prefix = api_prefix(socket).await?;
    let values: Vec<Value> =
        get_json(socket, &format!("{prefix}/containers/json?all=true")).await?;
    Ok(summarize(values))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whitelist_omits_secrets_and_bounds_sorted_results() {
        let values = (0..201).map(|i| serde_json::json!({"Id":format!("id{i}"),"Names":[format!("/container-{i:03}")],"Created":12,"Env":["PASSWORD=secret"],"Labels":{"secret":"secret"}})).collect();
        let list = summarize(values);
        assert_eq!(list.items.len(), 200);
        assert!(list.truncated);
        assert_eq!(list.items[0].created_at_ms, 12_000);
        assert!(!serde_json::to_string(&list).unwrap().contains("secret"));
    }
}
