//! Public verification material must be readable by the unprivileged platform API.
use super::*;

pub(super) const CONTAINER_PATH: &str = "/tmp/yaya-license-public.pem";

pub(super) fn write(path: &FilePath) -> Result<(), String> {
    let pem = std::env::var("LICENSE_SIGNING_PUBLIC_KEY_PEM").map_err(|_| "验签公钥未配置")?;
    if !pem.contains("BEGIN PUBLIC KEY") || pem.contains("PRIVATE KEY") {
        return Err("验签公钥格式错误".into());
    }
    fs::write(path, pem).map_err(|_| "写入许可证公钥失败")?;
    // systemd uses umask 0027; docker cp preserves the resulting file mode.
    // This is public material, never the license signing private key.
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o644))
            .map_err(|_| "设置许可证公钥读取权限失败")?;
    }
    Ok(())
}

pub(super) async fn repair(container: &str, source: &FilePath) -> Result<(), String> {
    local_docker::run(
        &[
            "cp",
            &source.to_string_lossy(),
            &format!("{container}:{CONTAINER_PATH}"),
        ],
        15,
    )
    .await?;
    local_docker::run(
        &[
            "exec",
            "--user",
            "0",
            container,
            "chmod",
            "0644",
            CONTAINER_PATH,
        ],
        15,
    )
    .await?;
    // Do not let a successful root-only deployment check mask API read failures.
    local_docker::run(
        &["exec", "--user", "yaya", container, "node", "-e",
          "require('node:crypto').createPublicKey(require('node:fs').readFileSync('/tmp/yaya-license-public.pem'))"],
        15,
    ).await.map_err(|_| "实例 API 用户无法读取有效验签公钥，请检查容器文件权限")?;
    Ok(())
}
