//! V3 portable package contract. Kept identical in both platform repositories.
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

#[path = "automation_protocol.rs"]
mod automation;

pub fn normalize_automation(flow: &mut Value) {
    automation::normalize(flow);
}
pub fn automation_issues(flow: &Value) -> Vec<(String, String)> {
    automation::inspect(flow)
}

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub const TABLES: &[(&str, &str)] = &[
    ("apps", "route_app_id"),
    ("form_definitions", "form_uuid"),
    ("form_schemas", "form_uuid"),
    ("app_navigation_items", "id"),
    ("form_views", "view_uuid"),
    ("automation_flows", "flow_uuid"),
    ("app_custom_buttons", "button_uuid"),
    ("form_detail_definitions", "detail_form_uuid"),
];

/// Reject configuration whose instance references cannot be exported and rewritten.
pub fn configuration_issue(key: &str, value: &Value) -> Option<&'static str> {
    if value.is_null() || value == "" || value.as_array().is_some_and(Vec::is_empty) {
        return None;
    }
    if key == "type" && value.as_str().is_some_and(|v| matches!(v, "html" | "tsx")) {
        return Some("自定义 HTML/TSX 无法静态验证引用完整性，暂不支持发布");
    }
    if key == "kind"
        && value
            .as_str()
            .is_some_and(|v| matches!(v, "http-request" | "connector" | "integration"))
    {
        return Some("外部连接器或 HTTP 自动化尚不支持发布");
    }
    if matches!(
        key,
        "credentialId"
            | "secret"
            | "apiKey"
            | "password"
            | "initiatorId"
            | "assigneeId"
            | "userId"
            | "roleId"
            | "userIds"
            | "roleIds"
            | "assigneeIds"
            | "recipientIds"
            | "fileId"
            | "attachmentId"
            | "recordId"
            | "recordIds"
            | "physical_table"
            | "main_table"
    ) {
        return Some("包含实例凭证、用户权限绑定或实例数据，请先移除");
    }
    if key == "url"
        && value
            .as_str()
            .is_some_and(|u| u.starts_with('/') && !u.starts_with("//"))
    {
        return Some("内部页面或文件地址必须使用可移植的结构化引用");
    }
    None
}

pub fn reference_kind(key: &str) -> Option<&'static str> {
    match key {
        "associationAppId" | "targetAppId" | "appId" | "app_route_app_id" => Some("apps"),
        "associationFormId"
        | "targetFormUuid"
        | "triggerFormUuid"
        | "formUuid"
        | "sourceFormUuid"
        | "detailFormUuid"
        | "detailSourceFormUuid"
        | "relationFormUuid"
        | "relationTarget"
        | "form_uuid"
        | "target_form_uuid"
        | "trigger_form_uuid"
        | "detail_form_uuid"
        | "source_form_uuid" => Some("form_definitions"),
        "automationId" | "flowUuid" | "processFlowId" => Some("automation_flows"),
        "buttonId" => Some("app_custom_buttons"),
        "viewUuid" => Some("form_views"),
        "parent_id" => Some("app_navigation_items"),
        _ => None,
    }
}

/// Views are identified inside a form; `default` is intentionally reused.
pub fn view_identity(form: &str, view: &str) -> String {
    serde_json::to_string(&(form, view)).unwrap()
}
pub fn resource_identity(table: &str, key: &str, row: &Value) -> Result<String, String> {
    let id = row[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("资源缺少 ID")?;
    if table == "form_views" {
        Ok(view_identity(
            row["form_uuid"].as_str().ok_or("视图缺少所属表单")?,
            id,
        ))
    } else {
        Ok(id.into())
    }
}
fn form_context<'a>(value: &'a Value, inherited: Option<&'a str>) -> Option<&'a str> {
    [
        "form_uuid",
        "formUuid",
        "targetFormUuid",
        "target_form_uuid",
    ]
    .iter()
    .find_map(|key| value[*key].as_str().filter(|v| !v.is_empty()))
    .or(inherited)
}
fn resolve_view<'a, T>(id: &str, index: &'a BTreeMap<String, T>) -> Option<&'a T> {
    if let Some(value) = index.get(id) {
        return Some(value);
    }
    let mut matches = index.iter().filter(|(key, _)| {
        serde_json::from_str::<(String, String)>(key).is_ok_and(|(_, view)| view == id)
    });
    let (_, value) = matches.next()?;
    if matches.next().is_some() {
        None
    } else {
        Some(value)
    }
}

/// Visit only contract-defined reference slots, never arbitrary user strings.
pub fn references(value: &Value, path: &str, out: &mut Vec<(String, String, String)>) {
    references_in_form(value, path, out, None);
}
fn references_in_form(
    value: &Value,
    path: &str,
    out: &mut Vec<(String, String, String)>,
    inherited: Option<&str>,
) {
    let form = form_context(value, inherited);
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                let pointer = format!("{path}/{key}");
                if let Some(kind) = reference_kind(key) {
                    if let Some(id) = child.as_str().filter(|id| !id.is_empty()) {
                        let id = if kind == "form_views" {
                            form.map(|f| view_identity(f, id))
                                .unwrap_or_else(|| id.into())
                        } else {
                            id.into()
                        };
                        out.push((kind.into(), id, pointer.clone()));
                    }
                }
                references_in_form(child, &pointer, out, form);
            }
        }
        Value::Array(items) => {
            for (i, child) in items.iter().enumerate() {
                references_in_form(child, &format!("{path}/{i}"), out, form);
            }
        }
        _ => (),
    }
}

pub fn dependency_graph(bundle: &Value) -> Vec<Value> {
    let mut owners = BTreeMap::new();
    for member in bundle["members"].as_array().into_iter().flatten() {
        for (table, key) in TABLES {
            for row in member["resources"][*table].as_array().into_iter().flatten() {
                if let Ok(id) = resource_identity(table, key, row) {
                    owners.insert(format!("{table}:{id}"), member["appId"].clone());
                }
                if *table == "automation_flows" {
                    if let Some(id) = row["id"].as_str() {
                        owners.insert(format!("{table}:{id}"), member["appId"].clone());
                    }
                }
            }
        }
    }
    let mut edges = BTreeMap::new();
    for member in bundle["members"].as_array().into_iter().flatten() {
        let mut refs = Vec::new();
        references(&member["resources"], "/resources", &mut refs);
        for (kind, id, path) in refs {
            let view_owners: BTreeMap<_, _> = owners
                .iter()
                .filter_map(|(key, owner)| {
                    key.strip_prefix("form_views:")
                        .map(|id| (id.to_string(), owner.clone()))
                })
                .collect();
            let owner = owners.get(&format!("{kind}:{id}")).or_else(|| {
                if kind == "form_views" {
                    resolve_view(&id, &view_owners)
                } else {
                    None
                }
            });
            if let Some(owner) = owner {
                if owner != &member["appId"] {
                    let edge = json!({"sourceAppId":member["appId"],"targetAppId":owner,"resourceType":kind,"resourceId":id,"path":path});
                    edges.insert(edge.to_string(), edge);
                }
            }
        }
    }
    edges.into_values().collect()
}

pub fn rewrite(
    value: &mut Value,
    maps: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), String> {
    rewrite_in_form(value, maps, None)
}
fn rewrite_in_form(
    value: &mut Value,
    maps: &BTreeMap<String, BTreeMap<String, String>>,
    inherited: Option<&str>,
) -> Result<(), String> {
    let form = form_context(value, inherited).map(str::to_string);
    match value {
        Value::Object(object) => {
            for (key, child) in object {
                if let Some(kind) = reference_kind(key) {
                    if let Some(id) = child.as_str().filter(|id| !id.is_empty()) {
                        let qualified = if kind == "form_views" {
                            form.as_deref()
                                .map(|f| view_identity(f, id))
                                .unwrap_or_else(|| id.into())
                        } else {
                            id.into()
                        };
                        let mapped = maps
                            .get(kind)
                            .and_then(|m| {
                                if kind == "form_views" {
                                    resolve_view(&qualified, m)
                                } else {
                                    m.get(&qualified)
                                }
                            })
                            .ok_or_else(|| format!("引用未映射：{key}={id}"))?;
                        *child = json!(mapped);
                    }
                }
                rewrite_in_form(child, maps, form.as_deref())?;
            }
        }
        Value::Array(items) => {
            for child in items {
                rewrite_in_form(child, maps, form.as_deref())?;
            }
        }
        _ => (),
    }
    Ok(())
}

pub fn resources(bundle: &Value) -> Result<BTreeMap<String, (String, Value)>, String> {
    automation::validate_bundle(bundle)?;
    let mut index = BTreeMap::new();
    let members = bundle["members"].as_array().ok_or("缺少成员")?;
    if members.is_empty() {
        return Err("应用包成员不能为空".into());
    }
    let mut apps = BTreeSet::new();
    for member in members {
        let app = member["appId"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or("成员缺少应用 ID")?;
        if !apps.insert(app) {
            return Err("重复成员".into());
        }
        for (table, key) in TABLES {
            let rows = member["resources"][*table]
                .as_array()
                .ok_or_else(|| format!("缺少资源目录 {table}"))?;
            if *table == "apps" && (rows.len() != 1 || rows[0]["route_app_id"] != app) {
                return Err("成员应用归属无效".into());
            }
            for row in rows {
                let id = row[*key]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .ok_or("资源缺少 ID")?;
                if let Some(owner) = row["app_route_app_id"].as_str() {
                    if owner != app {
                        return Err("资源应用归属不一致".into());
                    }
                }
                let identity = resource_identity(table, key, row)?;
                let index_key = format!("{table}:{identity}");
                if index.insert(index_key, (app.into(), row.clone())).is_some() {
                    return Err(format!(
                        "重复配置：应用 {app} 的 {table} 中编号 {id} 重复；配置名称：{}，所属表单：{}",
                        row["name"]
                            .as_str()
                            .or_else(|| row["title"].as_str())
                            .unwrap_or("未命名"),
                        row["form_uuid"].as_str().unwrap_or("无")
                    ));
                }
            }
        }
        let forms: BTreeSet<_> = member["resources"]["form_definitions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|f| f["form_uuid"].as_str())
            .collect();
        for table in ["form_schemas", "form_views", "app_custom_buttons"] {
            for row in member["resources"][table].as_array().unwrap() {
                if !row["form_uuid"].as_str().is_some_and(|f| forms.contains(f)) {
                    return Err("表单资源归属不一致".into());
                }
            }
        }
        for form in member["resources"]["form_definitions"].as_array().unwrap() {
            if !member["resources"]["form_schemas"]
                .as_array()
                .unwrap()
                .iter()
                .any(|s| {
                    s["form_uuid"] == form["form_uuid"]
                        && s["version"] == form["current_schema_version"]
                })
            {
                return Err("缺少当前生效 Schema".into());
            }
        }
    }
    for (_, (_, row)) in &index {
        if row.get("detail_form_uuid").is_some() {
            let target = row["detail_form_uuid"].as_str().ok_or("明细表单 ID 无效")?;
            if !index.contains_key(&format!("form_definitions:{target}")) {
                return Err("明细定义缺少表单".into());
            }
        }
    }
    if bundle["applicationType"] != if apps.len() == 1 { "single" } else { "suite" } {
        return Err("应用类型与成员不一致".into());
    }
    let mut refs = Vec::new();
    // Schemas share a form ID with definitions, so keep their namespace separate.
    references(&bundle["members"], "/members", &mut refs);
    for (kind, id, path) in refs {
        let found = index.contains_key(&format!("{kind}:{id}"))
            || kind == "form_views" && {
                let views = index
                    .iter()
                    .filter_map(|(k, v)| k.strip_prefix("form_views:").map(|k| (k.to_string(), v)))
                    .collect();
                resolve_view(&id, &views).is_some()
            }
            || kind == "automation_flows"
                && index
                    .values()
                    .any(|(_, r)| r["id"] == id && r.get("flow_uuid").is_some());
        if !found {
            return Err(format!("包外或失效引用 {path}: {id}"));
        }
    }
    fn check_pairs(v: &Value, index: &BTreeMap<String, (String, Value)>) -> Result<(), String> {
        match v {
            Value::Object(o) => {
                for (form_key, app_key) in [
                    ("associationFormId", "associationAppId"),
                    ("targetFormUuid", "targetAppId"),
                ] {
                    if let (Some(form), Some(app)) = (
                        o.get(form_key).and_then(Value::as_str),
                        o.get(app_key)
                            .and_then(Value::as_str)
                            .filter(|a| !a.is_empty()),
                    ) {
                        if !form.is_empty()
                            && index
                                .get(&format!("form_definitions:{form}"))
                                .is_some_and(|(owner, _)| owner != app)
                        {
                            return Err(format!("引用应用与表单真实归属不一致：{form_key}={form}"));
                        }
                    }
                }
                for (key, v) in o {
                    if let Some(message) = configuration_issue(key, v) {
                        return Err(format!("{message}：{key}"));
                    }
                    check_pairs(v, index)?;
                }
            }
            Value::Array(a) => {
                for v in a {
                    check_pairs(v, index)?
                }
            }
            _ => (),
        }
        Ok(())
    }
    check_pairs(&bundle["members"], &index)?;
    if bundle.get("rootAppId").is_some() || bundle.get("additionalAppIds").is_some() {
        return Err("不支持旧应用包格式，请重新选择成员并导出".into());
    }
    let selected = bundle["selectedAppIds"].as_array().ok_or("缺少所选成员数组")?;
    if selected.is_empty() || selected.len() > 256 { return Err("请选择 1–256 个成员".into()); }
    let mut reached = BTreeSet::new();
    for id in selected {
        let id = id.as_str().ok_or("所选成员编号无效")?;
        if !apps.contains(id) { return Err("所选成员未包含在应用包中".into()); }
        if !reached.insert(id.to_string()) { return Err("所选成员重复".into()); }
    }
    loop {
        let old = reached.len();
        for edge in dependency_graph(bundle) {
            if reached.contains(edge["sourceAppId"].as_str().unwrap_or("")) {
                if let Some(target) = edge["targetAppId"].as_str() {
                    reached.insert(target.to_string());
                }
            }
        }
        if old == reached.len() {
            break;
        }
    }
    if reached.len() != members.len() {
        return Err("包包含未选择且不属于必要依赖的成员".into());
    }
    let declared: BTreeSet<_> = bundle["dependencies"]
        .as_array()
        .ok_or("缺少依赖图")?
        .iter()
        .map(Value::to_string)
        .collect();
    let actual: BTreeSet<_> = dependency_graph(bundle)
        .iter()
        .map(Value::to_string)
        .collect();
    if declared != actual {
        return Err("声明依赖与资源实际引用不一致".into());
    }
    Ok(index)
}

pub fn read(bytes: &[u8]) -> Result<(Value, Value), String> {
    if bytes.len() > 64 * 1024 * 1024 {
        return Err("应用包超过 64MiB".into());
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "应用包不是有效 ZIP")?;
    if archive.len() > 4096 {
        return Err("应用包文件过多".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    for i in 0..archive.len() {
        let mut file = archive.by_index(i).map_err(|_| "ZIP 读取失败")?;
        let name = file.name().to_owned();
        total = total.checked_add(file.size()).ok_or("解压大小溢出")?;
        if name.contains("..")
            || name.starts_with('/')
            || name.contains('\\')
            || name.contains(':')
            || file.is_dir()
            || file.size() > 16 * 1024 * 1024
            || total > 256 * 1024 * 1024
        {
            return Err("应用包路径或解压大小无效".into());
        }
        let mut data = Vec::new();
        file.by_ref()
            .take(16 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| "解压失败")?;
        if data.len() > 16 * 1024 * 1024 || files.insert(name, data).is_some() {
            return Err("应用包重复文件或文件过大".into());
        }
    }
    let manifest: Value =
        serde_json::from_slice(files.get("manifest.json").ok_or("缺少 manifest")?)
            .map_err(|_| "manifest 无效")?;
    if manifest["formatVersion"] != 3 || manifest["packageType"] != "yaya-application" {
        return Err("仅支持 V3 应用包，请重新发布旧版应用".into());
    }
    if manifest.get("rootAppId").is_some() || manifest.get("additionalAppIds").is_some() {
        return Err("不支持旧应用包格式，请重新选择成员并导出".into());
    }
    let checksums = manifest["checksums"].as_object().ok_or("缺少校验和")?;
    if checksums.len() + 1 != files.len() {
        return Err("存在未声明资源".into());
    }
    for (name, expected) in checksums {
        if name == "manifest.json"
            || expected.as_str() != Some(digest(files.get(name).ok_or("资源缺失")?).as_str())
        {
            return Err(format!("资源校验失败：{name}"));
        }
    }
    let bundle: Value = serde_json::from_slice(files.get("bundle.json").ok_or("缺少 bundle")?)
        .map_err(|_| "bundle 无效")?;
    resources(&bundle)?;
    for key in ["publication", "selectedAppIds", "packageName", "applicationType", "members", "dependencies"] {
        if manifest[key] != bundle[key] {
            return Err(format!("manifest 与资源包 {key} 不一致"));
        }
    }
    Ok((manifest, bundle))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn bundle() -> Value {
        let mut members = Vec::new();
        for (i, next) in [(1, 2), (2, 3), (3, 1)] {
            let app = format!("source-app-{i}");
            let form = format!("source-form-{i}");
            let mut resources = json!({});
            for (table, _) in TABLES {
                resources[*table] = json!([]);
            }
            resources["apps"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"route_app_id":app,"name":format!("应用{i}"),"description":"suite fixture","icon":"general","badge":null,"color":"primary"}]);
            resources["form_definitions"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"app_route_app_id":app,"form_uuid":form,"name":format!("表单{i}"),"slug":form,"form_type":"normal","current_schema_version":1,"latest_schema_version":1}]);
            resources["form_schemas"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"form_uuid":form,"version":1,"change_log":null,"schema_json":{"fields":[{"id":"name","type":"text","label":"名称"},{"id":"link","type":"associationFormField","props":{"associationAppId":format!("source-app-{next}"),"associationFormId":format!("source-form-{next}"),"associationPrimaryFieldId":"name"}}]}}]);
            if i == 1 {
                let parent = uuid::Uuid::new_v4().to_string();
                let child = uuid::Uuid::new_v4().to_string();
                resources["app_navigation_items"] = json!([
                    {"id":parent,"app_route_app_id":app,"item_type":"group","target_form_uuid":null,"title":"分组","path_slug":"group","sort_order":0,"is_default_entry":false,"parent_id":null,"visibility_rule":null},
                    {"id":child,"app_route_app_id":app,"item_type":"form","target_form_uuid":form,"title":"表单入口","path_slug":"entry","sort_order":1,"is_default_entry":true,"parent_id":parent,"visibility_rule":null}
                ]);
                resources["form_views"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"form_uuid":form,"view_uuid":"source-view-1","name":"视图","config_json":{"customButtons":[{"buttonId":"source-button-1"}]}}]);
                resources["app_custom_buttons"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"button_uuid":"source-button-1","app_route_app_id":app,"form_uuid":form,"name":"关联填写","icon":null,"description":"","action_type":"fill_form","action_config":{"targetAppId":"source-app-3","targetFormUuid":"source-form-3","fieldIds":["name"]},"availability":{"mode":"all","rules":[]},"disabled_reason":""}]);
                resources["automation_flows"] = json!([{"id":uuid::Uuid::new_v4().to_string(),"flow_uuid":"source-flow-1","app_route_app_id":app,"name":"读取跨应用数据","description":null,"status":"paused","current_version":1,"flow_type":"trigger","trigger_form_uuid":form,"trigger_config":{},"nodes_json":[{"id":"read","data":{"kind":"get-many","config":{"sourceMode":"form","formUuid":"source-form-3"}}}],"edges_json":[]}]);
            }
            resources["storageDefinitions"] = json!([]);
            members.push(json!({"appId":app,"resources":resources}));
        }
        let mut bundle = json!({"selectedAppIds":["source-app-1"],"applicationType":"suite","members":members,"dependencies":[]});
        bundle["dependencies"] = json!(dependency_graph(&bundle));
        bundle
    }
    pub fn pack(bundle: &Value, version: &str) -> Vec<u8> {
        use std::io::Write;
        let data = serde_json::to_vec(bundle).unwrap();
        let manifest = json!({"formatVersion":3,"packageType":"yaya-application","packageName":bundle["packageName"],"selectedAppIds":bundle["selectedAppIds"],"version":version,"applicationType":bundle["applicationType"],"members":bundle["members"],"dependencies":bundle["dependencies"],"publication":bundle["publication"],"checksums":{"bundle.json":digest(&data)}});
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in [
            ("manifest.json", serde_json::to_vec(&manifest).unwrap()),
            ("bundle.json", data),
        ] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn views_are_unique_within_their_form_and_default_is_preserved() {
        let mut b = bundle();
        for member in b["members"].as_array_mut().unwrap() {
            let form = member["resources"]["form_definitions"][0]["form_uuid"].clone();
            member["resources"]["form_views"].as_array_mut().unwrap().push(json!({"id":uuid::Uuid::new_v4().to_string(),"form_uuid":form,"view_uuid":"default","name":"全部数据","config_json":{}}));
        }
        assert!(resources(&b).is_ok());
        let mut value =
            json!({"formUuid":"source-form-1","nested":{"viewUuid":"default"},"text":"default"});
        let maps = BTreeMap::from([
            (
                "form_definitions".into(),
                BTreeMap::from([("source-form-1".into(), "local-form".into())]),
            ),
            (
                "form_views".into(),
                BTreeMap::from([(view_identity("source-form-1", "default"), "default".into())]),
            ),
        ]);
        rewrite(&mut value, &maps).unwrap();
        assert_eq!(value["formUuid"], "local-form");
        assert_eq!(value["nested"]["viewUuid"], "default");
        assert_eq!(value["text"], "default");
        let duplicate = b["members"][0]["resources"]["form_views"][1].clone();
        b["members"][0]["resources"]["form_views"]
            .as_array_mut()
            .unwrap()
            .push(duplicate);
        let error = resources(&b).unwrap_err();
        assert!(error.contains("重复配置"));
        assert!(error.contains("default"));
        assert!(error.contains("全部数据"));
    }
    #[test]
    fn rejects_unportable_configuration() {
        for config in [
            json!({"type":"tsx","props":{"code":"return fetch('/api/forms')"}}),
            json!({"type":"attachment","props":{"fileId":"instance-file"}}),
            json!({"kind":"http-request"}),
        ] {
            let mut b = bundle();
            b["members"][0]["resources"]["form_schemas"][0]["schema_json"]["fields"]
                .as_array_mut()
                .unwrap()
                .push(config);
            assert!(resources(&b).is_err());
        }
    }
    #[test]
    fn overlapping_products_and_cycle_entries_are_valid_but_incomplete_cycles_are_not() {
        let original = bundle();
        for root in ["source-app-1", "source-app-2", "source-app-3"] {
            let mut b = original.clone();
            b["selectedAppIds"] = json!([root]);
            assert!(read(&pack(&b, "1.0.0")).is_ok());
        }
        let mut chain = original.clone();
        chain["members"][2]["resources"]["form_schemas"][0]["schema_json"]["fields"]
            .as_array_mut()
            .unwrap()
            .truncate(1);
        chain["dependencies"] = json!(dependency_graph(&chain));
        assert!(resources(&chain).is_ok());
        let mut subset = chain.clone();
        subset["selectedAppIds"] = json!(["source-app-2"]);
        subset["members"].as_array_mut().unwrap().remove(0);
        subset["dependencies"] = json!(dependency_graph(&subset));
        assert!(read(&pack(&subset, "1.0.0")).is_ok());
        // If C still points to A, B+C cannot be published as a complete package.
        let mut incomplete = original;
        incomplete["selectedAppIds"] = json!(["source-app-2"]);
        incomplete["members"].as_array_mut().unwrap().remove(0);
        incomplete["dependencies"] = json!(dependency_graph(&incomplete));
        assert!(resources(&incomplete).unwrap_err().contains("引用"));
    }
    #[test]
    fn suite_cycle_is_closed_and_portable() {
        let b = bundle();
        assert!(read(&pack(&b, "1.0.0")).is_ok());
    }
    #[test]
    fn rejects_missing_and_mismatched_associations() {
        let mut b = bundle();
        b["members"].as_array_mut().unwrap().pop();
        assert!(resources(&b).unwrap_err().contains("引用"));
        let mut b = bundle();
        b["members"][0]["resources"]["form_schemas"][0]["schema_json"]["fields"][1]["props"]["associationAppId"] =
            json!("source-app-3");
        assert!(resources(&b).unwrap_err().contains("归属"));
    }
    #[test]
    fn rejects_duplicate_members_and_unreachable_members() {
        let mut b = bundle();
        let m = b["members"][0].clone();
        b["members"].as_array_mut().unwrap().push(m);
        assert!(resources(&b).unwrap_err().contains("重复"));
        let mut b = bundle();
        b["members"][0]["resources"]["form_schemas"][0]["schema_json"]["fields"][1]["props"] =
            json!({});
        assert!(resources(&b).unwrap_err().contains("未选择"));
    }
    #[test]
    fn explicit_members_allow_independent_apps_but_still_require_dependency_closure() {
        let mut b = bundle();
        b["selectedAppIds"] = json!(["source-app-3"]);
        b["members"][2]["resources"]["form_schemas"][0]["schema_json"]["fields"][1]["props"] = json!({});
        b["dependencies"] = json!(dependency_graph(&b));
        assert!(resources(&b).unwrap_err().contains("未选择"));
        b["selectedAppIds"] = json!(["source-app-1", "source-app-3"]);
        assert!(resources(&b).is_ok());
        let (_, decoded) = read(&pack(&b, "1.0.0")).unwrap();
        assert_eq!(decoded["selectedAppIds"], json!(["source-app-1", "source-app-3"]));
        let mut missing = b.clone();
        missing["members"].as_array_mut().unwrap().remove(1);
        assert!(resources(&missing).is_err());
        b["selectedAppIds"] = json!(["unknown"]);
        assert!(resources(&b).unwrap_err().contains("未包含"));
        b["selectedAppIds"] = json!(["source-app-1", "source-app-1"]);
        assert!(resources(&b).unwrap_err().contains("重复"));
        b["selectedAppIds"] = json!("source-app-1");
        assert!(resources(&b).unwrap_err().contains("数组"));
    }
    #[test]
    fn member_selection_has_no_primary_application_and_old_scopes_are_rejected() {
        let mut b = bundle();
        b["selectedAppIds"] = json!(["source-app-3", "source-app-1"]);
        assert!(resources(&b).is_ok());
        let (_, decoded) = read(&pack(&b, "1.0.0")).unwrap();
        assert!(decoded.get("rootAppId").is_none());
        b["rootAppId"] = json!("source-app-1");
        assert!(resources(&b).unwrap_err().contains("旧应用包"));
        b.as_object_mut().unwrap().remove("rootAppId");
        b["additionalAppIds"] = json!([]);
        assert!(resources(&b).unwrap_err().contains("旧应用包"));
        b.as_object_mut().unwrap().remove("additionalAppIds");
        b.as_object_mut().unwrap().remove("selectedAppIds");
        assert!(resources(&b).unwrap_err().contains("所选成员"));
    }
    #[test]
    fn rejects_legacy_and_tampered_packages() {
        let bytes = pack(&bundle(), "1.0.0");
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut manifest = String::new();
        archive
            .by_name("manifest.json")
            .unwrap()
            .read_to_string(&mut manifest)
            .unwrap();
        let mut manifest: Value = serde_json::from_str(&manifest).unwrap();
        manifest["formatVersion"] = json!(2);
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        use std::io::Write;
        zip.start_file("manifest.json", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(&serde_json::to_vec(&manifest).unwrap())
            .unwrap();
        assert!(
            read(&zip.finish().unwrap().into_inner())
                .unwrap_err()
                .contains("V3")
        );
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, data) in [("bundle.json", b"one"), ("undeclared.json", b"two")] {
            zip.start_file(name, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(data).unwrap();
        }
        assert!(read(&zip.finish().unwrap().into_inner()).is_err());
    }
    #[test]
    fn rewrite_only_reference_slots() {
        let mut v = json!({"associationAppId":"a","associationFormId":"f","label":"f","fields":[{"id":"f"}]});
        let maps = BTreeMap::from([
            (
                "apps".into(),
                BTreeMap::from([("a".into(), "local-a".into())]),
            ),
            (
                "form_definitions".into(),
                BTreeMap::from([("f".into(), "local-f".into())]),
            ),
        ]);
        rewrite(&mut v, &maps).unwrap();
        assert_eq!(v["associationFormId"], "local-f");
        assert_eq!(v["associationAppId"], "local-a");
        assert_eq!(v["label"], "f");
        assert_eq!(v["fields"][0]["id"], "f");
    }

    #[test]
    fn condition_paths_survive_form_identity_remapping() {
        // Field and node IDs are scoped inside copied resources and are preserved.
        // Only form/application identities are remapped by the package protocol.
        let path = "__condition_path_v2__:{\"root\":\"trigger-1:company\",\"path\":[\"group\",\"contact\"]}";
        let mut value = json!({"associationFormId":"old-form","conditionRules":[{"id":"r","fieldKey":path,"rawValue":"__condition_value_v2__:0"}]});
        let maps = BTreeMap::from([("form_definitions".into(),BTreeMap::from([("old-form".into(),"new-form".into())]))]);
        rewrite(&mut value,&maps).unwrap();
        assert_eq!(value["associationFormId"],"new-form");
        assert_eq!(value["conditionRules"][0]["fieldKey"],path);
        assert_eq!(value["conditionRules"][0]["rawValue"],"__condition_value_v2__:0");
    }
    #[test]
    fn missing_mapping_is_an_error() {
        assert!(rewrite(&mut json!({"targetFormUuid":"missing"}), &BTreeMap::new()).is_err());
    }
}
