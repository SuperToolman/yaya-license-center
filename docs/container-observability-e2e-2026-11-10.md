# 容器终端、日志与实例监控验收

日期：2026-11-10。真实环境：Windows Docker Desktop 29.9.2 / Linux 容器，运营 Web 8778、API 8779。

## 使用入口

- 平台实例更多操作 → 容器终端，直接进入交互终端标签；点击“连接终端”建立会话。
- 平台实例更多操作 → 容器详情与启停 → 资源监控 / 容器日志 / 交互终端。
- 容器列表 → 详情或日志。
- 部署失败且已创建容器时，部署记录提供“查看失败时段容器日志”，以部署开始时间预填日志范围；容器未创建时明确提示查看部署错误。

## 终端实现

使用 xterm.js、FitAddon、WebSocket 和 Docker Engine Exec TTY，提供 `/bin/sh` 交互会话、键盘输入、Ctrl+C、终端窗口缩放和状态提示。Docker Engine 使用 Windows 固定 named pipe 或 Linux 固定 Unix socket，不开放远程地址输入，不挂载宿主文件，不配置 privileged 或切换容器用户。

连接前和连接后每 10 秒复查管理员会话、容器状态、完整容器 ID、实例标签双重绑定及保护规则。仅运行中、未归档、非部署中的平台客户容器允许连接。必须携带受信任 Origin，跨站 WebSocket 不放行。

单进程最多 8 个终端，每位管理员最多 2 个；单消息 16 KiB、单次输入 8 KiB；输出发送有 5 秒超时、浏览器最多保留 2000 行。无键盘输入 10 分钟关闭连接，窗口缩放、持续输出和心跳不延长空闲计时；单次最长 1 小时。不自动重连或重放输入。

`ops_terminal_sessions` 记录管理员、容器、创建/连接/断开时间、状态、原因、Docker Exec ID、容器内 Shell PID 和清理结果；界面优先显示用户姓名。连接、关闭、失败和 API 重启中断写入运维审计。记录连接元数据，不录制终端内容或输入命令。

主动断开、浏览器关闭、空闲超时或会话失效后请求 Shell 退出，按原 Exec 身份核对退出；必要时只向该容器内原 Shell 发送 HUP/KILL。API 重启将连接标为 interrupted，并通过保存的 Exec 身份清理原 Shell，失败项每 30 秒重试，清理结果可在连接记录查看。这里不是对任意命令和后台进程的沙箱；管理员主动启动的脱离终端后台任务不保证随 Shell 退出。

## 日志实现

日志统一由 Docker Engine 读取，解码 stdout/stderr 多路帧，兼容跨帧 UTF-8，清除 ANSI 色彩控制码。历史读取默认最近 200 行，可传 1–1000 行，单次最多 4 MiB；超过上限显示截断，建议缩小时间范围。下载同一查询范围内的原始日志，不应用浏览器关键词筛选。

提供本地时间起止筛选、stdout/stderr 类型、当前已接收日志关键词搜索、实时日志、暂停滚动和文本下载。暂停滚动不暂停接收；另有“停止接收”。实时界面保留最近 2000 条，单条超长日志分段；最多 16 个并发日志流，单流 64 MiB 或 1 小时后结束；15 秒复查会话并发送心跳，慢接收关闭连接，切换标签或关闭详情释放连接。

Docker TTY 容器在源头已经合并 stdout/stderr，界面显示 combined 并说明无法还原区分。日志内容按纯文本展示，管理员应按业务敏感数据管理，不声称对任意业务日志进行全面脱敏。

## 资源监控实现与口径

后台针对最多 200 个非归档、已绑定实例容器采样，并发 4 个，每轮结束间隔 5 秒；采样失败保留明确错误与 null 指标，不伪造为 0。数据持久化在 SQLite，每轮分批清理 24 小时以前样本；详情展示最近 180 次采样，可关闭每 5 秒刷新，超过 30 秒未更新显示过期提示。

- CPU：Docker CPU 增量 / 系统 CPU 增量 × 核数，单核 100%，多核可超过 100%。
- 内存：usage 减 inactive_file / total_inactive_file，展示用量、上限与占比。
- 网络：接收和发送累计字节数、相邻采样增量速率；重启或计数器回退时速率不可用，避免负值和假峰值。
- 磁盘：容器可写层 SizeRw 和文件系统 SizeRootFs，约 60 秒采样一次；**不包含挂载的数据卷占用，不代表客户数据库或附件总大小**，界面明确标注。没有扫描数据卷或宿主磁盘。
- 趋势：CPU、内存、网络收/发；不可用指标保留缺口。
- 异常：OOM、非零退出码、最近 10 分钟重启计数增量至少 3 或采到至少 4 个启动时间，以及 Docker 无法采集提示。采样间隔内未被观测到的手动快速重启不能保证捕获。

Windows Docker Desktop 展示容器/虚拟机环境指标，不将其作为 Windows 宿主机物理指标。宿主与 GPU 页面继续按实际系统能力展示，Linux 真机和真实 GPU 生产验证不包含在本次验收。

## 接口与迁移

所有接口沿用 `/api/ops/local` 管理员权限和审计，不受 Linux 宿主监控开关影响。

| 路径 | 行为 |
| --- | --- |
| GET /api/ops/local/containers/{id}/terminal | WebSocket TTY，消息 input / resize / close |
| GET /api/ops/local/containers/{id}/terminal-sessions | 最近 30 次连接元数据与清理结果 |
| GET /api/ops/local/containers/{id}/logs | since/until（Unix 秒）、tail、stream、download |
| GET /api/ops/local/containers/{id}/logs/live | WebSocket 日志 entry / connected / closed |
| GET /api/ops/local/containers/{id}/metrics | 最新指标、最近 180 次样本、异常提示 |

新增 ops/0005_container_observability.sql、0006_terminal_exec_identity.sql；没有修改历史迁移。新增前端 xterm 依赖及 Rust WebSocket、Hyper 本机 Engine 传输依赖，锁文件同步。代理部署必须转发 WebSocket Upgrade，并允许持续双向流量；没有打开 Docker TCP 端口。

## 真实验收证据

沿用原测试实例 `48f11a2f-240d-4f34-a2ad-ab1f35fd9037`、容器 `yaya-offline-e2e-19044280`，未创建替代实例、未重建容器，未删除卷。

1. 真实 TTY 执行 printf 与 stty size，resize 到 100×35 后得到 35 100。
2. 测试终端向容器 PID 1 标准输出/错误输出写入少量唯一测试标记，实时日志分别得到 stdout 和 stderr；日志时间范围、仅 stdout 查询和下载均验证成功。没有修改客户数据库、配置或数据文件。
3. 关闭终端后查到管理员姓名、连接/断开时间和退出原因，Docker ExecIDs 为 null。
4. 真实浏览器从实例直接连接终端，执行只读命令、调整窗口后断开；验证日志搜索、暂停滚动、实时接收和下载，查看资源趋势。
5. Docker 实际数值采样得到 CPU、内存、网络、SizeRw/SizeRootFs，趋势历史样本持久化。
6. **在真实终端仍连接时重启运营 API**：原会话变为 interrupted，原因“API 重启导致连接中断”，清理结果“Shell 已退出”，Docker ExecIDs 为 null，客户 API 仍健康。
7. 原两个数据卷名称和挂载点逐字比较保持一致，客户 `/healthz` 返回成功；未执行业务备份恢复或全库一致性验证。

本地证据（ignored，不包含管理员密码）：

- `api/runtime/dev/observability-e2e-result.json`
- `api/runtime/dev/terminal-restart-result.json`
- `api/runtime/dev/observability-terminal.png`
- `api/runtime/dev/observability-logs.png`
- `api/runtime/dev/observability-metrics.png`

自动化：运维 Rust 16 项、交付 Rust 17 项、现有页面 31 项及新增页面交互 2 项通过。TypeScript 和修改组件 ESLint 通过。空闲 600 秒边界通过单元测试验证，未人为等待 10 分钟做墙钟验收；权限撤销、跨站来源、错误目标、帧解码、时间范围、计数器回退、重启/退出异常与重启中断均有自动化覆盖。
