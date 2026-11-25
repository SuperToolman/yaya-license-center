use super::*;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Input {
    alias: String,
    url: String,
    #[serde(default)]
    username: String,
    #[serde(default)]
    password: Option<String>,
    #[serde(default)]
    mirrors: Vec<String>,
    #[serde(default)]
    is_default: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Registry {
    pub registry_id: String,
    pub alias: String,
    pub url: String,
    pub username: String,
    pub has_password: bool,
    pub mirrors: Vec<String>,
    pub is_default: bool,
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Registry> {
    let mirrors: String = r.get(5)?;
    Ok(Registry {
        registry_id: r.get(0)?,
        alias: r.get(1)?,
        url: r.get(2)?,
        username: r.get(3)?,
        has_password: r.get::<_, Option<String>>(4)?.is_some(),
        mirrors: serde_json::from_str(&mirrors).unwrap_or_default(),
        is_default: r.get(6)?,
    })
}
const SELECT: &str =
    "SELECT registry_id,alias,url,username,credentials,mirrors,is_default FROM platform_registries";
fn get(db: &Connection, id: &str) -> Result<Registry, Failure> {
    db.query_row(&format!("{SELECT} WHERE registry_id=?"), [id], row)
        .optional()
        .map_err(db_error)?
        .ok_or_else(|| error(StatusCode::NOT_FOUND, "镜像仓库不存在"))
}
pub(super) fn endpoint(value: &str) -> Result<String, Failure> {
    let url =
        reqwest::Url::parse(value.trim()).map_err(|_| bad("仓库地址须为完整 HTTP/HTTPS 地址"))?;
    if !["https", "http"].contains(&url.scheme())
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
        || value.len() > 500
    {
        return Err(bad(
            "仓库地址只允许 HTTP/HTTPS 协议、主机和端口，不含路径或登录凭据",
        ));
    }
    Ok(url.as_str().trim_end_matches('/').to_string())
}
fn validate(input: &mut Input, id: &str) -> Result<(), Failure> {
    input.alias = input.alias.trim().to_string();
    input.username = input.username.trim().to_string();
    if input.alias.is_empty()
        || input.alias.chars().count() > 80
        || input.username.len() > 256
        || input.username.contains(['\n', '\r', '\0'])
        || input
            .password
            .as_ref()
            .is_some_and(|v| v.len() > 4096 || v.contains(['\n', '\r', '\0']))
    {
        return Err(bad("请填写有效的仓库别名与登录凭据"));
    }
    input.url = endpoint(&input.url)?;
    if id == "dockerhub" && input.url != "https://hub.docker.com" {
        return Err(bad("内置 DockerHub 的仓库地址不可修改"));
    }
    if input.mirrors.len() > 10 || (id != "dockerhub" && !input.mirrors.is_empty()) {
        return Err(bad("仅 DockerHub 支持加速器，最多 10 个"));
    }
    let mut seen = HashSet::new();
    for value in &mut input.mirrors {
        *value = endpoint(value)?;
        if !value.starts_with("https://") || !seen.insert(value.clone()) {
            return Err(bad("加速器须为不重复的 HTTPS 地址"));
        }
    }
    if input.password.as_ref().is_some_and(|v| !v.is_empty()) && input.username.is_empty() {
        return Err(bad("填写密码时请同时填写用户名"));
    }
    Ok(())
}
pub(super) async fn list(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Envelope<Vec<Registry>>>, Failure> {
    admin(&headers, &state).await?;
    let data = work(state, |db| {
        let mut q = db
            .prepare(&format!("{SELECT} ORDER BY registry_id!='dockerhub',alias"))
            .map_err(db_error)?;
        q.query_map([], row)
            .map_err(db_error)?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(db_error)
    })
    .await?;
    Ok(Json(success("镜像仓库", data)))
}
pub(super) async fn create(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(input): Json<Input>,
) -> Result<Json<Envelope<Registry>>, Failure> {
    save(state, headers, Uuid::new_v4().to_string(), input, true).await
}
pub(super) async fn update(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(input): Json<Input>,
) -> Result<Json<Envelope<Registry>>, Failure> {
    save(state, headers, id, input, false).await
}
async fn save(
    state: AppState,
    headers: HeaderMap,
    id: String,
    mut input: Input,
    create: bool,
) -> Result<Json<Envelope<Registry>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    validate(&mut input, &id)?;
    let path = state.database_path.clone();
    let data=work(state,move|db|{
        let tx=db.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate).map_err(db_error)?;
        let old=if create {None} else {Some(get(&tx,&id)?)};
        let duplicate:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM platform_registries WHERE alias=? AND registry_id!=?)",params![input.alias,id],|r|r.get(0)).map_err(db_error)?;
        if duplicate {return Err(error(StatusCode::CONFLICT,"仓库别名已存在"));}
        let old_secret:Option<String>=if old.is_some(){tx.query_row("SELECT credentials FROM platform_registries WHERE registry_id=?",[&id],|r|r.get(0)).map_err(db_error)?}else{None};
        let credentials=if input.username.is_empty(){None}else if let Some(password)=input.password.filter(|v|!v.is_empty()) {
            Some(environment::encrypt(&path,&format!("registry:{id}"),&BTreeMap::from([("password".into(),password)]))?)
        }else if old.as_ref().is_some_and(|v|v.username==input.username && v.url==input.url){old_secret}else{None};
        if !input.username.is_empty() && credentials.is_none() { return Err(bad("新建连接或修改地址/用户名时请填写密码；匿名连接请清空用户名")); }
        if input.is_default {tx.execute("UPDATE platform_registries SET is_default=0 WHERE is_default=1",[]).map_err(db_error)?;}
        let is_default=input.is_default || old.as_ref().is_some_and(|v|v.is_default);
        if create {
            tx.execute("INSERT INTO platform_registries VALUES(?,?,?,?,?,?,?)",params![id,input.alias,input.url,input.username,credentials,serde_json::to_string(&input.mirrors).unwrap(),is_default]).map_err(db_error)?;
        }else{
            tx.execute("UPDATE platform_registries SET alias=?,url=?,username=?,credentials=?,mirrors=?,is_default=? WHERE registry_id=?",params![input.alias,input.url,input.username,credentials,serde_json::to_string(&input.mirrors).unwrap(),is_default,id]).map_err(db_error)?;
        }
        event(&tx,&id,"registry.saved",&user.user_id)?;
        let result=get(&tx,&id)?; tx.commit().map_err(db_error)?; Ok(result)
    }).await?;
    Ok(Json(success("镜像仓库已保存，下次部署生效", data)))
}
pub(super) async fn remove(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<Envelope<bool>>, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    work(state, move |db| {
        let tx = db.transaction().map_err(db_error)?;
        let item = get(&tx, &id)?;
        if id == "dockerhub" || item.is_default {
            return Err(bad(
                "内置 DockerHub 和当前默认仓库不可删除，请先切换默认仓库",
            ));
        }
        tx.execute("DELETE FROM platform_registries WHERE registry_id=?", [&id])
            .map_err(db_error)?;
        event(&tx, &id, "registry.deleted", &user.user_id)?;
        tx.commit().map_err(db_error)?;
        Ok(())
    })
    .await?;
    Ok(Json(success("镜像仓库已删除", true)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_registry_endpoint_without_embedded_credentials_or_paths() {
        assert_eq!(
            endpoint("https://registry.example.com:5443/").unwrap(),
            "https://registry.example.com:5443"
        );
        for invalid in [
            "https://user:secret@registry.example.com",
            "file:///etc/passwd",
            "https://registry.example.com/v2/",
            "https://registry.example.com?token=secret",
        ] {
            assert!(endpoint(invalid).is_err());
        }
    }
}
