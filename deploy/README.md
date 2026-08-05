# License Center Deployment

`secrets/` must contain `private.pem`, `public.pem`, and `admin-token.txt`. Do not commit these files.

First deployment uploads the local `secrets/` directory:

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-license-center -Initialize
```

Later deployments retain the remote secrets and SQLite Docker volume:

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-license-center
```

Set `WEB_PORT` or `API_PORT` in remote `deploy/.env` when the default ports are unavailable. The platform must use the API address, for example `http://47.112.107.44:8779`, not the management UI address on `8778`.

默认远端目录为 `/opt/yaya-license-center-service`，与低代码平台部署目录隔离。除非明确迁移已有许可证中心，不要将 `-RemoteDir` 指向低代码平台的部署目录。

许可证中心固定使用 Docker Compose 项目 `yaya-license-center`，且发布不会使用 `--remove-orphans`。因此不会将低代码平台容器识别为孤儿容器或删除。首次升级到此部署逻辑时，脚本只会重建名称与 `-ContainerName` 一致、并带有 `license-center` 服务标签的旧许可证中心容器，以切换到独立项目；其 SQLite 数据卷会保留。

构建时默认通过 `rsproxy` 拉取 Rust 依赖，并在 45 秒网络超时后重试两次。可在服务器 `deploy/.env` 设置 `CARGO_REGISTRIES_CRATES_IO_INDEX` 覆盖为可访问的内部镜像。
