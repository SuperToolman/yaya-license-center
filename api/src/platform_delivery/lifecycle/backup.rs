use super::*;
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
const CHUNK: usize = 1024 * 1024;
const RESERVE: u64 = 256 * 1024 * 1024;
#[derive(Clone, Serialize, Deserialize)]
pub(super) struct Archive {
    pub volume: String,
    pub destination: String,
    pub file: String,
    pub sha256: String,
    pub size: u64,
}
fn aad(point: &str, index: usize, sequence: u64) -> Vec<u8> {
    format!("yaya-volume-v1:{point}:{index}:{sequence}").into_bytes()
}
fn cipher(state: &AppState) -> Result<Aes256Gcm, String> {
    Aes256Gcm::new_from_slice(
        &environment::key(&state.database_path, true).map_err(|(_, v)| v.0.message)?,
    )
    .map_err(|_| "备份密钥无效".into())
}
pub(super) async fn helper() -> Result<String, String> {
    let name = std::env::var("YAYA_BACKUP_HELPER_IMAGE").unwrap_or_else(|_| "docker:27-cli".into());
    let v = match engine::json("GET", &format!("/images/{}/json", name), None).await {
        Ok(v) => v,
        Err(_) => {
            let archive = FilePath::new("/app/tools/backup-helper-rootfs.tar");
            if std::env::var_os("YAYA_BACKUP_HELPER_IMAGE").is_some() || !archive.is_file() {
                return Err("备份工具镜像未安装，请预先导入 docker:27-cli 或配置 YAYA_BACKUP_HELPER_IMAGE；任务不会联网拉取".into());
            }
            let tag = "yaya-backup-helper:bundled";
            if engine::json("GET", &format!("/images/{tag}/json"), None)
                .await
                .is_err()
            {
                crate::local_docker::read(&["import", archive.to_str().unwrap(), tag], 120, 65536)
                    .await?;
            }
            engine::json("GET", &format!("/images/{tag}/json"), None).await?
        }
    };
    v["Id"]
        .as_str()
        .map(str::to_owned)
        .ok_or("备份工具镜像无效".into())
}
async fn container(helper: &str, volume: &str, write: bool, name: &str) -> Result<String, String> {
    let mount = json!({"Type":"volume","Source":volume,"Target":"/data","ReadOnly":!write});
    let v=engine::json("POST",&format!("/containers/create?name={name}"),Some(json!({"Image":helper,"Entrypoint":["tar"],"Cmd":if write {vec!["-xpf","-","-C","/data"]}else{vec!["-cpf","-","-C","/data","."]},"AttachStdout":true,"AttachStderr":true,"AttachStdin":write,"OpenStdin":write,"StdinOnce":write,"Labels":{"com.yaya.version-helper":name},"HostConfig":{"NetworkMode":"none","ReadonlyRootfs":true,"AutoRemove":false,"Mounts":[mount],"CapDrop":["ALL"],"CapAdd":["DAC_OVERRIDE","CHOWN","FOWNER"],"SecurityOpt":["no-new-privileges"]}}))).await?;
    v["Id"]
        .as_str()
        .map(str::to_owned)
        .ok_or("备份工具创建失败".into())
}
pub(super) async fn exclusive(v: &Value) -> Result<(), String> {
    let mounts = v["Mounts"].as_array().ok_or("无法读取存储挂载")?;
    if mounts
        .iter()
        .any(|m| m["Type"] != "volume" || m["Name"].as_str().is_none())
    {
        return Err("当前恢复点仅支持显式 Docker 数据卷；请先将 bind/tmpfs 存储改为受管卷".into());
    }
    let all = engine::json("GET", "/containers/json?all=1", None).await?;
    for c in all.as_array().ok_or("容器列表无效")? {
        if c["Id"] == v["Id"] {
            continue;
        }
        for m in c["Mounts"].as_array().into_iter().flatten() {
            if mounts.iter().any(|s| s["Name"] == m["Name"]) {
                return Err("数据卷还被其他容器挂载，请先解除共享；不能保证独立备份和恢复".into());
            }
        }
    }
    Ok(())
}
pub(super) async fn export(
    state: &AppState,
    task: &str,
    point: &str,
    v: &Value,
    helper: &str,
) -> Result<Vec<Archive>, String> {
    let folder = root(state).join(point);
    fs::create_dir_all(&folder).map_err(|_| "备份目录不可写")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))
            .map_err(|e| e.to_string())?;
    }
    let cipher = cipher(state)?;
    let mut files = vec![];
    for (index, m) in v["Mounts"]
        .as_array()
        .ok_or("卷配置无效")?
        .iter()
        .enumerate()
    {
        let volume = m["Name"].as_str().ok_or("只支持 Docker 数据卷备份")?;
        if fs2::available_space(&folder).map_err(|e| e.to_string())? < RESERVE {
            return Err("备份磁盘可用空间不足 256 MiB".into());
        }
        let name = format!("yaya-backup-{task}-{index}");
        let helper_id = container(helper, volume, false, &name).await?;
        let path = folder.join(format!("{index}.ygcm"));
        let result = async {
            let mut options = tokio::fs::OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            let mut out = options.open(&path).await.map_err(|e| e.to_string())?;
            let mut child = crate::local_docker::command()
                .args(["start", "-a", &helper_id])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            let mut input = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let errors = tokio::spawn(async move {
                let mut b = vec![];
                let _ = stderr.take(65536).read_to_end(&mut b).await;
            });
            let mut hash = Sha256::new();
            let mut size = 0u64;
            let mut sequence = 0;
            let mut bytes = vec![0u8; CHUNK];
            loop {
                let n = input.read(&mut bytes).await.map_err(|e| e.to_string())?;
                let terminal = n == 0;
                let uuid = Uuid::new_v4();
                let nonce = &uuid.as_bytes()[..12];
                let encoded = cipher
                    .encrypt(
                        Nonce::from_slice(nonce),
                        Payload {
                            msg: &bytes[..n],
                            aad: &aad(point, index, sequence),
                        },
                    )
                    .map_err(|_| "备份加密失败")?;
                let mut record = vec![];
                record.extend_from_slice(&(encoded.len() as u32).to_be_bytes());
                record.extend_from_slice(nonce);
                record.extend_from_slice(&encoded);
                if fs2::available_space(&folder).map_err(|e| e.to_string())?
                    < RESERVE + record.len() as u64
                {
                    return Err("备份空间不足，原卷保留".into());
                }
                out.write_all(&record).await.map_err(|_| "备份写入失败")?;
                hash.update(&record);
                size += record.len() as u64;
                sequence += 1;
                if terminal {
                    break;
                }
            }
            if !child.wait().await.map_err(|e| e.to_string())?.success() {
                return Err("卷归档工具失败，恢复点不可用".into());
            }
            let _ = errors.await;
            out.sync_all().await.map_err(|e| e.to_string())?;
            Ok::<Archive, String>(Archive {
                volume: volume.into(),
                destination: m["Destination"].as_str().unwrap_or("").into(),
                file: format!("{index}.ygcm"),
                sha256: format!("{:x}", hash.finalize()),
                size,
            })
        };
        let result = tokio::time::timeout(std::time::Duration::from_secs(3600), result)
            .await
            .map_err(|_| "数据卷备份超过一小时".to_string())
            .and_then(|v| v);
        let _ = engine::json("DELETE", &format!("/containers/{helper_id}?force=1"), None).await;
        let archive = result?;
        verify(state, point, index, &archive).await?;
        files.push(archive);
    }
    Ok(files)
}
async fn chunks(
    state: &AppState,
    point: &str,
    index: usize,
    a: &Archive,
    mut output: Option<tokio::process::ChildStdin>,
) -> Result<(), String> {
    if a.file != format!("{index}.ygcm") || Uuid::parse_str(point).is_err() {
        return Err("备份路径无效".into());
    }
    let mut file = tokio::fs::File::open(root(state).join(point).join(&a.file))
        .await
        .map_err(|_| "备份文件缺失")?;
    let cipher = cipher(state)?;
    let mut sequence = 0;
    let mut hash = Sha256::new();
    let mut size = 0;
    loop {
        let mut header = [0u8; 16];
        file.read_exact(&mut header)
            .await
            .map_err(|_| "备份截断或缺少结束标记")?;
        let n = u32::from_be_bytes(header[..4].try_into().unwrap()) as usize;
        if !(16..=CHUNK + 16).contains(&n) {
            return Err("备份分块长度无效".into());
        }
        let mut data = vec![0; n];
        file.read_exact(&mut data).await.map_err(|_| "备份截断")?;
        hash.update(header);
        hash.update(&data);
        size += 16 + n as u64;
        let plain = cipher
            .decrypt(
                Nonce::from_slice(&header[4..]),
                Payload {
                    msg: &data,
                    aad: &aad(point, index, sequence),
                },
            )
            .map_err(|_| "备份验签失败，密钥错误或文件损坏")?;
        sequence += 1;
        if plain.is_empty() {
            let mut end = [0u8; 1];
            if file.read(&mut end).await.map_err(|e| e.to_string())? != 0 {
                return Err("备份结束标记后存在多余数据".into());
            }
            break;
        }
        if let Some(ref mut out) = output {
            out.write_all(&plain)
                .await
                .map_err(|_| "数据卷恢复工具提前退出")?;
        }
    }
    if size != a.size || format!("{:x}", hash.finalize()) != a.sha256 {
        return Err("备份 SHA-256 或大小不一致".into());
    }
    if let Some(mut out) = output {
        out.shutdown().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
pub(super) async fn verify(
    state: &AppState,
    point: &str,
    index: usize,
    a: &Archive,
) -> Result<(), String> {
    chunks(state, point, index, a, None).await
}
pub(super) async fn restore(
    state: &AppState,
    task: &str,
    point: &str,
    index: usize,
    a: &Archive,
    volume: &str,
    helper: &str,
) -> Result<(), String> {
    verify(state, point, index, a).await?;
    let helper_id = container(
        helper,
        volume,
        true,
        &format!("yaya-restore-{task}-{index}"),
    )
    .await?;
    let action = async {
        let mut child = crate::local_docker::command()
            .args(["start", "-ai", &helper_id])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| e.to_string())?;
        chunks(state, point, index, a, child.stdin.take()).await?;
        if !child.wait().await.map_err(|e| e.to_string())?.success() {
            return Err("数据卷恢复失败".into());
        }
        Ok(())
    };
    let result = tokio::time::timeout(std::time::Duration::from_secs(3600), action)
        .await
        .map_err(|_| "数据卷恢复超时".to_string())
        .and_then(|v| v);
    let _ = engine::json("DELETE", &format!("/containers/{helper_id}?force=1"), None).await;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn backup_chunks_detect_corruption_truncation_and_wrong_point() {
        let fixture = crate::platform_delivery::tests::Fixture::new("platform_admin");
        let point = Uuid::new_v4().to_string();
        let folder = root(&fixture.state).join(&point);
        fs::create_dir_all(&folder).unwrap();
        let cipher = cipher(&fixture.state).unwrap();
        let mut bytes = vec![];
        for (sequence, plain) in [b"volume-data".as_slice(), b"".as_slice()]
            .into_iter()
            .enumerate()
        {
            let uuid = Uuid::new_v4();
            let nonce = &uuid.as_bytes()[..12];
            let encrypted = cipher
                .encrypt(
                    Nonce::from_slice(nonce),
                    Payload {
                        msg: plain,
                        aad: &aad(&point, 0, sequence as u64),
                    },
                )
                .unwrap();
            bytes.extend_from_slice(&(encrypted.len() as u32).to_be_bytes());
            bytes.extend_from_slice(nonce);
            bytes.extend_from_slice(&encrypted);
        }
        fs::write(folder.join("0.ygcm"), &bytes).unwrap();
        let archive = Archive {
            volume: "data".into(),
            destination: "/data".into(),
            file: "0.ygcm".into(),
            size: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&bytes)),
        };
        verify(&fixture.state, &point, 0, &archive).await.unwrap();
        let mut corrupt = bytes.clone();
        corrupt[20] ^= 1;
        fs::write(folder.join("0.ygcm"), &corrupt).unwrap();
        assert!(verify(&fixture.state, &point, 0, &archive).await.is_err());
        fs::write(folder.join("0.ygcm"), &bytes[..bytes.len() - 20]).unwrap();
        assert!(verify(&fixture.state, &point, 0, &archive).await.is_err());
    }
}
