//! Resolve the classic Docker identity without trusting a mutable local tag.
use super::*;
use std::io::Read;

fn read_entry(path: &FilePath, name: &str) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| format!("读取镜像归档失败：{e}"))?;
    let mut archive = tar::Archive::new(file);
    for entry in archive.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        if entry.path().map_err(|e| e.to_string())?.as_ref() != FilePath::new(name) {
            continue;
        }
        if !entry.header().entry_type().is_file() || entry.size() > 8 * 1024 * 1024 {
            return Err("镜像归档元数据类型或大小不合法".into());
        }
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        return Ok(bytes);
    }
    Err(format!("镜像归档缺少 {name}"))
}

pub(super) fn archive_identity(
    expected: &platform_versions::Image,
    path: &FilePath,
) -> Result<String, String> {
    let manifest: serde_json::Value = serde_json::from_slice(&read_entry(path, "manifest.json")?)
        .map_err(|_| "镜像归档 manifest.json 无效")?;
    let images = manifest.as_array().ok_or("镜像归档清单格式无效")?;
    if images.len() != 1
        || !images[0]["RepoTags"]
            .as_array()
            .is_some_and(|tags| tags.iter().any(|tag| tag.as_str() == Some(&expected.tag)))
    {
        return Err("镜像归档必须只包含清单指定标签的一个镜像".into());
    }
    let config_path = images[0]["Config"].as_str().ok_or("镜像归档缺少配置路径")?;
    let digest = config_path
        .strip_prefix("blobs/sha256/")
        .or_else(|| config_path.strip_suffix(".json"))
        .filter(|v| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("镜像归档配置摘要不合法")?;
    let config = read_entry(path, config_path)?;
    if format!("{:x}", Sha256::digest(&config)) != digest {
        return Err("镜像配置 SHA-256 校验失败".into());
    }
    let config_id = format!("sha256:{digest}");
    let config: serde_json::Value =
        serde_json::from_slice(&config).map_err(|_| "镜像配置 JSON 无效")?;
    if format!(
        "{}/{}",
        config["os"].as_str().unwrap_or(""),
        config["architecture"].as_str().unwrap_or("")
    ) != expected.platform
    {
        return Err("镜像归档配置架构与清单不一致".into());
    }
    if expected.id != config_id {
        let index: serde_json::Value = serde_json::from_slice(&read_entry(path, "index.json")?)
            .map_err(|_| "镜像归档 OCI 索引无效")?;
        if !index["manifests"].as_array().is_some_and(|items| {
            items
                .iter()
                .any(|item| item["digest"].as_str() == Some(&expected.id))
        }) {
            return Err("镜像归档索引或配置 ID 与清单不一致".into());
        }
    }
    Ok(config_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(oci: bool, corrupt: bool) -> (Directory, platform_versions::Image, String) {
        let dir =
            Directory(std::env::temp_dir().join(format!("archive-identity-{}", Uuid::new_v4())));
        fs::create_dir(&dir.0).unwrap();
        let config = br#"{"os":"linux","architecture":"amd64"}"#;
        let digest = format!("{:x}", Sha256::digest(config));
        let config_id = format!("sha256:{digest}");
        let id = if oci {
            format!("sha256:{}", "a".repeat(64))
        } else {
            config_id.clone()
        };
        let image = platform_versions::Image {
            id: id.clone(),
            tag: "yaya-platform:test".into(),
            platform: "linux/amd64".into(),
            archive: "image.tar".into(),
            size: 1,
            sha256: String::new(),
            environment: None,
        };
        let config_path = if oci {
            format!("blobs/sha256/{digest}")
        } else {
            format!("{digest}.json")
        };
        let mut tar = tar::Builder::new(fs::File::create(dir.0.join("image.tar")).unwrap());
        for (name, bytes) in [
            (
                "manifest.json".to_string(),
                serde_json::to_vec(
                    &serde_json::json!([{"Config":config_path,"RepoTags":[image.tag]}]),
                )
                .unwrap(),
            ),
            (
                "index.json".to_string(),
                serde_json::to_vec(&serde_json::json!({"manifests":[{"digest":id}]})).unwrap(),
            ),
            (
                config_path,
                if corrupt {
                    b"{}".to_vec()
                } else {
                    config.to_vec()
                },
            ),
        ] {
            let mut header = tar::Header::new_gnu();
            header.set_size(bytes.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            tar.append_data(&mut header, name, bytes.as_slice())
                .unwrap();
        }
        tar.finish().unwrap();
        (dir, image, config_id)
    }

    #[test]
    fn resolves_classic_and_containerd_archive_identities() {
        for oci in [false, true] {
            let (dir, image, id) = fixture(oci, false);
            assert_eq!(
                archive_identity(&image, &dir.0.join("image.tar")).unwrap(),
                id
            );
        }
    }

    #[test]
    fn rejects_corrupt_config_wrong_tag_id_and_platform() {
        let (dir, mut image, _) = fixture(true, false);
        let path = dir.0.join("image.tar");
        image.id = format!("sha256:{}", "b".repeat(64));
        assert!(archive_identity(&image, &path).unwrap_err().contains("ID"));
        image.tag = "other".into();
        assert!(
            archive_identity(&image, &path)
                .unwrap_err()
                .contains("标签")
        );
        let (dir, mut image, _) = fixture(true, false);
        image.platform = "linux/arm64".into();
        assert!(
            archive_identity(&image, &dir.0.join("image.tar"))
                .unwrap_err()
                .contains("架构")
        );
        let (dir, image, _) = fixture(true, true);
        assert!(
            archive_identity(&image, &dir.0.join("image.tar"))
                .unwrap_err()
                .contains("SHA-256")
        );
    }

    #[tokio::test]
    async fn classic_daemon_uses_archive_config_when_oci_index_is_missing() {
        let (_dir, image, config_id) = fixture(true, false);
        let mut calls = Vec::new();
        let result = resolve_imported_identity(&image, &config_id, |id| {
            calls.push(id.clone());
            let result = if id == config_id {
                Ok(format!("{id}|linux/amd64"))
            } else {
                Err("No such image".into())
            };
            std::future::ready(result)
        })
        .await
        .unwrap();
        assert_eq!(result, config_id);
        assert_eq!(calls, vec![image.id.clone(), config_id]);
    }

    #[tokio::test]
    async fn refuses_wrong_identity_or_platform_and_reports_missing_images() {
        let (_dir, image, config_id) = fixture(true, false);
        for identity in [
            format!("{}|linux/arm64", image.id),
            format!("{config_id}|linux/amd64"),
        ] {
            assert!(
                resolve_imported_identity(&image, &config_id, |_| std::future::ready(Ok(
                    identity.clone()
                )))
                .await
                .unwrap_err()
                .contains("不一致")
            );
        }
        let error = resolve_imported_identity(&image, &config_id, |_| {
            std::future::ready(Err("No such image".into()))
        })
        .await
        .unwrap_err();
        assert!(error.contains("镜像已导入"));
        assert!(!error.contains("无法连接"));
    }
}
