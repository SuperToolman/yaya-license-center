# 主机监控服务

`host-monitor/`（原 `agent/`）是独立 Rust 程序 `yaya-host-monitor`，用于 Linux 主机 CPU、内存、运行时间、NVIDIA GPU 采集，以及只读容器列表查询。它不是 AI 智能体，不提供主机终端或任意命令执行接口。

API 的主机节点与指标接口通过 Unix Socket 访问该服务；当前 `/api/ops/local/…` Docker 管理接口直接访问 Docker，不依赖该进程。

```sh
cargo build --release --locked --manifest-path host-monitor/Cargo.toml
python3 host-monitor/scripts/smoke.py --monitor host-monitor/target/release/yaya-host-monitor
```

上述 smoke 验证须在 Linux 实机执行；有真实 GPU 时可追加 `--require-gpu`。

| 配置 | 默认值 | 使用方 |
| --- | --- | --- |
| `YAYA_HOST_MONITOR_SOCKET` | `/run/yaya-host-monitor/monitor.sock` | API 和监控服务 |
| `YAYA_HOST_MONITOR_DATA_DIR` | `/var/lib/yaya-host-monitor` | 监控服务 |
| `YAYA_HOST_MONITOR_ALLOWED_UID` | `10001` | 监控服务，允许连接的 API 宿主 UID |
| `YAYA_OPS_DOCKER_SOCKET` | `/var/run/docker.sock` | 监控服务 |
| `YAYA_OPS_ENABLED` | `false` | API，启用主机监控入口 |

新配置优先，兼容旧 `YAYA_OPS_AGENT_SOCKET`、`YAYA_OPS_AGENT_DATA_DIR`、`YAYA_OPS_AGENT_ALLOWED_UID`。已有旧版 socket 路径需在两端显式配置。若旧默认数据目录已有 identity.json，则继续使用，避免改变节点身份。自定义旧数据目录需要显式配置。

协议 v1 的 JSON 字段 `agentVersion`、数据库列 `agent_identity` 和既有错误码暂时保留，避免破坏页面、历史节点快照和数据库迁移；Rust 类型已改为 HostMonitor 命名。

宿主机发布脚本会将此服务与 API、管理台统一安装、统一升级，由 `yaya-operation-center.target` 管理。安装时使用同一个平台服务账户，自动配置 Socket 权限及允许连接的 UID，并启用主机监控入口；不需要用户单独注册或接入。上表是手动启动程序的默认值，正式发布的数据目录为安装根目录下的 `shared/monitor`。使用方法见 [部署说明](../deploy/README.md)。
