//! Observations never overwrite deployment outcomes and never recreate containers or volumes.
use crate::{
    local_docker::read,
    ops::{db, local_containers::inspect},
};
use rusqlite::params;
use serde_json::{Value, json};
use std::path::PathBuf;

static SYNC: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn observation(instance: &str, item: Option<&Value>, reachable: bool) -> Value {
    let state = if !reachable {
        "unknown"
    } else {
        match item {
            None => "missing",
            Some(v) if v["instanceId"] != instance => "mismatch",
            Some(v) => v["state"].as_str().unwrap_or("unknown"),
        }
    };
    json!({"state":state,"checkedAt":chrono::Utc::now().timestamp_millis(),
    "health":item.and_then(|v|v["health"].as_str()),
    "exitCode":item.and_then(|v|v["exitCode"].as_i64()),
    "message":match state {
        "unknown"=>Some("无法确认容器状态：Docker 未连接或读取失败，稍后自动重试"),
        "missing"=>Some("已绑定的容器已被删除；数据卷不作任何变更，请检查后人工处理"),
        "mismatch"=>Some("容器实例标签与记录不一致，已禁止操作"),
        _=>None,
    }})
}

pub(crate) async fn reconcile(path: PathBuf) -> Result<(), String> {
    let Ok(_guard) = SYNC.try_lock() else {
        return Ok(());
    };
    let records = db::blocking(path.clone(), |db| {
        let mut q=db.prepare("SELECT instance_id,container_id FROM platform_instances WHERE container_id IS NOT NULL").map_err(|e|e.to_string())?;
        q.query_map([],|r| Ok((r.get::<_,String>(0)?,r.get::<_,String>(1)?)))
            .map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }).await?;
    if records.is_empty() {
        return Ok(());
    }
    // A successful daemon listing is required before absence can mean deletion.
    let listing = read(
        &["container", "ls", "-aq", "--no-trunc"],
        10,
        4 * 1024 * 1024,
    )
    .await;
    let mut snapshots = Vec::new();
    for chunk in records.chunks(50) {
        let ids: Vec<String> = chunk
            .iter()
            .filter(|(_, id)| {
                listing
                    .as_ref()
                    .is_ok_and(|text| text.lines().any(|v| v == id))
            })
            .map(|(_, id)| id.clone())
            .collect();
        let inspected = inspect(&ids).await;
        for (instance, id) in chunk {
            let item = inspected
                .as_ref()
                .ok()
                .and_then(|items| items.iter().find(|v| v["id"] == *id));
            let value = observation(instance, item, listing.is_ok() && inspected.is_ok());
            snapshots.push((instance.clone(), id.clone(), value.to_string()));
        }
    }
    db::blocking(path, move |db| {
        let tx=db.transaction().map_err(|e|e.to_string())?;
        for (instance,id,value) in snapshots {
            tx.execute("UPDATE platform_instances SET runtime_state=? WHERE instance_id=? AND container_id=?",params![value,instance,id]).map_err(|e|e.to_string())?;
            // Interrupted means outcome uncertain, even if the desired state now matches.
            // Keep both the interrupted outcome and the first successful reconciliation.
            if serde_json::from_str::<Value>(&value).ok().is_some_and(|v|v["state"]!="unknown") {
                tx.execute("UPDATE ops_container_actions SET reconciliation=? WHERE container_id=? AND status='interrupted' AND reconciliation IS NULL",params![value,id]).map_err(|e|e.to_string())?;
            }
        }
        tx.commit().map_err(|e|e.to_string())
    }).await
}

pub(crate) fn start(path: PathBuf) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = reconcile(path.clone()).await {
                eprintln!("container reconciliation: {e}");
            }
            tokio::time::sleep(std::time::Duration::from_secs(5)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distinguishes_disconnect_deletion_exit_and_replaced_binding() {
        assert_eq!(observation("i", None, false)["state"], "unknown");
        assert_eq!(observation("i", None, true)["state"], "missing");
        let item = json!({"instanceId":"i","state":"exited","exitCode":137});
        assert_eq!(observation("i", Some(&item), true)["exitCode"], 137);
        assert_eq!(observation("other", Some(&item), true)["state"], "mismatch");
        assert_eq!(observation("i", Some(&item), false)["state"], "unknown");
    }
}
