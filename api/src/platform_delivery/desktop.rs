use super::*;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::{ffi::OsString, io::ErrorKind, time::Duration};
use tokio::{process::Command, sync::Semaphore, time::timeout};

const INSTALLER: &str = include_str!("desktop.nsi");
static BUILDS: Semaphore = Semaphore::const_new(2);

pub(super) async fn download(
    State(state): State<AppState>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Result<Response, Failure> {
    let user = admin(&headers, &state).await?;
    platform_versions::check_origin(&headers)?;
    let instance = work(state.clone(), move |db| get_instance(db, &id)).await?;
    if instance.status == "archived" {
        return Err(bad("归档的实例无法生成桌面端安装包"));
    }
    if instance.container_id.is_none() {
        return Err(bad("请先部署平台实例"));
    }
    let url = instance_url(&instance.public_url, instance.web_port, &headers)?;
    let instance_id = Uuid::parse_str(&instance.instance_id).map_err(db_error)?;
    let version = &instance.version;
    if version.is_empty()
        || version.len() > 64
        || !version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
    {
        return Err(bad("实例版本格式无效，无法生成安装包"));
    }
    let _permit = BUILDS.try_acquire().map_err(|_| {
        error(
            StatusCode::SERVICE_UNAVAILABLE,
            "正在生成其他安装包，请稍后重试",
        )
    })?;
    let bytes = build(&instance, instance_id, url).await?;
    work(state, move |db| {
        event(db, &instance.instance_id, "desktop.download", &user.user_id)
    })
    .await?;
    Ok((
        [
            ("content-type", "application/octet-stream".to_string()),
            (
                "content-disposition",
                "attachment; filename=\"yaya-setup.exe\"; filename*=UTF-8''yaya%E4%BD%8E%E4%BB%A3%E7%A0%81%E5%B9%B3%E5%8F%B0.exe".to_string(),
            ),
            ("cache-control", "no-store".to_string()),
        ],
        bytes,
    )
        .into_response())
}

fn instance_url(value: &str, web_port: u16, headers: &HeaderMap) -> Result<reqwest::Url, Failure> {
    let origin = headers.get("origin").and_then(|value| value.to_str().ok()).unwrap_or("");
    let candidate = if value.is_empty() { origin } else { value };
    let mut url = reqwest::Url::parse(candidate)
        .ok()
        .filter(|url| {
            matches!(url.scheme(), "http" | "https")
                && url.host().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .ok_or_else(|| bad("请先为平台实例配置客户可访问的 HTTP(S) 地址"))?;
    if value.is_empty() {
        url.set_scheme("http").map_err(|_| bad("实例访问地址无效"))?;
        url.set_port(Some(web_port)).map_err(|_| bad("无法确定实例访问端口"))?;
    }
    Ok(url)
}

async fn build(
    instance: &InstanceRecord,
    instance_id: Uuid,
    url: reqwest::Url,
) -> Result<Vec<u8>, Failure> {
    let launcher = std::env::var_os("YAYA_DESKTOP_LAUNCHER")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("YAYA_OPERATION_CENTER_RELEASE_DIR")
                .map(|root| PathBuf::from(root).join("bin/yaya-desktop.exe"))
        })
        .ok_or_else(|| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "运营服务尚未配置桌面端打包文件",
            )
        })?;
    let root = std::env::temp_dir().join(format!("yaya-desktop-{}", Uuid::new_v4()));
    tokio::fs::create_dir(&root).await.map_err(db_error)?;
    let directory = local_docker::Directory(root);
    let root = &directory.0;
    tokio::fs::copy(&launcher, root.join("yaya-desktop.exe"))
        .await
        .map_err(|err| {
            eprintln!("Could not copy the desktop launcher: {err}");
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "运营服务无法读取桌面端打包文件",
            )
        })?;
    tokio::fs::write(root.join("instance-url.txt"), url.as_str())
        .await
        .map_err(db_error)?;
    tokio::fs::write(
        root.join("platform-package.json"),
        json!({
            "instanceId": instance.instance_id,
            "packageId": instance.package_id,
            "version": instance.version,
            "imageId": instance.image_id,
        })
        .to_string(),
    )
    .await
    .map_err(db_error)?;

    let script = format!(
        "\u{feff}!define INSTANCE_ID \"{instance_id}\"\n!define INSTANCE_LABEL \"yaya低代码平台\"\n!define PLATFORM_VERSION \"{}\"\n{INSTALLER}",
        instance.version
    );
    tokio::fs::write(root.join("installer.nsi"), script)
        .await
        .map_err(db_error)?;
    let compiler = std::env::var_os("YAYA_DESKTOP_MAKENSIS").unwrap_or_else(default_compiler);
    let mut command = Command::new(compiler);
    command
        .current_dir(root)
        .arg("installer.nsi")
        .kill_on_drop(true);
    #[cfg(windows)]
    command.creation_flags(0x08000000);
    let result = timeout(Duration::from_secs(45), command.output())
        .await
        .map_err(|_| {
            error(
                StatusCode::SERVICE_UNAVAILABLE,
                "生成桌面端安装包超时，请重试",
            )
        })?
        .map_err(|err| {
            eprintln!("Could not start NSIS: {err}");
            if err.kind() == ErrorKind::NotFound {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "运营服务尚未安装 NSIS 打包工具",
                )
            } else {
                error(StatusCode::INTERNAL_SERVER_ERROR, "无法启动桌面端打包工具")
            }
        })?;
    if !result.status.success() {
        eprintln!("NSIS failed: {}", String::from_utf8_lossy(&result.stderr));
        return Err(error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "生成桌面端安装包失败",
        ));
    }
    tokio::fs::read(root.join("yaya-instance-installer.exe"))
        .await
        .map_err(db_error)
}

fn default_compiler() -> OsString {
    #[cfg(windows)]
    if let Some(local_app_data) = std::env::var_os("LOCALAPPDATA") {
        let bundled = PathBuf::from(local_app_data).join("tauri/NSIS/makensis.exe");
        if bundled.is_file() {
            return bundled.into_os_string();
        }
    }
    "makensis".into()
}
