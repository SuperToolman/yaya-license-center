//! Publisher-owned immutable document resources, delivered through employee package metadata.
use super::*;
#[cfg(test)]
mod tests;
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct KnowledgeFile {
    pub id: String,
    pub name: String,
    pub size: i64,
    pub sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeManifest {
    pub format_version: u32,
    pub version: String,
    pub documents: Vec<KnowledgeFile>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Upload {
    pub name: String,
    pub client_request_id: String,
}
pub fn migrate(c: &Connection) -> rusqlite::Result<()> {
    c.execute_batch("CREATE TABLE IF NOT EXISTS employee_knowledge_documents(id TEXT PRIMARY KEY,employee_id TEXT NOT NULL,name TEXT NOT NULL,size INTEGER NOT NULL,sha256 TEXT NOT NULL,contents BLOB NOT NULL,active INTEGER NOT NULL DEFAULT 1,client_id TEXT NOT NULL,created_at INTEGER NOT NULL,UNIQUE(employee_id,client_id)); CREATE INDEX IF NOT EXISTS employee_knowledge_owner ON employee_knowledge_documents(employee_id,active); CREATE TRIGGER IF NOT EXISTS delete_employee_knowledge AFTER DELETE ON ai_employee_products BEGIN DELETE FROM employee_knowledge_documents WHERE employee_id=OLD.id; END;")
}
pub fn manifest(c: &Connection, e: &str, version: &str) -> rusqlite::Result<KnowledgeManifest> {
    let mut stmt=c.prepare("SELECT id,name,size,sha256 FROM employee_knowledge_documents WHERE employee_id=? AND active=1 ORDER BY created_at,id")?;
    let documents = stmt
        .query_map([e], |r| {
            Ok(KnowledgeFile {
                id: r.get(0)?,
                name: r.get(1)?,
                size: r.get(2)?,
                sha256: r.get(3)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(KnowledgeManifest {
        format_version: 1,
        version: version.into(),
        documents,
    })
}
type Err = (StatusCode, Json<Envelope<()>>);
fn db(_e: rusqlite::Error) -> Err {
    error(StatusCode::INTERNAL_SERVER_ERROR, "员工知识库存储失败")
}
pub async fn list(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(e): Path<String>,
) -> Result<Json<Envelope<KnowledgeManifest>>, Err> {
    require_admin(&h, &s).await?;
    let c = Connection::open(&s.database_path).map_err(db)?;
    let p = get_ai_employee_product(&c, &e)
        .map_err(db)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "员工不存在"))?;
    Ok(Json(success(
        "员工知识已读取",
        manifest(&c, &e, &p.version).map_err(db)?,
    )))
}
pub async fn upload(
    State(s): State<AppState>,
    h: HeaderMap,
    Path(e): Path<String>,
    Query(q): Query<Upload>,
    bytes: Bytes,
) -> Result<Json<Envelope<KnowledgeManifest>>, Err> {
    require_admin(&h, &s).await?;
    if q.client_request_id.len() > 80
        || Uuid::parse_str(&q.client_request_id).is_err()
        || q.name.is_empty()
        || q.name.len() > 240
        || q.name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\' | ':'))
        || bytes.is_empty()
        || bytes.len() > 8 * 1024 * 1024
    {
        return Err(error(StatusCode::BAD_REQUEST, "文件名无效或文档超过 8 MiB"));
    }
    let ext = q.name.rsplit('.').next().unwrap_or("").to_lowercase();
    let text = matches!(ext.as_str(), "txt" | "md" | "csv" | "tsv")
        && std::str::from_utf8(&bytes).is_ok()
        && !bytes.contains(&0);
    let pdf = ext == "pdf" && bytes.starts_with(b"%PDF-");
    let mut office = false;
    if matches!(ext.as_str(), "docx" | "xlsx" | "ods" | "pptx") {
        if let Ok(mut z) = ZipArchive::new(Cursor::new(&bytes)) {
            let mut total = 0u64;
            if z.len() > 10000 {
                return Err(error(StatusCode::BAD_REQUEST, "文档资源过多"));
            }
            for n in 0..z.len() {
                total = total.saturating_add(
                    z.by_index(n)
                        .map_err(|_| error(StatusCode::BAD_REQUEST, "Office 文档损坏"))?
                        .size(),
                );
            }
            if total > 128 * 1024 * 1024 {
                return Err(error(StatusCode::BAD_REQUEST, "文档解压超限"));
            }
            let key = match ext.as_str() {
                "docx" => "word/document.xml",
                "xlsx" => "xl/workbook.xml",
                "pptx" => "ppt/presentation.xml",
                _ => "content.xml",
            };
            office = z.by_name(key).is_ok();
            if ext == "ods" {
                let mut mime = String::new();
                office = office
                    && z.by_name("mimetype")
                        .ok()
                        .and_then(|f| f.take(100).read_to_string(&mut mime).ok())
                        .is_some()
                    && mime == "application/vnd.oasis.opendocument.spreadsheet";
            } else {
                office = office && z.by_name("[Content_Types].xml").is_ok();
            }
        }
    }
    if !(text || pdf || office) {
        return Err(error(StatusCode::BAD_REQUEST, "文档格式不支持或内容不匹配"));
    }
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let mut c = Connection::open(&s.database_path).map_err(db)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db)?;
    let p = get_ai_employee_product(&tx, &e)
        .map_err(db)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "请先保存员工"))?;
    let old:Option<(String,String)>=tx.query_row("SELECT name,sha256 FROM employee_knowledge_documents WHERE employee_id=? AND client_id=?",params![e,q.client_request_id],|r|Ok((r.get(0)?,r.get(1)?))).optional().map_err(db)?;
    if let Some((name, sha)) = old {
        if name != q.name || sha != hash {
            return Err(error(StatusCode::CONFLICT, "请求 ID 的文档内容不同"));
        }
    } else {
        let (count,size):(i64,i64)=tx.query_row("SELECT count(*),COALESCE(sum(size),0) FROM employee_knowledge_documents WHERE employee_id=? AND active=1",[&e],|r|Ok((r.get(0)?,r.get(1)?))).map_err(db)?;
        if count >= 20 || size + bytes.len() as i64 > 40 * 1024 * 1024 {
            return Err(error(
                StatusCode::BAD_REQUEST,
                "员工知识最多 20 份 / 40 MiB",
            ));
        }
        tx.execute("INSERT INTO employee_knowledge_documents(id,employee_id,name,size,sha256,contents,client_id,created_at) VALUES(?,?,?,?,?,?,?,?)",params![Uuid::new_v4().to_string(),e,q.name,bytes.len() as i64,hash,bytes.to_vec(),q.client_request_id,Utc::now().timestamp()]).map_err(db)?;
        tx.execute(
            "UPDATE ai_employee_products SET version=?,updated_at=? WHERE id=?",
            params![next_patch_version(&p.version), Utc::now().timestamp(), e],
        )
        .map_err(db)?;
    }
    let p = get_ai_employee_product(&tx, &e).map_err(db)?.unwrap();
    let result = manifest(&tx, &e, &p.version).map_err(db)?;
    tx.commit().map_err(db)?;
    Ok(Json(success("知识文档已保存，员工版本已更新", result)))
}
pub async fn remove(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((e, d)): Path<(String, String)>,
) -> Result<Json<Envelope<KnowledgeManifest>>, Err> {
    require_admin(&h, &s).await?;
    let mut c = Connection::open(&s.database_path).map_err(db)?;
    let tx = c
        .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
        .map_err(db)?;
    let p = get_ai_employee_product(&tx, &e)
        .map_err(db)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "员工不存在"))?;
    let n=tx.execute("UPDATE employee_knowledge_documents SET active=0,contents=X'' WHERE id=? AND employee_id=? AND active=1",params![d,e]).map_err(db)?;
    if n > 0 {
        tx.execute(
            "UPDATE ai_employee_products SET version=?,updated_at=? WHERE id=?",
            params![next_patch_version(&p.version), Utc::now().timestamp(), e],
        )
        .map_err(db)?;
    }
    let p = get_ai_employee_product(&tx, &e).map_err(db)?.unwrap();
    let result = manifest(&tx, &e, &p.version).map_err(db)?;
    tx.commit().map_err(db)?;
    Ok(Json(success("知识文档已移除，客户需更新员工", result)))
}
pub async fn download(
    State(s): State<AppState>,
    h: HeaderMap,
    Path((e, d)): Path<(String, String)>,
) -> Result<Response, Err> {
    let token = bearer_token(&h).ok_or_else(|| error(StatusCode::UNAUTHORIZED, "缺少许可证"))?;
    let claims =
        verify_license(token, &s).map_err(|_| error(StatusCode::UNAUTHORIZED, "许可证无效"))?;
    if !claims
        .ai_employees
        .iter()
        .any(|v| v.id == e && v.expires_at > Utc::now().timestamp())
    {
        return Err(error(StatusCode::FORBIDDEN, "未获员工授权"));
    }
    let c = Connection::open(&s.database_path).map_err(db)?;
    if !get_ai_employee_product(&c, &e)
        .map_err(db)?
        .is_some_and(|p| p.status == "active")
    {
        return Err(error(StatusCode::FORBIDDEN, "员工未上架"));
    }
    let bytes:Vec<u8>=c.query_row("SELECT contents FROM employee_knowledge_documents WHERE id=? AND employee_id=? AND active=1",params![d,e],|r|r.get(0)).optional().map_err(db)?.ok_or_else(||error(StatusCode::NOT_FOUND,"文档已撤回，请重新获取员工包"))?;
    Ok(Response::builder()
        .header(CONTENT_TYPE, "application/octet-stream")
        .header("cache-control", "private, no-store")
        .header("x-content-type-options", "nosniff")
        .body(Body::from(bytes))
        .unwrap())
}
