# Yaya 运营 / 运维一体化平台升级方案

## 最新部署决定：宿主机统一安装

运营/运维平台自身直接安装在目标 Linux 宿主机，API、管理台、主机监控服务统一发布并由 systemd 托管；客户低代码平台继续在 Docker 中运行。主机监控是平台内部组成，不需要另外接入。本文历史章节中平台继续用 Compose 的方案已被取代，实际入口为 `deploy/publish.ps1`，详见 [发布说明](../deploy/README.md)。

日期：2026-11-09。状态：部分实施，已按实际交付修订；当前边界与下一阶段以本节及开发计划中的最新修订为准。

## 最新修订：交付现状与实施顺序（2026-11-09）

### 已确定的产品要求

- 统一现有侧栏和导航分组，不增加运营/运维双标签。“平台管理”包含版本与客户平台实例，“资源监控”展示当前环境资源。
- 客户平台仅部署到运营平台连接的当前本机 Docker，不要求选择服务器或注册节点；远程节点不属于当前交付范围。
- 发布端构建并验证完整镜像，再导出版本号 + UUID 命名的离线 ZIP；部署端校验、docker load、按镜像标识创建容器，不编译源码，不要求推送仓库，不兼容旧源码包与仓库引用包。
- 镜像仓库设置保留为后续能力，不是离线发布和部署的前置条件。
- 本机采集服务属于内部实现。页面按权限、功能开关、系统支持、服务连接及设备能力分别提示，不要求最终用户手动“接入 主机监控服务”。正式 Linux 安装流程应负责安装与管理该服务。

### 当前已完成与未完成

离线发布 → 真实运营页面上传 → 新建客户平台实例 → 当前 Docker 导入与部署 → Web/API 健康检查 → 重启数据持久化已实测通过，见 [验收记录](./offline-release-e2e-2026-11-09.md)。版本表格已拆列，上传人关联用户姓名，技术摘要放入详情。

会话已落库并实现到期、撤销和账号状态复查；只读运维协议、鉴权、审计、采集服务及页面有基础实现。不能据此宣称完整只读面板已在 Linux/GPU 真机验收。

实例“运行中”目前记录部署结束时的检查结果，尚未周期同步 Docker 状态。容器日志、启停重启、终端、运行中升级、备份恢复和自动 HTTPS 接入均未完整落地。Windows 原生主机/GPU 采集不在当前支持范围；Docker Desktop 运行 Linux 容器的本机 Docker 已可用于部署。

### 修订后的能力边界

1. 本机 Docker 能力单独探测，覆盖 Linux Docker 与 Windows Docker Desktop，不应被仅支持 Linux 的主机/GPU 采集总开关阻断。当前页面尚未完成这种解耦，这是下一阶段任务。
2. 主机/GPU 采集维持 Linux 优先。Docker Desktop 的 Linux 虚拟机指标不能标作 Windows 物理宿主机指标；没有 GPU、驱动缺失、服务不可用分别显示原因。
3. 当前交付模块通过受限参数调用本机 Docker CLI，属于与原“所有执行均经采集服务”的架构偏差。下一阶段先抽取统一 Docker 服务接口，复用环境探测与资源校验；保留 Linux 执行服务适配路径。不能仅把 CLI 移到一个模块，就宣称已完成进程权限隔离。
4. Docker socket 控制权仍属高权限。新增写操作前补齐目标绑定、保护资源、持久化任务、会话/来源校验、审计与结果对账；生产部署的权限收敛另行验收。

### 调整后的推进顺序

离线版本与首次部署已先于原 P1/P2 完成，不代表原 P3 全量完成。后续依次推进：本机容器与实例状态闭环 → 容器终端 → Linux 主机/GPU 实机监控与自动安装 → 备份恢复与版本升级 → 远程节点、历史指标和告警。主机终端在 Linux 权限边界和会话回收验收后开放。

下一阶段具体任务和验收标准见 [开发计划最新修订](./operations-platform-development-plan.md#最新修订下一阶段本机容器与实例状态闭环)。原文中“尚未实施”、内存会话和阶段排期描述属于初始调查背景；与本节冲突时以本节为准。

## 1. 产品定位与范围

目标是在现有商业运营平台中增加类似 1Panel 的服务器运维能力，而不是重写现有系统。首期以 Linux 本机管理体验为优先：安装后能查看本机、管理 Docker 容器、使用终端、查看 NVIDIA GPU。节点模型从第一天保留，后续再开放远程节点接入。

用户已表达“类似于 1Panel”；这并未明确客户服务器托管、多租户运维或 GPU 型号。因此，多客户代运维、非 NVIDIA GPU、复杂集群调度暂列扩展项，而不是首期承诺。

长期业务闭环为：客户 / 订单 → 许可证权益 → 部署实例 → 主机 / 容器 → 健康状态 / 告警 → 运维任务 / 交付记录。

## 2. 当前项目的实际基础

| 层面 | 已确认现状 | 对升级的影响 |
| --- | --- | --- |
| 前端 | Next.js 16、React 19、TypeScript、HeroUI 3、Tailwind 4；已有分组导航 | 保留现有技术栈，在统一导航下新增运维分组，不必更换组件库 |
| 后端 | Rust、Axum 0.8、Tokio；main.rs 约 6,200 行，另有知识库、官网鉴权等模块 | 运维从独立模块起步，避免继续把路由、数据库和执行代码堆进 main.rs |
| 数据 | rusqlite + SQLite，初始化已启用 WAL | 首期可继续使用；高频指标、终端录像不进入业务表 |
| 身份 | Cookie 会话保存在进程内 HashMap，用户和固定角色在数据库 | 重启会丢失会话；需服务端过期、撤销、状态复查和运维授权 |
| 权限 | 五种固定角色；require_admin 允许平台管理员、运营、财务 | 不能把这个方法直接套到容器写操作和终端 |
| 审计 | HTTP 请求/响应审计，管理台来源请求存在跳过逻辑 | 运维动作必须单独记录，不能依赖当前 HTTP 日志 |
| 部署 | 一个镜像运行 Node 与 Rust 两个进程；数据卷及许可证密钥挂载；未挂 Docker socket | 当前没有宿主机管理通路，必须新建边界明确的节点执行服务 |
| 业务 | 客户、订单、财务、许可证、服务商、应用商品、AI 员工、Skills、知识库 | 可复用商业资产，新增“部署实例”连接运营与运维 |
| AI 运行边界 | 本仓库主要管理商品、配置和安装包；文档涉及相邻客户平台的执行机制 | 不能宣称本仓库已有容器运行时、任务隔离或 GPU 调度 |

主要依据：api/src/main.rs、api/Cargo.toml、web/package.json、web/app/components/OperationProvider.tsx、web/app/lib/types/*、web/next.config.mjs、deploy/compose.yaml、deploy/Dockerfile，以及 Skills / 员工版本相关说明。

## 3. 方案选择

| 方案 | 优点 | 代价 | 建议 |
| --- | --- | --- | --- |
| 与现成 1Panel 并行部署、提供跳转 | 最快获得成熟运维能力 | 两套身份、权限、审计；业务数据关联弱 | 可作为过渡方案或独立运维工具 |
| 通过 1Panel API 做适配 | 可借用部分现成功能 | 需验证实际版本的 API 覆盖、授权方式、终端协议、兼容性和版本功能差异 | 仅在愿意长期绑定 1Panel 时选择 |
| 复用本项目 UI / Rust 服务，新增节点 主机监控服务 | 统一体验、可连接商业业务、技术栈连续 | 自己承担执行安全、升级、恢复和测试 | 推荐作为产品主线 |

不建议为了三个功能直接 Fork 整个 1Panel。其公开仓库使用 GPL-3.0，且采用不同实现栈；如果要复制代码或分发派生产品，应先评估许可义务。接口集成与复制代码是不同决策，不应混为一谈。

## 4. 推荐架构

```text
浏览器：运营页面 / 运维页面 / xterm.js
                    │ HTTPS / WSS
统一入口：反向代理，支持 WebSocket Upgrade 和流式响应
                    │
Rust Control Plane：鉴权、资源权限、节点、任务、审计、业务关联
            ├── SQLite：业务与控制元数据
            ├── 大文件存储：任务输出 / 可选终端录像
            └── 节点通道
                  ├── 首期：本机 Unix Socket + OS 权限校验
                  └── 后续：主机监控服务 主动建立 mTLS 长连接
                                  │
                         Rust Host Monitor
                           ├── Docker Engine API
                           ├── 非特权系统账户 PTY / 受控提权
                           └── 主机指标 / NVML
```

控制平面继续用 Rust，主机监控服务 也优先用 Rust，不新增一套 Go / Java 技术栈。主机监控服务 不直接读运营数据库、不持有许可证签名私钥。宿主机特权能力集中到有限的执行接口；不能把整个 Web / API 服务变成 root 执行器。

本机 主机监控服务 推荐 systemd 管理。平台继续用 Compose 部署，通过权限受控的 Unix Socket 连接 主机监控服务。部署时明确控制平面进程的 UID/GID、Socket 所有权和可见范围。

远程节点后续通过一次性注册令牌换取节点证书，再主动连接中心。证书轮换、节点撤销、协议版本和能力声明属于接入协议，不使用长期共用管理员 Token。远程终端、日志和任务通道需多路复用、限流、心跳及背压。

## 5. 容器管理

首期实现：容器列表、过滤、Inspect、启停重启、带边界的删除、实时日志、CPU/内存/网络/块设备统计、容器终端。镜像列表与拉取、卷和网络查看可紧随其后。

主机监控服务 通过 Docker Engine API 操作本机 Unix Socket；Rust 客户端可评估 bollard，并在实现前验证 Docker API 版本协商、日志流与 Exec 支持。常规操作使用结构化 API，不拼接用户输入生成 shell 命令。

区分“接管查看已有容器”和“由平台创建/编排的容器”。平台创建的资源加标签，例如 managed、deployment_id、environment。标签用于识别和展示，不能代替数据库权限校验。既有容器先只读；明确授权后才能变更，不能因为名称相似而自动归属客户。

Docker socket 的控制能力通常接近宿主机 root 权限，read-only 挂载 socket 也不等于只读 Docker API。需要 主机监控服务 的操作授权、资源限制与进程边界；rootless Docker 作为可选部署方式另行验证。

Compose 在下一阶段开放：受控模板 → 参数校验 → 变更预览 → 执行任务 → 健康检查 → 结果记录。任意 Compose 导入可能包含特权容器、宿主目录、host network 等配置，应按高级主机权限处理。

镜像版本记录 digest，变更前保存可恢复配置；旧镜像能回退并不代表数据库迁移能回退。涉及数据库的部署需单独的备份与恢复方案。平台自身容器及关键基础组件设为保护资源，避免普通操作将控制平面停掉。

## 6. 终端

前端使用 @xterm/xterm 与 fit addon，动态客户端加载；xterm.js 只负责终端显示，不承担 SSH、PTY 或鉴权。

容器终端：浏览器 → WSS → Rust 网关 → 主机监控服务 → Docker Exec，支持 TTY、窗口 resize、退出码和断开清理。

主机终端：浏览器 → WSS → 主机监控服务 → 指定非特权系统账户的 PTY。Unix Socket 部署不代表允许任意 root shell；必要提权通过系统策略控制。若要管理没有 主机监控服务 的远程主机，再增加独立 SSH 模式，校验主机指纹并管理凭据。

连接前校验节点/容器范围与终端权限；使用短时、一次性且绑定目标的会话票据，严格校验 WebSocket Origin、有效登录和目标授权。票据不能复用通用 API Token；避免把长期凭据放进 URL 和日志。设置空闲断开、最长会话时间、并发限制、输入输出上限和管理员主动终止。

审计至少记录操作者、目标、开始/结束、提权、退出状态。终端输出记录含敏感信息，应独立加密、限权和保留期管理。PTY 录屏不等于可靠命令审计；交互输入、编辑器和脚本使“按行记录命令”无法保证完整。

## 7. GPU 与主机监控

首期按 NVIDIA 实现 NVML 采集：型号、UUID、驱动信息、利用率、显存、温度、功耗和可获得的进程信息。nvidia-smi 的结构化查询可作为诊断或早期降级手段，不长期解析展示文本。

建议 主机监控服务 每 5–10 秒采样，页面仅在可见时订阅；离线或超过 freshness 阈值时显示“数据过期”，不要显示假零值。没有 GPU、驱动不可用、权限不足、字段不支持分别建模。NVML 用 GPU UUID，涉及 MIG 时加入实例标识，避免只用易变化的 GPU 索引。

初期保留短时内存窗口，显示实时状态，不将每个采样点写进现有业务 SQLite。需要跨天趋势和告警时增加 Prometheus 或 VictoriaMetrics；NVIDIA 数据中心场景可用 DCGM Exporter，按硬件和驱动支持情况启用 MIG / ECC / Xid 等指标。

采集链路需与网络拓扑一致：本机可抓取本机 Exporter；出站连接的远程 主机监控服务 可在本地抓取后 remote_write 到中心，不能假设中心一定能反向访问客户内网的 /metrics。指标端点不公开暴露。

容器与 GPU 进程的关联需要结合宿主 PID / cgroup / 容器信息；MPS、共享 GPU 和驱动能力会影响归属精度。不能把主机 GPU 利用率直接当作某个客户或模型的资源消耗。

监控不等于 GPU 调度。首期不承诺显存硬隔离、公平调度或 GPU 计费。Token/s、首 Token 延迟和推理队列也要接入推理服务指标，不能仅从 GPU 利用率推算。

## 8. 权限、审计和异步任务先行

先保留现有固定商业角色，新增运维角色及明确能力，例如 nodes.read、containers.read、containers.restart、containers.delete、terminal.container、terminal.host、metrics.read、deployments.write，并增加节点/环境范围。

财务、服务商账号默认没有主机终端和容器写权限。角色只决定能力，资源范围再决定“在哪些节点能操作”。前端菜单按能力展示，服务端每次请求和连接都要复查。新增角色还需迁移 users 表中的角色 CHECK 约束，不能仅改前端选项。

当前会话内存结构没有服务端过期字段，且 Cookie 配置未设置 Secure。运维开放前应补齐服务端到期、撤销、账号状态/角色变化后生效、安全 Cookie、敏感操作重新认证或 MFA、写接口 CSRF 防护和 WebSocket Origin 校验。

现有审计按 Origin / Referer 跳过部分管理台请求，不能复用为运维审计。新增 ops_audit_events，由操作服务强制写入：操作者、资源、动作、脱敏参数、前后状态、任务 ID、结果。不能因为请求来自管理台就省略。

耗时操作返回 job_id，而不是阻塞 HTTP 等待拉取或部署完成。任务状态：queued → running → succeeded / failed / cancelled，另有结果未知的 interrupted / unknown 状态。需要超时、心跳、幂等键、执行租约、重启恢复与结果对账；非幂等操作不能断线后盲目重试，也不能宣称端到端 exactly-once。终端会话单独管理，不把交互 shell 当普通后台任务自动重跑。

## 9. 数据与代码结构

建议新增：

- nodes、node_capabilities：节点与能力、最近心跳、主机监控服务 版本。
- projects / environments：资源范围，首期可简化为默认项目与环境。
- deployments、deployment_resources、deployment_revisions：业务部署及实际资源、变更版本。
- ops_jobs、ops_job_events：任务状态、执行日志索引与恢复信息。
- terminal_sessions、ops_audit_events、resource_grants：终端、独立审计及资源授权。

customer_id、license_id、application_product_id / employee_id 可作为 deployments 的可选关联。普通第三方容器不需要伪造订单或许可证。许可证代表商业权益，不直接充当节点证书、主机凭据或运维授权；许可证失效也不应自动停生产容器，需单独定义合同与执行策略。

后端先拆 api/src/ops/{nodes,containers,terminal,metrics,jobs,audit,authz}，再独立 host-monitor crate。逐步提取数据库访问和迁移，不要求先重写全部商业模块。同步 rusqlite 调用集中到阻塞线程/连接管理层；SQLite WAL 保留，出现多副本控制平面或明显写入竞争时再规划 PostgreSQL。

前端沿用 page → components → hooks → lib/types；新增 /ops/overview、/ops/nodes、/ops/containers、/ops/terminal、/ops/gpu、/ops/jobs、/ops/audit。REST 操作继续复用现有请求封装，日志和终端使用独立的流式连接封装。

正式入口统一 HTTPS 域名，反向代理将 /operation-api 与 /ops/ws 等路径交给 Rust。明确配置 Upgrade、空闲超时、流式响应禁缓冲，避免把 Next.js 普通 rewrite 当作已经验证可靠的长连接网关。

## 10. 导航与页面体验

使用统一侧栏及现有导航分组，不增加“运营 / 运维”双标签。保留客户、订单、财务和 AI 商品；“应用产品”更名“平台管理”，新增平台版本管理。管理员资源监控组包含总览、主机、容器、GPU，后续扩展终端、任务和审计；节点选择器位于运维页面顶部。共用账号与视觉组件。

保留本次实现的分组默认收起及 localStorage 记忆。节点选择也可记忆，但每次恢复后必须重新验证资源权限。权限在服务端执行，缓存状态只控制展示。

监控概览采用资源卡片、时间曲线和异常列表；容器采用表格 + 详情页；终端采用标签页；GPU 同时展示实时状态和能力缺失原因。不需要引入新的前端框架来实现这些布局。

## 11. 分阶段交付与验收

| 阶段 | 交付范围 | 主要验收 |
| --- | --- | --- |
| P0：基础边界 | 模块拆分、节点模型、主机监控服务 协议、运维授权、独立审计、任务骨架 | 非运维用户无法调用接口；每个写动作有审计；节点断开可识别 |
| P1：只读面板 | 本机接入、主机概览、容器列表/Inspect/日志/统计、GPU 实时状态 | Linux 真机与 GPU 真机验证；无驱动/无 GPU 可正常使用 |
| P2：可操作面板 | 容器启停重启、受控删除、容器终端、主机终端 | 资源越权、会话到期、Origin、断连回收、审计和保护容器均通过 |
| P3：业务闭环 | 部署实例、受控 Compose、变更记录、健康检查、客户/许可证关联 | 部署失败可诊断；重启后任务可对账；恢复边界清楚 |
| P4：规模化 | 远程节点、证书轮换、历史指标、告警、可选 SSH | 客户内网出站接入、撤销立即生效、指标时序与节点隔离验证 |

P0–P2 形成首个可用的类 1Panel 面板。建议先选择一台非生产 Linux 主机与一台 NVIDIA GPU 主机做纵向验证，再展开更多运维功能。当前未核实主机数量、GPU 型号、开发人数和质量要求，不给固定日历工期承诺。

不列入首期：完整网站建站、数据库管理产品、开放应用商店生态、Kubernetes、跨租户 GPU 调度、AI 自主执行宿主机命令。后续可以让 AI 生成诊断和操作建议，再调用同一套已授权、可审计的任务 API，不能绕过主机权限边界。

## 12. 官方资料与查阅边界

以下为本次实际读取的官方页面或官方仓库资料；外部站点会更新，具体版本实现前仍需锁定验证：

1. 1Panel 文档：https://docs.fit2cloud.com/1panel/ 。涵盖容器、系统、终端、AI、审计与节点管理等产品范围。
2. 1Panel 公开仓库：https://github.com/1Panel-dev/1Panel 。可见 core / agent / frontend 目录与 GPL-3.0 标记；本次未逐项验证其 API 或商业版功能。
3. Docker Daemon 访问保护：https://docs.docker.com/engine/security/protect-access/ 。说明 Unix Socket、SSH、TLS 等连接方式和权限风险。
4. xterm.js 官方安全指南源码：https://raw.githubusercontent.com/xtermjs/xtermjs.org/master/_docs/guides/security.md 。说明终端数据不可信、WebSocket 需要应用层保护、示例不是生产安全实现。
5. xterm.js：https://xtermjs.org/ 。浏览器终端组件。
6. NVIDIA NVML：https://docs.nvidia.com/deploy/nvml-api/nvml-api-reference.html 。GPU 指标及硬件管理 API。
7. NVIDIA DCGM Exporter：https://docs.nvidia.com/datacenter/cloud-native/gpu-telemetry/latest/dcgm-exporter.html 。GPU / MIG 指标与 Prometheus 接入。

Docker Engine API 参考和 bollard 文档在本次访问中出现连接重置，相关库选择属于待 PoC 的建议，不冒充已经完成兼容性验证。未对用户部署环境进行连接、修改或上线。
