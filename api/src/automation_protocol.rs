//! Portable automation dependency semantics, shared by publisher and receiver.
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

type Nodes<'a> = BTreeMap<&'a str, &'a Value>;
fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    v[key].as_str().unwrap_or("")
}
fn required(v: &Value, key: &str) -> Result<String, String> {
    let id = text(v, key);
    if id.trim().is_empty() {
        Err(format!("{key}: 必须配置目标或来源"))
    } else {
        Ok(id.into())
    }
}
fn upstream(edges: &[Value], current: &str, source: &str) -> Result<(), String> {
    let mut seen = BTreeSet::from([current]);
    let mut queue = vec![current];
    while let Some(target) = queue.pop() {
        for edge in edges.iter().filter(|e| text(e, "target") == target) {
            let id = text(edge, "source");
            if seen.insert(id) {
                queue.push(id);
            }
        }
    }
    if source.is_empty() || source == current || !seen.contains(source) {
        Err("来源必须是当前节点的上游数据节点".into())
    } else {
        Ok(())
    }
}
fn source_form(
    nodes: &Nodes<'_>,
    edges: &[Value],
    id: &str,
    trigger: &str,
    visiting: &mut BTreeSet<String>,
) -> Result<String, String> {
    let mut current = id.to_string();
    loop {
        if !visiting.insert(current.clone()) {
            return Err("数据来源存在循环引用".into());
        }
        let node = nodes.get(current.as_str()).ok_or("来源节点不存在")?;
        let config = &node["data"]["config"];
        match text(&node["data"], "kind") {
            "trigger" => {
                return if trigger.is_empty() {
                    Err("触发表单未配置".into())
                } else {
                    Ok(trigger.into())
                };
            }
            "add-data" if text(config, "targetMode") != "subtable" => {
                return required(config, "targetFormUuid");
            }
            "get-one" | "get-many" => match text(config, "sourceMode") {
                "" | "form" => return required(config, "formUuid"),
                "trigger" => {
                    return if trigger.is_empty() {
                        Err("trigger_form_uuid: 触发表单未配置".into())
                    } else {
                        Ok(trigger.into())
                    };
                }
                "data-node" => {
                    let source = required(config, "dataNodeId")?;
                    upstream(edges, &current, &source)?;
                    current = source;
                }
                _ => return Err("sourceMode: 不支持的数据来源模式".into()),
            },
            _ => return Err("来源节点没有主表记录输出".into()),
        }
    }
}

/// Strip only caches that the execution engine explicitly ignores. Keep update targets:
/// update-data still reads targetFormUuid even when selecting records from an upstream node.
pub(super) fn normalize(flow: &mut Value) {
    for node in flow["nodes_json"].as_array_mut().into_iter().flatten() {
        let kind = text(&node["data"], "kind").to_string();
        if let Some(config) = node
            .get_mut("data")
            .and_then(|d| d.get_mut("config"))
            .and_then(Value::as_object_mut)
        {
            let mode = |key: &str| config.get(key).and_then(Value::as_str).unwrap_or("");
            let ignored = match kind.as_str() {
                "add-data" => mode("targetMode") == "subtable",
                "delete-data" => mode("deleteMode") == "data-node",
                "get-one" | "get-many" => matches!(mode("sourceMode"), "data-node" | "trigger"),
                _ => false,
            };
            if ignored {
                config.remove(if matches!(kind.as_str(), "get-one" | "get-many") {
                    "formUuid"
                } else {
                    "targetFormUuid"
                });
                config.remove("targetAppId");
            }
        }
    }
}

/// Resolve actual forms for every data-consuming node, including indirect node chains.
/// Returns node-index/config-path diagnostics and optional subtable targets.
pub(super) fn inspect(flow: &Value) -> Vec<(String, String)> {
    let mut issues = Vec::new();
    let Some(items) = flow["nodes_json"].as_array() else {
        return vec![("/nodes_json".into(), "自动化节点目录无效".into())];
    };
    let Some(edges) = flow["edges_json"].as_array() else {
        return vec![("/edges_json".into(), "自动化连接目录无效".into())];
    };
    let mut nodes = BTreeMap::new();
    for (i, node) in items.iter().enumerate() {
        let id = text(node, "id");
        if id.is_empty() || nodes.insert(id, node).is_some() {
            issues.push((format!("/nodes_json/{i}/id"), "节点 ID 缺失或重复".into()));
        }
    }
    for (i, edge) in edges.iter().enumerate() {
        if !nodes.contains_key(text(edge, "source")) || !nodes.contains_key(text(edge, "target")) {
            issues.push((format!("/edges_json/{i}"), "连接引用不存在的节点".into()));
        }
    }
    let trigger = text(flow, "trigger_form_uuid");
    for (i, node) in items.iter().enumerate() {
        let id = text(node, "id");
        let kind = text(&node["data"], "kind");
        let config = &node["data"]["config"];
        let resolve = |key: &str| -> Result<String, String> {
            let source = required(config, key)?;
            upstream(edges, id, &source).map_err(|e| format!("{key}: {e}"))?;
            source_form(&nodes, edges, &source, trigger, &mut BTreeSet::new())
                .map_err(|e| format!("{key}: {e}"))
        };
        let result: Result<(), String> = (|| {
            let mode_key = match kind {
                "get-one" | "get-many" => "sourceMode",
                "add-data" => "targetMode",
                "delete-data" => "deleteMode",
                "update-data" => "updateMode",
                _ => "",
            };
            if config
                .get(mode_key)
                .is_some_and(|v| !v.is_null() && !v.is_string())
            {
                return Err(format!("{mode_key}: 模式必须为字符串"));
            }
            match kind {
                "get-one" | "get-many" => {
                    source_form(&nodes, edges, id, trigger, &mut BTreeSet::new())?;
                }
                "add-data" => {
                    match text(config, "targetMode") {
                        "subtable" => {
                            resolve("parentDataNodeId")?;
                            required(config, "subformFieldId")?;
                        }
                        "" | "form" => {
                            required(config, "targetFormUuid")?;
                        }
                        _ => return Err("targetMode: 不支持的新增模式".into()),
                    }
                    if text(config, "recordMode") == "multiple" {
                        resolve("multipleSourceNodeId")?;
                    }
                }
                "delete-data" => match text(config, "deleteMode") {
                    "data-node" => {
                        resolve("sourceNodeId")?;
                    }
                    "" | "form" => {
                        required(config, "targetFormUuid")?;
                    }
                    _ => return Err("deleteMode: 不支持的删除模式".into()),
                },
                "update-data" => {
                    let target = required(config, "targetFormUuid")?;
                    match text(config, "updateMode") {
                        "data-node" => {
                            if resolve("sourceNodeId")? != target {
                                return Err(
                                    "targetFormUuid: 更新目标与上游记录所属表单不一致".into()
                                );
                            }
                        }
                        "" | "form" => (),
                        _ => return Err("updateMode: 不支持的更新模式".into()),
                    }
                }
                "initiate-approval" => {
                    required(config, "targetFormUuid")?;
                }
                "trigger" | "condition" | "parallel-condition" | "approval" | "copy"
                | "executor" => (),
                "http-request" | "connector" | "integration" => {
                    return Err("外部连接器或 HTTP 自动化尚不支持发布".into());
                }
                _ => return Err(format!("不支持的自动化节点类型：{kind}")),
            }
            Ok(())
        })();
        if let Err(message) = result {
            issues.push((
                format!("/nodes_json/{i}/data/config"),
                format!("节点 {id}：{message}"),
            ));
        }
    }
    // A graph cycle cannot be a valid upstream relationship, even if all sources exist.
    let mut pending: BTreeSet<_> = nodes.keys().copied().collect();
    loop {
        let roots: Vec<_> = pending
            .iter()
            .copied()
            .filter(|id| {
                !edges
                    .iter()
                    .any(|e| text(e, "target") == *id && pending.contains(text(e, "source")))
            })
            .collect();
        if roots.is_empty() {
            break;
        }
        for id in roots {
            pending.remove(id);
        }
    }
    if !pending.is_empty() {
        issues.push(("/edges_json".into(), "自动化连接存在循环".into()));
    }
    issues
}

pub(super) fn validate_bundle(bundle: &Value) -> Result<(), String> {
    for member in bundle["members"].as_array().into_iter().flatten() {
        for (i, flow) in member["resources"]["automation_flows"]
            .as_array()
            .into_iter()
            .flatten()
            .enumerate()
        {
            let prefix = format!(
                "应用 {} / 自动化 {} /resources/automation_flows/{i}",
                text(member, "appId"),
                text(flow, "flow_uuid")
            );
            let mut canonical = flow.clone();
            normalize(&mut canonical);
            if &canonical != flow {
                return Err(format!("{prefix}: 包含非生效的缓存表单引用，请重新导出"));
            }
            if let Some((path, message)) = inspect(flow).into_iter().next() {
                return Err(format!("{prefix}{path}: {message}"));
            }
            let nodes: Nodes<'_> = flow["nodes_json"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| (text(n, "id"), n))
                .collect();
            let edges = flow["edges_json"].as_array().unwrap();
            for (j, node) in flow["nodes_json"].as_array().unwrap().iter().enumerate() {
                let config = &node["data"]["config"];
                if text(&node["data"], "kind") != "add-data"
                    || text(config, "targetMode") != "subtable"
                {
                    continue;
                }
                let parent = source_form(
                    &nodes,
                    edges,
                    text(config, "parentDataNodeId"),
                    text(flow, "trigger_form_uuid"),
                    &mut BTreeSet::new(),
                )?;
                let fields = bundle["members"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .flat_map(|m| {
                        m["resources"]["form_schemas"]
                            .as_array()
                            .into_iter()
                            .flatten()
                    })
                    .find(|s| text(s, "form_uuid") == parent)
                    .and_then(|s| s["schema_json"]["fields"].as_array())
                    .ok_or_else(|| format!("{prefix}/nodes_json/{j}: 子表所属主表结构不可用"))?;
                let subform = text(config, "subformFieldId");
                if !fields
                    .iter()
                    .any(|f| text(f, "id") == subform && text(f, "type") == "subform")
                {
                    return Err(format!(
                        "{prefix}/nodes_json/{j}/data/config/subformFieldId: 子表不存在或不属于来源表单"
                    ));
                }
                let mut used = BTreeSet::new();
                for (k, row) in config["rows"].as_array().into_iter().flatten().enumerate() {
                    let field = text(row, "fieldId");
                    if !used.insert(field)
                        || !fields.iter().any(|f| {
                            text(f, "id") == field
                                && text(f, "parentGroupId") == subform
                                && !matches!(
                                    text(f, "type"),
                                    "groupContainer" | "separator" | "button" | "subform"
                                )
                        })
                    {
                        return Err(format!(
                            "{prefix}/nodes_json/{j}/data/config/rows/{k}/fieldId: 子表目标字段失效或重复"
                        ));
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn node(id: &str, kind: &str, config: Value) -> Value {
        json!({"id":id,"data":{"kind":kind,"config":config}})
    }
    fn flow() -> Value {
        json!({"flow_uuid":"flow-a","trigger_form_uuid":"form-a","nodes_json":[
            node("trigger","trigger",json!({})),
            node("read","get-many",json!({"sourceMode":"form","formUuid":"form-b"})),
            node("delete","delete-data",json!({"deleteMode":"data-node","sourceNodeId":"read","targetFormUuid":"deleted-cache"})),
            node("child","add-data",json!({"targetMode":"subtable","parentDataNodeId":"read","subformFieldId":"items","targetFormUuid":"deleted-cache"}))
        ],"edges_json":[{"source":"trigger","target":"read"},{"source":"read","target":"delete"},{"source":"read","target":"child"}]})
    }
    #[test]
    fn ignores_inactive_caches_but_keeps_upstream_dependency_and_node_ids() {
        let mut f = flow();
        normalize(&mut f);
        assert!(inspect(&f).is_empty(), "{:?}", inspect(&f));
        let mut refs = Vec::new();
        super::super::references(&f, "", &mut refs);
        assert!(refs.iter().any(|(_, id, _)| id == "form-b"));
        assert!(!refs.iter().any(|(_, id, _)| id == "deleted-cache"));
        let maps = BTreeMap::from([(
            "form_definitions".into(),
            BTreeMap::from([
                ("form-a".into(), "local-a".into()),
                ("form-b".into(), "local-b".into()),
            ]),
        )]);
        super::super::rewrite(&mut f, &maps).unwrap();
        assert_eq!(f["nodes_json"][1]["data"]["config"]["formUuid"], "local-b");
        assert_eq!(f["nodes_json"][2]["data"]["config"]["sourceNodeId"], "read");
    }
    #[test]
    fn rejects_missing_non_upstream_and_cyclic_data_sources() {
        for source in ["missing", "delete", "child"] {
            let mut f = flow();
            f["nodes_json"][2]["data"]["config"]["sourceNodeId"] = json!(source);
            assert!(!inspect(&f).is_empty());
        }
        let mut f = flow();
        f["edges_json"]
            .as_array_mut()
            .unwrap()
            .push(json!({"source":"delete","target":"read"}));
        assert!(inspect(&f).iter().any(|(_, m)| m.contains("循环")));
    }
    #[test]
    fn update_requires_actual_source_form_not_stale_target() {
        let mut f = flow();
        f["nodes_json"][2] = node(
            "delete",
            "update-data",
            json!({"updateMode":"data-node","sourceNodeId":"read","targetFormUuid":"form-a"}),
        );
        assert!(inspect(&f).iter().any(|(_, m)| m.contains("更新目标")));
        f["nodes_json"][2]["data"]["config"]["targetFormUuid"] = json!("form-b");
        normalize(&mut f);
        assert!(inspect(&f).is_empty());
        assert_eq!(
            f["nodes_json"][2]["data"]["config"]["targetFormUuid"],
            "form-b"
        );
    }
    #[test]
    fn trigger_queries_do_not_require_cached_form_or_node_selection() {
        let mut f = flow();
        f["nodes_json"][1]["data"]["config"] =
            json!({"sourceMode":"trigger","formUuid":"deleted-cache"});
        normalize(&mut f);
        assert!(inspect(&f).is_empty(), "{:?}", inspect(&f));
        f["trigger_form_uuid"] = Value::Null;
        assert!(!inspect(&f).is_empty());
    }
    #[test]
    fn validates_all_direct_data_targets_and_rejects_unknown_modes() {
        for kind in [
            "add-data",
            "update-data",
            "delete-data",
            "get-one",
            "get-many",
            "initiate-approval",
        ] {
            let mut f = flow();
            f["nodes_json"] = json!([node("n", kind, json!({}))]);
            f["edges_json"] = json!([]);
            assert!(!inspect(&f).is_empty(), "{kind}");
            f["nodes_json"][0]["data"]["config"] =
                json!({"formUuid":"form-b","targetFormUuid":"form-b"});
            assert!(inspect(&f).is_empty(), "{kind}");
        }
        let mut f = flow();
        f["nodes_json"][1]["data"]["config"]["sourceMode"] = json!("unknown");
        assert!(!inspect(&f).is_empty());
    }
    fn subtable_bundle() -> Value {
        let mut b = super::super::tests::bundle();
        let mut f = flow();
        f["trigger_form_uuid"] = json!("source-form-1");
        f["nodes_json"][1]["data"]["config"]["formUuid"] = json!("source-form-2");
        normalize(&mut f);
        b["members"][0]["resources"]["automation_flows"][0]["nodes_json"] = f["nodes_json"].clone();
        b["members"][0]["resources"]["automation_flows"][0]["edges_json"] = f["edges_json"].clone();
        b["members"][1]["resources"]["form_schemas"][0]["schema_json"]["fields"]
            .as_array_mut()
            .unwrap()
            .extend([
                json!({"id":"items","type":"subform"}),
                json!({"id":"item-name","type":"text","parentGroupId":"items"}),
            ]);
        b["dependencies"] = json!(super::super::dependency_graph(&b));
        b
    }
    #[test]
    fn receiver_checks_subtable_owner_and_mapping_fields() {
        let mut b = subtable_bundle();
        assert!(super::super::resources(&b).is_ok());
        let config = &mut b["members"][0]["resources"]["automation_flows"][0]["nodes_json"][3]["data"]
            ["config"];
        config["rows"] = json!([{"fieldId":"item-name"}]);
        assert!(super::super::resources(&b).is_ok());
        b["members"][0]["resources"]["automation_flows"][0]["nodes_json"][3]["data"]["config"]["rows"] =
            json!([{"fieldId":"name"}]);
        assert!(
            super::super::resources(&b)
                .unwrap_err()
                .contains("子表目标字段")
        );
        b["members"][0]["resources"]["automation_flows"][0]["nodes_json"][3]["data"]["config"]["subformFieldId"] =
            json!("missing");
        assert!(
            super::super::resources(&b)
                .unwrap_err()
                .contains("子表不存在")
        );
    }
    #[test]
    fn receiver_rejects_inactive_cache_and_invalid_source_even_with_valid_checksums() {
        let mut b = subtable_bundle();
        b["members"][0]["resources"]["automation_flows"][0]["nodes_json"][2]["data"]["config"]["targetFormUuid"] =
            json!("source-form-3");
        let zip = super::super::tests::pack(&b, "1.0.0");
        assert!(super::super::read(&zip).is_err());
    }
}

#[cfg(test)]
mod malformed_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn malformed_nodes_are_rejected_without_panicking() {
        for node in [
            Value::Null,
            json!(1),
            json!({"id":"bad","data":null}),
            json!({"id":"bad","data":{"kind":"unsupported"}}),
        ] {
            let mut flow = json!({"nodes_json":[node],"edges_json":[]});
            normalize(&mut flow);
            assert!(!inspect(&flow).is_empty());
        }
    }
}

#[cfg(test)]
mod dependency_tests {
    use serde_json::json;
    #[test]
    fn automation_only_chain_and_cycle_are_closed_and_remapped() {
        let mut b = super::super::tests::bundle();
        for (i, m) in b["members"].as_array_mut().unwrap().iter_mut().enumerate() {
            m["resources"]["form_schemas"][0]["schema_json"]["fields"][1]["props"] = json!({});
            m["resources"]["app_custom_buttons"] = json!([]);
            m["resources"]["form_views"] = json!([]);
            m["resources"]["automation_flows"] = json!([{
                "flow_uuid":format!("flow-{i}"),"app_route_app_id":format!("source-app-{}",i+1),
                "nodes_json":[{"id":"read","data":{"kind":"get-many","config":{"sourceMode":"form","formUuid":format!("source-form-{}",(i+1)%3+1)}}}],"edges_json":[]
            }]);
        }
        b["dependencies"] = json!(super::super::dependency_graph(&b));
        assert_eq!(b["dependencies"].as_array().unwrap().len(), 3);
        assert!(super::super::resources(&b).is_ok());
        assert!(super::super::read(&super::super::tests::pack(&b, "1.0.0")).is_ok());
        b["members"][1]["resources"]["automation_flows"][0]["nodes_json"][0]["data"]["config"]["formUuid"] =
            json!("missing");
        assert!(super::super::resources(&b).unwrap_err().contains("引用"));
    }
}
