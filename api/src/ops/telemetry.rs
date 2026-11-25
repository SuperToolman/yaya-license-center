use super::*;
use super::{engine, local_containers::valid_id};
use futures_util::{StreamExt, stream};
use serde_json::{Value, json};

fn number(v: &Value) -> Option<f64> {
    v.as_f64().filter(|v| v.is_finite())
}
fn cpu(v: &Value) -> Option<f64> {
    let usage = number(&v["cpu_stats"]["cpu_usage"]["total_usage"])?
        - number(&v["precpu_stats"]["cpu_usage"]["total_usage"])?;
    let system = number(&v["cpu_stats"]["system_cpu_usage"])?
        - number(&v["precpu_stats"]["system_cpu_usage"])?;
    let cores = number(&v["cpu_stats"]["online_cpus"]).or_else(|| {
        v["cpu_stats"]["cpu_usage"]["percpu_usage"]
            .as_array()
            .map(|v| v.len() as f64)
    })?;
    if system <= 0.0 || usage < 0.0 {
        return None;
    }
    Some(usage / system * cores * 100.0)
}
fn network(v: &Value, key: &str) -> Option<u64> {
    v["networks"]
        .as_object()
        .map(|n| n.values().filter_map(|v| v[key].as_u64()).sum())
}
fn rate(current: Option<u64>, previous: &Value, key: &str, elapsed: f64) -> Option<f64> {
    let delta = current?.checked_sub(previous[key].as_u64()?)?;
    (elapsed > 0.0).then_some(delta as f64 / elapsed)
}
async fn sample(id: String, previous: Value) -> Value {
    let now = chrono::Utc::now().timestamp_millis();
    let disk_due = previous["diskCheckedAt"]
        .as_i64()
        .is_none_or(|time| now - time >= 60000);
    let inspect = match engine::json(
        "GET",
        &format!(
            "/containers/{id}/json{}",
            if disk_due { "?size=true" } else { "" }
        ),
        None,
    )
    .await
    {
        Ok(v) => v,
        Err(e) => return json!({"capturedAt":now,"state":"unknown","error":e}),
    };
    let mut stats_error = None;
    let stats = if inspect["State"]["Running"] == true {
        match engine::json("GET", &format!("/containers/{id}/stats?stream=false"), None).await {
            Ok(v) => v,
            Err(e) => {
                stats_error = Some(e);
                Value::Null
            }
        }
    } else {
        Value::Null
    };
    let usage = stats["memory_stats"]["usage"].as_u64();
    let cache = stats["memory_stats"]["stats"]["inactive_file"]
        .as_u64()
        .or_else(|| stats["memory_stats"]["stats"]["total_inactive_file"].as_u64())
        .unwrap_or(0);
    let memory = usage.map(|v| v.saturating_sub(cache));
    let limit = stats["memory_stats"]["limit"].as_u64();
    let elapsed = if previous["startedAt"] == inspect["State"]["StartedAt"] {
        (now - previous["capturedAt"].as_i64().unwrap_or(now)) as f64 / 1000.0
    } else {
        0.0
    };
    let rx = network(&stats, "rx_bytes");
    let tx = network(&stats, "tx_bytes");
    json!({"capturedAt":now,"state":inspect["State"]["Status"],"health":inspect["State"]["Health"]["Status"],
        "exitCode":inspect["State"]["ExitCode"],"oomKilled":inspect["State"]["OOMKilled"],"restartCount":inspect["RestartCount"],"startedAt":inspect["State"]["StartedAt"],
        "cpuPercent":cpu(&stats),"memoryUsageBytes":memory,"memoryLimitBytes":limit,"memoryPercent":memory.zip(limit).filter(|(_,l)|*l>0).map(|(u,l)|u as f64/l as f64*100.0),
        "networkRxBytes":rx,"networkTxBytes":tx,"networkRxBytesPerSecond":rate(rx,&previous,"networkRxBytes",elapsed),"networkTxBytesPerSecond":rate(tx,&previous,"networkTxBytes",elapsed),
        "writableLayerBytes":if disk_due{inspect["SizeRw"].clone()}else{previous["writableLayerBytes"].clone()},
        "rootFsBytes":if disk_due{inspect["SizeRootFs"].clone()}else{previous["rootFsBytes"].clone()},"diskCheckedAt":if disk_due{json!(now)}else{previous["diskCheckedAt"].clone()},
        "error":stats_error})
}

async fn collect(path: PathBuf) -> Result<(), String> {
    let records=db::blocking(path.clone(),|db|{
        let mut q=db.prepare("SELECT i.container_id,(SELECT data FROM ops_container_samples s WHERE s.container_id=i.container_id ORDER BY captured_at_ms DESC LIMIT 1) FROM platform_instances i WHERE i.container_id IS NOT NULL AND i.status!='archived' LIMIT 200").map_err(|e|e.to_string())?;
        q.query_map([],|r|Ok((r.get::<_,String>(0)?,r.get::<_,Option<String>>(1)?.and_then(|s|serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null)))).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())
    }).await?;
    let snapshots = stream::iter(records)
        .map(|(id, prev)| async move {
            let v = sample(id.clone(), prev).await;
            (id, v)
        })
        .buffer_unordered(4)
        .collect::<Vec<_>>()
        .await;
    db::blocking(path,move|db|{
        let tx=db.transaction().map_err(|e|e.to_string())?;
        for (id,value) in snapshots{tx.execute("INSERT OR REPLACE INTO ops_container_samples(container_id,captured_at_ms,data) VALUES(?,?,?)",params![id,value["capturedAt"].as_i64(),value.to_string()]).map_err(|e|e.to_string())?;}
        tx.execute("DELETE FROM ops_container_samples WHERE rowid IN (SELECT rowid FROM ops_container_samples WHERE captured_at_ms<? LIMIT 5000)",[chrono::Utc::now().timestamp_millis()-24*60*60*1000]).map_err(|e|e.to_string())?;
        tx.commit().map_err(|e|e.to_string())
    }).await
}
pub(crate) fn start(path: PathBuf) {
    tokio::spawn(async move {
        loop {
            if let Err(e) = collect(path.clone()).await {
                eprintln!("container sampling: {e}")
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
        }
    });
}

fn alerts(history: &[Value]) -> Vec<Value> {
    let mut result = vec![];
    let Some(current) = history.last() else {
        return result;
    };
    if current["oomKilled"] == true {
        result.push(json!({"code":"OOM_KILLED","message":"容器曾因内存不足被终止，请检查内存限制与应用日志"}));
    }
    if ["exited", "dead"].contains(&current["state"].as_str().unwrap_or(""))
        && current["exitCode"].as_i64().is_some_and(|v| v != 0)
    {
        result.push(json!({"code":"ABNORMAL_EXIT","message":format!("容器异常退出，退出码 {}",current["exitCode"])}));
    }
    let since = current["capturedAt"].as_i64().unwrap_or(0) - 10 * 60 * 1000;
    let recent: Vec<_> = history
        .iter()
        .filter(|v| v["capturedAt"].as_i64().is_some_and(|t| t >= since))
        .collect();
    let restarts = recent
        .iter()
        .filter_map(|v| v["restartCount"].as_u64())
        .collect::<Vec<_>>();
    let starts = recent
        .iter()
        .filter_map(|v| v["startedAt"].as_str())
        .filter(|v| !v.starts_with("0001-"))
        .collect::<std::collections::HashSet<_>>();
    if restarts
        .iter()
        .max()
        .copied()
        .unwrap_or(0)
        .saturating_sub(restarts.iter().min().copied().unwrap_or(0))
        >= 3
        || starts.len() >= 4
    {
        result.push(json!({"code":"FREQUENT_RESTART","message":"最近 10 分钟检测到至少 3 次重启，请检查日志与重启策略"}));
    }
    if current["state"] == "unknown" {
        result.push(json!({"code":"DOCKER_UNAVAILABLE","message":"容器指标无法采集，请检查 Docker 连接和目标容器"}));
    }
    result
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
    match db::blocking(state.database_path.clone(),move|db|{
        let mut q=db.prepare("SELECT data FROM ops_container_samples WHERE container_id=? ORDER BY captured_at_ms DESC LIMIT 180").map_err(|e|e.to_string())?;
        let mut items=q.query_map([id],|r|r.get::<_,String>(0)).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.into_iter().filter_map(|s|serde_json::from_str::<Value>(&s).ok()).collect::<Vec<_>>();items.reverse();
        let warnings=alerts(&items);
        Ok(json!({"latest":items.last(),"history":items,"alerts":warnings,"intervalSeconds":5,"retentionHours":24,"diskIncludesVolumes":false}))
    }).await{Ok(v)=>ok(v).into_response(),Err(_)=>failure(StatusCode::INTERNAL_SERVER_ERROR,"METRICS_FAILED","容器指标读取失败",&context.request_id)}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_rates_and_alerts_do_not_invent_zero_or_negative_values() {
        assert_eq!(cpu(&Value::Null), None);
        assert_eq!(rate(Some(1), &json!({"rx":10}), "rx", 1.0), None);
        assert_eq!(
            cpu(
                &json!({"cpu_stats":{"cpu_usage":{"total_usage":200},"system_cpu_usage":1000,"online_cpus":4},"precpu_stats":{"cpu_usage":{"total_usage":100},"system_cpu_usage":500}})
            ),
            Some(80.0)
        );
        let values = vec![
            json!({"capturedAt":100,"restartCount":0}),
            json!({"capturedAt":200,"restartCount":3,"oomKilled":true,"state":"exited","exitCode":137}),
        ];
        assert_eq!(alerts(&values).len(), 3);
    }
}
