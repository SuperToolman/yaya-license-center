//! Connection to the current local Docker daemon, shared by delivery and monitoring.
use std::{process::Stdio, time::Duration};
use tokio::io::AsyncReadExt;
use tokio::process::Command;

/// Bound both pipes and command duration. Dropping the child kills a timed-out process.
pub(crate) async fn read(args: &[&str], seconds: u64, limit: u64) -> Result<String, String> {
    let mut child = command()
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "当前环境无法启动 Docker，请确认 Docker Desktop 已安装并运行".to_string())?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let result = tokio::time::timeout(Duration::from_secs(seconds), async {
        let capture = async {
            let mut bytes = Vec::new();
            stdout
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| "读取 Docker 数据失败")?;
            Ok::<_, String>(bytes)
        };
        let errors = async {
            let mut bytes = Vec::new();
            stderr
                .take(limit + 1)
                .read_to_end(&mut bytes)
                .await
                .map_err(|_| "读取 Docker 诊断失败")?;
            Ok::<_, String>(bytes)
        };
        let (out, err) = tokio::try_join!(capture, errors)?;
        if (out.len() + err.len()) as u64 > limit {
            return Err("Docker 响应超过读取上限，请缩小查询范围".into());
        }
        let status = child.wait().await.map_err(|_| "等待 Docker 响应失败")?;
        if !status.success() {
            return Err(format!(
                "Docker {} 失败（退出码 {}）：{}",
                args.first().unwrap_or(&"操作"),
                status.code().unwrap_or(-1),
                crate::platform_delivery::docker_diagnostic(&out, &err)
            ));
        }
        Ok(String::from_utf8_lossy(&[out, err].concat()).into_owned())
    })
    .await
    .map_err(|_| "Docker 操作超时，请刷新核对实际状态".to_string())?;
    result
}

pub(crate) fn command() -> Command {
    let binary = std::env::var_os("YAYA_PLATFORM_DOCKER_CLI").unwrap_or_else(|| "docker".into());
    let mut cmd = Command::new(binary);
    // Always use the current environment's local daemon, never a caller-selected host/context.
    #[cfg(windows)]
    cmd.args(["--host", "npipe:////./pipe/docker_engine"]);
    #[cfg(not(windows))]
    cmd.args(["--host", "unix:///var/run/docker.sock"]);
    cmd.env_remove("DOCKER_HOST")
        .env_remove("DOCKER_CONTEXT")
        .kill_on_drop(true);
    if let Some(proxy) = std::env::var_os("YAYA_PLATFORM_DOCKER_HTTPS_PROXY") {
        cmd.env("HTTPS_PROXY", &proxy).env("HTTP_PROXY", proxy);
    }
    cmd
}
