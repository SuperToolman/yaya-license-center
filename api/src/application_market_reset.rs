//! Offline explicit market reset; all other product data is preserved.
use super::*;
use serde_json::{Value, json};
use std::path::Path;
pub(super) fn package_files(root: &Path) -> Result<BTreeMap<PathBuf, String>, String> {
    let mut result = BTreeMap::new();
    if !root.exists() {
        return Ok(result);
    }
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    if root.parent().is_none() || root.parent().and_then(Path::parent).is_none() {
        return Err("不能清理磁盘根目录".into());
    }
    fn visit(path: &Path, result: &mut BTreeMap<PathBuf, String>) -> Result<(), String> {
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            if ty.is_symlink() {
                return Err("包目录包含链接，请先人工核对".into());
            }
            let p = entry.path();
            if ty.is_dir() {
                if entry.file_name() != "installed-packages" {
                    return Err(format!("市场目录包含非预期子目录：{}", p.display()));
                }
                visit(&p, result)?;
            } else {
                if !p
                    .extension()
                    .and_then(|s| s.to_str())
                    .is_some_and(|e| matches!(e, "json" | "zip" | "tmp"))
                {
                    return Err(format!("市场目录包含非预期文件：{}", p.display()));
                }
                result.insert(
                    p.clone(),
                    application_protocol::digest(&fs::read(&p).map_err(|e| e.to_string())?),
                );
            }
        }
        Ok(())
    }
    visit(&root, &mut result)?;
    Ok(result)
}

const TABLES: &[&str] = &[
    "application_sales_listing",
    "application_commercial_versions",
    "application_entitlement_sources",
    "application_download_tickets",
    "application_entitlements",
    "application_listing_versions",
    "application_releases",
    "application_products",
];
fn snapshot(db: &Connection) -> Result<Value, String> {
    let mut result = json!({});
    for table in TABLES {
        if !db
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?)",
                [table],
                |r| r.get::<_, bool>(0),
            )
            .map_err(|e| e.to_string())?
        {
            continue;
        }
        let mut stmt = db
            .prepare(&format!("SELECT * FROM {table} ORDER BY 1"))
            .map_err(|e| e.to_string())?;
        let count = stmt.column_count();
        let names: Vec<_> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = stmt
            .query_map([], |r| {
                let mut row = serde_json::Map::new();
                for i in 0..count {
                    row.insert(names[i].clone(), json!(format!("{:?}", r.get_ref(i)?)));
                }
                Ok(Value::Object(row))
            })
            .map_err(|e| e.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        result[*table] = json!(rows);
    }
    Ok(result)
}
pub(crate) fn command(path: &Path, confirmation: Option<&str>) -> Result<Value, String> {
    let mut db = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
        .map_err(|e| e.to_string())?;
    let txn = db
        .transaction_with_behavior(rusqlite::TransactionBehavior::Exclusive)
        .map_err(|e| e.to_string())?;
    let commerce_exists:bool=txn.query_row("SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='application_order_details')",[],|r|r.get(0)).map_err(|e|e.to_string())?;
    let commerce_count: i64 = if commerce_exists {
        txn.query_row("SELECT COUNT(*) FROM application_order_details", [], |r| {
            r.get(0)
        })
        .map_err(|e| e.to_string())?
    } else {
        0
    };
    let data = snapshot(&txn)?;
    let root = application_package_root();
    let files = package_files(&root)?;
    let preview = json!({"database":path.canonicalize().map_err(|e|e.to_string())?.to_string_lossy(),"packageRoot":std::path::absolute(&root).map_err(|e|e.to_string())?.to_string_lossy(),"tables":data.as_object().unwrap().iter().map(|(k,v)|(k.clone(),json!(v.as_array().map_or(0,Vec::len)))).collect::<serde_json::Map<_,_>>(),"packageFiles":files.len(),"applicationOrders":commerce_count,"resetBlocked":commerce_count>0});
    let token = application_protocol::digest(
        &serde_json::to_vec(&json!({"preview":preview,"data":data,"files":files}))
            .map_err(|e| e.to_string())?,
    );
    if let Some(confirmation) = confirmation {
        if commerce_count > 0 {
            return Err("存在应用交易记录，禁止通过旧市场重置工具删除交易关联商品。请先制定交易归档与保留方案。".into());
        }
        if token != confirmation {
            return Err("重置凭据失效，请重新预览".into());
        }
        for table in TABLES {
            if data.get(*table).is_none() {
                continue;
            }
            txn.execute(&format!("DELETE FROM {table}"), [])
                .map_err(|e| e.to_string())?;
        }
        if package_files(&root)? != files {
            return Err("包文件已变化，请重新预览".into());
        }
        txn.commit().map_err(|e| e.to_string())?;
        for path in files.keys() {
            fs::remove_file(path).map_err(|e| e.to_string())?;
        }
        Ok(json!({"executed":true,"preview":preview}))
    } else {
        txn.rollback().map_err(|e| e.to_string())?;
        Ok(json!({"executed":false,"confirmation":token,"preview":preview}))
    }
}
