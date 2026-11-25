# Yaya 运营 / 运维一体化平台开发计划

> 编制日期：2026-11-09  
> 文档状态：持续迭代；离线发布、本机 Docker 部署、容器管理、终端、日志、实例监控及客户实例版本与恢复点已实现。Linux/GPU 真机验收、通用容器镜像切换和异地备份导入等仍待完成。历史阶段描述以本节最新修订及专项验收记录为准。  
> 适用仓库：`yaya-operation-center`  
> 关联方案：[运营 / 运维一体化平台升级方案](./operations-platform-upgrade-proposal.md)

## 最新部署决定：平台直接安装在宿主机

按用户确认，运营/运维平台自身直接安装到目标 Linux 宿主机，API、管理台和主机监控服务由一套发布脚本安装、升级，并以 `yaya-operation-center.target` 统一管理。首次为初始化安装，后续覆盖更新程序和服务定义，保留持久业务数据与密钥。客户低代码平台继续使用 Docker 容器。此前关于运营平台继续使用 Compose 或把主机监控作为额外可选安装的描述已被此决定取代；旧 Docker 文件只作历史部署参考。实现及验收状态见 [发布说明](../deploy/README.md)。

## 最新修订：下一阶段——本机容器与实例状态闭环

### 2026-11-10：版本与恢复点迭代

客户实例已新增部署修订、加密恢复点、版本变更预览、独立卷升级、失败恢复及历史版本恢复。入口位于客户业务 HeroUI 模态框，具体实现、测试及未覆盖范围见 [容器版本与恢复](./container-version-recovery.md)。普通第三方容器镜像切换、离线恢复包重新导入及自动保留清理仍待后续交付，不能据此标记完整备份产品完成。

客户低代码平台的 Web/API 同一容器部署设计继续保留；运营平台自身改为宿主机服务，以上节最新部署决定为准。现有容器终端、实时日志、实例资源趋势及状态闭环已实现，原文早期“待完成”描述以实际验收记录为准。

### 目标与不变项

围绕已部署客户实例，提供真实 Docker 状态、诊断和可审计的启停重启。保持统一侧栏、本机部署和离线 ZIP；本阶段不增加远程服务器选择，不要求用户配置采集服务，不引入源码或仓库包兼容。以下为待开发计划，不是已实现能力。

### 2026-11-10 增量进展

容器列表已从 Linux 主机采集入口解耦。新增管理员只读接口 GET /api/ops/local/containers，复用部署模块的本机 Docker 连接，Windows 使用 Docker Desktop named pipe，Linux 使用本地 Unix socket。保留会话/角色校验及运维审计，不受 YAYA_OPS_ENABLED 限制；不接受远程主机参数。页面支持搜索、手动刷新、可见时定时刷新、断连提示及恢复。已在当前 Windows Docker Desktop 读取并通过真实页面显示客户测试容器。主机/GPU 页面维持原环境限制；详情、日志、状态对账和写操作仍待完成。

### 2026-11-10 容器工作区增量

容器管理默认进入概述，提供容器、Compose 编排项目、镜像、网络、存储卷和配置标签；概述展示容器分组、资源数量及 Docker 磁盘占用。容器列表包含 CPU/内存、IP 与端口链接，支持按状态筛选、搜索和自动刷新。状态悬停/点击菜单已接入启动、停止、重启、强制停止、暂停、恢复，配套确认、来源校验、后台任务、执行前会话/状态复查、按容器互斥及前后审计。按用户最新要求，平台超级管理员可操作当前 Docker 的非保护容器，不再将写入口限定于平台创建的容器；受保护组件禁止操作。未实现删除、完整编排编辑或容器终端。

Windows Docker Desktop 上使用临时容器完成真实页面操作验收，全部状态操作和日志通过，临时容器已清理，已有客户实例保持运行。主机/GPU、实例长期状态同步和业务实例资源绑定仍待完成。详见 [Docker 容器工作区](./docker-management.md)。下表是原迭代范围，实际进展以增量记录为准，不将本次工作视为 N1–N8 全部完成。

### 实施任务与顺序

| 顺序 | 任务 | 实现要求 | 验收标准 |
| --- | --- | --- | --- |
| N1 | 统一本机 Docker 服务 | 从 platform_delivery/local_docker.rs 提取探测、执行、超时、脱敏及镜像/容器读取接口；部署与容器页面复用，保留平台适配边界 | 不接收用户指定远程 Docker 地址；现有离线部署回归通过；Linux socket 与 Windows named pipe 分别验证 |
| N2 | 解除容器页面总开关限制 | Docker 容器能力独立于 Linux 主机/GPU 采集判断；前端按资源能力渲染，后端同样检查会话和能力 | Windows Docker Desktop 可列真实容器；无 Docker 只提示；Linux 采集不可用不阻断 Docker 列表 |
| N3 | 实例状态同步 | 以持久化 container_id 绑定资源，核对平台实例标签；分别保存部署结果、容器运行态、健康态、最近检查时间；周期查询与手动刷新共享服务 | Docker 手动停止、重启、删除测试容器后，30 秒内或手动刷新后显示变化；Docker 断连显示未知/不可用，不能继续声称运行正常 |
| N4 | 容器只读详情与日志 | 容器列表、名称/镜像/状态/端口/时间、受控 Inspect、受限日志尾部；流式日志需取消、限量、超时和背压 | 支持搜索和详情；只读响应不回显环境密钥；明确日志本身可能含应用敏感内容，仅授权管理员可见；断开页面后停止读取 |
| N5 | 实例与容器关联 | 实例详情直达对应容器和日志，容器详情显示客户/实例关联；第三方容器不推断业务归属 | 以 ID 和标签校验，不能仅按容器名关联；同名替换容器不可误操作 |
| N6 | 容器启停重启 | 首批仅开放平台创建且绑定验证通过的客户容器；任务持久化、Origin/会话与角色检查、按容器互斥、执行前复查、超时及审计；保护运营平台自身和关键组件 | 权限不足不能操作；重复提交不重复执行；操作后按 Docker 实态更新结果；失败有阶段和诊断；不提供删除容器或卷 |
| N7 | 重启恢复与对账 | API 重启后将未结束任务标记中断，核查实际容器状态；不得盲目重放有副作用的操作 | 执行期间重启 API 后，任务结果与实际容器一致或明确标为待核对，不能永久卡在进行中 |
| N8 | 整体验收与文档 | 使用已建的非生产客户测试实例完成页面真实验收；补齐变更前后截图与操作记录 | 列表、日志、停止、启动、重启、外部状态变化、Docker 不可用和权限拒绝均通过；测试不删除客户数据卷 |

日志读取与 Inspect 使用响应白名单，敏感环境配置不返回。通用容器列表可以展示其他容器，但本阶段写权限不自动扩展到任意已有容器。开启写能力前必须明确保护资源识别方式，保护失败时拒绝变更。

### 后续阶段

1. 容器终端：xterm.js、Docker Exec、一次性目标绑定票据、WebSocket Origin、会话到期、窗口调整、断开回收与审计；不以普通 HTTP 命令执行接口替代终端。
2. Linux 主机/GPU：安装脚本与 systemd 自动管理内部采集服务，打通 socket 权限、真实指标和故障诊断；验证无 GPU、有 NVIDIA GPU、驱动缺失。Windows 物理主机监控暂不承诺；不把 Docker Desktop VM 数据当作宿主数据。
3. 升级与恢复：先实现数据库、平台文件和加密密钥的备份与恢复演练，再开放运行中版本升级、变更预览和回退；镜像回退不能替代数据库回退。
4. 远程节点、历史指标、告警、主机终端按依赖逐项推进，不纳入当前容器闭环迭代。

### 对原计划的修正

原文 R0/R1 等任务编号保留供追溯；不能按原顺序重复开发已完成的离线版本/首次部署。原“所有 Docker 执行均进入 主机监控服务”是目标权限边界，当前交付模块尚未满足该拓扑；先抽取接口，不把当前直接 CLI 调用描述为已完成隔离。生产 Linux 部署需继续验证 Web/API 与特权执行服务的边界。

## 目录

> 当前交付：完整离线镜像 ZIP 已真实构建、上传并部署客户实例，验证 Web/API 与重启持久化；不再现场构建或要求仓库。见 [验收记录](./offline-release-e2e-2026-11-09.md)。

1. [目标、范围与交付版本](#1-目标范围与交付版本)
2. [现状约束与实施原则](#2-现状约束与实施原则)
3. [目标架构与信任边界](#3-目标架构与信任边界)
4. [目录、模块与依赖](#4-目录模块与依赖)
5. [身份、权限与资源范围](#5-身份权限与资源范围)
6. [数据库与迁移设计](#6-数据库与迁移设计)
7. [控制平面 API 契约](#7-控制平面-api-契约)
8. [主机监控服务 通信与执行协议](#8-主机监控服务-通信与执行协议)
9. [持久化任务与恢复](#9-持久化任务与恢复)
10. [容器管理落地设计](#10-容器管理落地设计)
11. [终端落地设计](#11-终端落地设计)
12. [主机与 GPU 监控](#12-主机与-gpu-监控)
13. [前端页面与交互](#13-前端页面与交互)
14. [部署实例与商业业务关联](#14-部署实例与商业业务关联)
15. [部署、配置与升级](#15-部署配置与升级)
16. [分阶段任务清单](#16-分阶段任务清单)
17. [测试与验收矩阵](#17-测试与验收矩阵)
18. [发布、回退与运行手册](#18-发布回退与运行手册)
19. [工作量、依赖与风险](#19-工作量依赖与风险)
20. [首个迭代的执行清单](#20-首个迭代的执行清单)

## 1. 目标、范围与交付版本

### 1.1 产品目标

在既有客户、订单、财务、许可证、应用、AI 员工和 Skills 管理基础上，增加类似 1Panel 的服务器管理体验。通过同一个平台管理资源并连接交付业务：

```text
客户 / 订单 / 商业权益
          ↓ 可选关联
       部署实例
          ↓
节点 → 容器 / 服务 → 健康 / 日志 / GPU
          ↓
操作任务 → 审计 → 交付和维护记录
```

首期优先管理平台所在的 Linux 宿主机。设计保留节点 ID 和传输抽象，不在首期提前实现完整远程节点体系。用户尚未指定 GPU 型号、主机规模、客户托管范围；本文使用默认假设，并在环境验证任务中确认。

### 1.2 版本边界

以下为交付里程碑代号，不直接指定产品 semver；发布时统一更新现有 API、前端和部署版本标识。

| 版本           | 目标                   | 必须交付                                                                 | 不包含                      |
| -------------- | ---------------------- | ------------------------------------------------------------------------ | --------------------------- |
| R0：技术验证   | 消除关键可行性风险     | Linux 主机监控服务、UDS 通信、Docker API、容器流、PTY、NVML 小型验证和结果记录  | 生产开放运维功能            |
| R1：只读面板   | 看清本机资源           | 运维权限、独立审计、本机节点、主机概览、容器列表/日志/统计、GPU 实时卡片 | 资源变更、正式交互终端      |
| R2：可操作面板 | 满足本轮三项核心诉求   | 容器启停重启/受控删除、容器终端、主机终端、任务恢复、部署隔离            | 任意 Compose、远程节点      |
| R3：交付闭环   | 将运维能力连接运营业务 | 部署实例、受控 Compose 模板、变更预览、健康检查、客户/权益关联           | 自动停服催缴、任意应用市场  |
| R4：规模化     | 支持多主机持续运营     | 远程 主机监控服务、证书生命周期、历史指标、告警、节点范围授权                   | Kubernetes、跨租户 GPU 调度 |

R1 只是阶段性交付；R2 才完成“容器管理 + 终端 + GPU 监控”的首期目标，不能以只读页面完成代替最终交付。

### 1.3 首期排除项

- 完整网站建站、数据库管理产品、文件管理器、开放应用商店和防火墙管理。
- Windows 原生宿主机管理；Windows 开发使用 Mock，Linux 真机承担执行验收。
- AMD / Intel GPU 实现、GPU 分配和计费；接口预留厂商字段。
- AI 员工直接获得任意主机 shell；未来 AI 使用同一套授权任务 API。
- 完整高可用控制平面、多副本 SQLite、跨区域调度。
- 保证所有容器都能打开 shell、获取日志或读取精确 GPU 占用；必须呈现能力限制。

## 2. 现状约束与实施原则

### 2.1 已核对的仓库基础

| 文件 / 区域                               | 当前情况                                                  | 实施要求                                             |
| ----------------------------------------- | --------------------------------------------------------- | ---------------------------------------------------- |
| `api/src/main.rs`                         | 约 6,200 行，集中了业务、会话、路由和迁移                 | 新运维代码独立模块；仅提取被运维依赖的公共能力       |
| `api/Cargo.toml`                          | Axum 0.8、Tokio、rusqlite；尚无 WS / Docker / NVML 客户端 | 先验证再锁定依赖版本和特性                           |
| `web/app/(operation)`                     | 领域页面已拆分，已有可取消查询与防重复提交 Hook           | 运维页面延续结构，不把资源列表放入 OperationProvider |
| `web/app/components/OperationSidebar.tsx` | 原生 details 分组，默认收起并缓存                         | 统一导航保留现有行为和缓存兼容                       |
| `web/app/lib/types/common.ts`             | 响应基础为 `{message, data}`                              | 新接口保持基础字段兼容，可增加结构化 error           |
| `api/src/main.rs` 会话                    | 内存 HashMap、固定角色、Cookie 登录                       | 运维公开前实现服务端 TTL、撤销、状态/权限复查        |
| `api/src/main.rs` 审计                    | 按 Origin / Referer 跳过部分管理台请求                    | 新运维审计由业务服务强制写入，不调用该跳过规则       |
| `deploy/Dockerfile` / `entrypoint.sh`     | 单镜像同时启动 Node 和 Rust                               | 开放真实写操作和主机终端前拆开 Web / API 运行容器    |
| `deploy/compose.yaml`                     | 单个业务容器，持久卷名由 CONTAINER_NAME 决定              | 迁移保留原数据卷和密钥，不自动创建空库替代           |
| `deploy/publish.ps1`                      | 当前发布可覆盖源码并保留配置、密钥、数据                  | 增加构建/发布验证和 主机监控服务 独立升级流程               |

### 2.2 实施原则

1. 保留运营 URL、许可证签发/校验、官网客户认证和已有商品接口的兼容性。
2. 新增运维路由使用 `/api/ops` 命名空间；通过环境开关关闭后，不影响运营服务启动。
3. 平台 API 不直接调用宿主 shell、NVML 或 Docker socket；执行全部进入 主机监控服务。
4. 不把角色展示、前端按钮隐藏或 Docker 标签当作后端授权。
5. 写操作先有持久化任务与审计，再调度执行；不能“先执行，稍后补日志”。
6. 监控失败不影响许可证和订单；主机监控服务 故障不阻止运营 API 启动。
7. 所有时间存储为 UTC epoch 毫秒；前端按用户时区展示，首期默认 Asia/Shanghai。已有业务秒级时间字段保持原语义，不静默改变。
8. 不将高频样本、终端字节流塞入运营 SQLite；不为本轮重写所有存量数据库访问。

## 3. 目标架构与信任边界

### 3.1 运行拓扑

```text
浏览器
  └─ HTTPS / WSS → 反向代理
                      ├─ 页面资源 → web 容器（非 root，无 主机监控服务 socket）
                      └─ /operation-api/* → api 容器（非 root）
                                                ├─ 业务 SQLite / 文件
                                                ├─ 许可证签名密钥（仅 API）
                                                └─ UDS → 宿主机 主机监控服务
                                                           ├─ Docker socket
                                                           ├─ 非特权账户 PTY
                                                           └─ /proc、/sys、NVML
```

生产面板只开放一个 HTTPS 入口。客户平台仍可通过兼容地址访问既有许可证/市场/官网 API；不能因关闭旧端口而中断业务。管理路由和对外业务路由在代理上分开编目。

### 3.2 本机 主机监控服务 的边界

- 主机监控服务 默认没有 TCP 监听，仅监听 `/run/yaya-host-monitor/monitor.sock`。
- Socket 目录 root 管理；Socket 模式 `0660`、专用组 `yaya-ops-api`。API 容器通过指定数字 GID 获得访问权，Web 不加入该组。
- 挂载 Socket 所在的受控目录，避免只 bind 单个 socket inode 后，主机监控服务 重启替换 socket 导致容器仍引用旧 inode；目录不能放私钥和任意可写配置。
- 验证本机 UDS peer credential；容器 UID 映射、rootless Docker 与用户命名空间情况必须在 R0 实测，不能只依赖进程看到的用户名。
- 主机监控服务 的节点 ID 写入 `/var/lib/yaya-host-monitor/identity.json`，重启和升级不改变；不以 hostname 唯一识别节点。
- 主机监控服务 能使用 Docker socket 时，仍是接近 root 的可信组件。拆分是缩小暴露面，不宣称它能抵御已完全控制 API 的攻击者。
- 主机监控服务 默认不挂载、不读取运营数据库、许可证私钥和 Skill 环境配置。

### 3.3 隔离和兼容开关

建议配置开关：`YAYA_OPS_ENABLED`、`YAYA_OPS_WRITES_ENABLED`、`YAYA_OPS_HOST_TERMINAL_ENABLED`。默认均为 false，按验收结果逐步开启。

关闭开关时，相关服务不创建执行任务、不建立终端；后端返回 `OPS_DISABLED`，前端呈现禁用原因。开关变更关闭写能力后，排队任务在执行前再次检查并停止派发。关闭终端开关要终止既有主机终端，不只隐藏入口。

## 4. 目录、模块与依赖

### 4.1 目标目录

```text
api/
  src/
    auth/                    # 提取员工会话、权限判断；保留兼容调用入口
    db/                      # 新连接管理、运维迁移；不强改全部历史模块
    ops/
      mod.rs                 # router + OpsState 装配
      error.rs
      authz.rs
      nodes/{routes,service,repository}.rs
      containers/{routes,service}.rs
      metrics/{routes,service,cache}.rs
      terminal/{routes,service,tickets,gateway}.rs
      jobs/{routes,service,repository,dispatcher,recovery}.rs
      audit/{routes,service,repository}.rs
      deployments/           # R3 新增
      host_monitor/{client,local,transport}.rs
  migrations/ops/             # 编号 SQL，含说明与校验和
  tests/ops/                  # 由 tests/ops.rs 汇总，确保 Cargo 实际发现
host-monitor/
  Cargo.toml
  Cargo.lock
  src/
    main.rs
    config.rs
    server.rs
    identity.rs
    executor/{docker,pty,guard,journal}.rs
    collectors/{host,nvidia}.rs
    streams/{logs,terminal,limits}.rs
  tests/
crates/ops-protocol/
  Cargo.toml
  src/{lib,commands,events,capabilities,errors}.rs
web/app/
  (operation)/ops/
    layout.tsx
    overview/
    nodes/
    containers/[containerId]/
    terminal/
    gpu/
    jobs/[jobId]/
    audit/
    deployments/              # R3 新增
  components/ops/
    OpsProvider.tsx
    OpsNodeSelector.tsx
    OpsCapabilityGate.tsx
    OpsConnectionStatus.tsx
  hooks/ops/
    useOpsNode.ts
    useOpsStream.ts
    useOpsJob.ts
  lib/ops/{api,stream,errors}.ts
  lib/types/ops/
deploy/
  compose.ops.yaml
  proxy/nginx.conf
  host-monitor/{install.sh,uninstall.sh,yaya-host-monitor.service,config.example.toml}
  scripts/{backup-ops.sh,verify-ops.sh}
docs/ops/
  environment-baseline.md
  compatibility-matrix.md
  api-contract.md
  host-monitor-protocol.md
  runbook.md
  release-checklist.md
```

`page.tsx` 只组装页面；领域内 `components/`、`hooks/` 负责视图和数据。上图省略这些重复子目录，不意味着把所有代码写进 page.tsx。

### 4.2 Cargo 和构建策略

首期不创建根 Cargo workspace，以免同时改变当前 `api/Cargo.lock`、Docker 构建上下文和开发命令。`api`、`host-monitor` 分别为独立包，通过 path dependency 共享 `crates/ops-protocol`；协议 crate 不依赖业务 API 或 主机监控服务。

提交两个可执行包的 lockfile。Docker 的 API builder 必须复制 `crates/ops-protocol` 并保留相对路径；主机监控服务 在 Linux builder 中独立构建。后续确需 workspace 再单独迁移。

### 4.3 依赖候选和验证项

| 用途           | 候选                             | 开发门槛                                         |
| -------------- | -------------------------------- | ------------------------------------------------ |
| API WS         | Axum `ws` 特性                   | 与现有版本兼容、消息尺寸/连接关闭控制            |
| Docker         | bollard                          | API 版本协商、TTY/非 TTY、日志、Exec、事件流实测 |
| 本地 HTTP / WS | hyper 系列、tokio-tungstenite 等 | 在 UDS 上验证 JSON 与双向流，锁定一套实现        |
| PTY            | portable-pty 或 Linux 专用实现   | 身份切换、进程组、resize、断线回收验证           |
| GPU            | nvml-wrapper 等 NVML binding     | 动态加载、无驱动仍能启动、支持目标卡型           |
| 主机采集       | sysinfo / Linux procfs           | cgroup v1/v2、CPU 增量、磁盘/网络计数器重置      |
| 前端终端       | @xterm/xterm、@xterm/addon-fit   | client-only 加载、销毁、resize、无敏感内容遥测   |
| 图表           | 项目内封装，候选 ECharts         | 仅运维页动态加载、可限制点数与刷新频率           |

R0 形成兼容矩阵后才锁版本；不在计划中虚构已验证的库版本。

## 5. 身份、权限与资源范围

### 5.1 权限模型

判定式：`有效员工会话 AND 角色允许 action AND 资源范围允许 action AND 节点能力支持 AND 功能开关允许`。

角色的能力不能只检查一次登录结果；读取用户当前状态、角色和权限版本。资源范围首期为节点，R3 再引入项目/环境。API 路径必须携带 nodeId，容器使用完整 Docker ID；禁止通过名称模糊匹配绕开范围。

| 能力                                      | platform_admin          | ops_operator（新增） | ops_viewer（新增）   | 运营 / 财务 / 服务商 |
| ----------------------------------------- | ----------------------- | -------------------- | -------------------- | -------------------- |
| nodes.read、containers.read、metrics.read | 所有节点                | 授权节点             | 授权节点             | 默认无               |
| containers.logs                           | 所有节点                | 授权节点             | 授权节点；可单独禁用 | 默认无               |
| containers.start/stop/restart             | 允许，仍受保护策略约束  | 授权节点             | 无                   | 无                   |
| containers.delete                         | 重新认证 + 明确确认     | 默认无；需资源授权   | 无                   | 无                   |
| terminal.container                        | 重新认证                | 授权节点 + 重新认证  | 无                   | 无                   |
| terminal.host                             | 重新认证 + 主机终端开关 | 默认无；需显式授权   | 无                   | 无                   |
| nodes.manage、ops.grants.manage           | 允许                    | 无                   | 无                   | 无                   |
| ops.jobs.read、ops.audit.read             | 所有范围                | 授权节点             | 授权节点             | 无                   |

资源授权记录 actions 上限；用户具备角色能力后仍需匹配授权。platform_admin 可跨节点，但不能绕过被标记为平台自身的保护资源禁令。首期不开放后台编辑任意角色定义，只管理固定角色和节点授权。

### 5.2 会话升级

1. 将员工会话持久化到独立 `staff_sessions`，避免混用现有 `website_sessions`。
2. 新会话使用密码学安全随机 Token，仅持久化 SHA-256 哈希；不保存明文 Token。
3. Cookie 名暂沿用 `license_center_session`；生产设置 HttpOnly、Secure、SameSite=Lax、Path=/，开发明确允许本地 HTTP。
4. 默认绝对有效期 7 天；空闲有效期 12 小时。`last_seen` 最多每分钟落库一次，不按每个 WS 帧更新数据库。
5. 用户停用、密码重置、角色/授权变更增加 `auth_version` 并撤销/重验会话。
6. 升级时旧内存会话不迁移，明确发布公告“需重新登录”；不自动信任旧 Cookie。
7. 新增当前会话退出和敏感操作重新认证接口；重新认证绑定当前员工会话，5 分钟有效，密码限速且不记录密码。
8. REST 每次请求核验；WS/PTY 每 30 秒复查身份和授权，撤销后最长 30 秒内关闭。
9. 生产写接口校验精确 Origin 和会话绑定 CSRF Token；非浏览器业务客户端继续使用原来的独立认证，不能强制它们携带管理台 CSRF。

CSRF Token 可采用短时签名令牌，绑定员工会话哈希、用途和到期时间，由独立 `YAYA_SESSION_CSRF_KEY` 签名；不复用许可证 RSA 私钥或节点 CA。`GET /api/session/csrf` 只向已认证同源请求发放，跨源无凭据读取必须被拒绝。重新认证接口同样需要 CSRF，不能因为“要输入密码”就省略。

本轮先实现密码重新认证。MFA 接口留扩展位置；如果部署策略要求 MFA，则未实现前不得开放对应高权限能力，不能用空实现代替。

### 5.3 非授权资源响应

无有效会话返回 401；已登录但无运维角色返回 403；对无资源范围的具体 node/container 返回 404，避免泄露资源存在。列表从查询层过滤。错误和拒绝同样进入必要的运维安全事件，但不把被拒绝请求派给 主机监控服务。

## 6. 数据库与迁移设计

### 6.1 连接与迁移规则

- 沿用 SQLite 和 WAL。新模块统一配置 busy_timeout、连接级 foreign_keys，并把阻塞操作放入受限 `spawn_blocking` 或连接池。
- 旧库在启用全局外键前先执行完整性检查；不能突然让旧数据导致原业务写入失败。
- 新建 `ops_schema_migrations(version, name, checksum, applied_at_ms)`，每个版本事务化执行，重启幂等，校验和不一致拒绝继续迁移。
- 先完成旧 `initialize_database` 和官网认证初始化，再执行运维迁移；运维迁移失败时不得启动半完成的运维写服务。
- 迁移前通过 SQLite Online Backup API 或停写后的规范备份生成快照；在线时不能只复制 `.sqlite3` 忽略 WAL。
- 新表默认使用 `TEXT` UUID、`INTEGER` 毫秒和 `TEXT` JSON；金额仍使用现有整数分，不迁移为浮点。
- 不对所有历史表做通用 ORM 重写。

### 6.2 迁移批次

| 编号 | 内容                                                                                | 发布阶段 |
| ---- | ----------------------------------------------------------------------------------- | -------- |
| 0001 | 增加 users.auth_version；重建角色 CHECK 支持 ops_operator / ops_viewer              | R1       |
| 0002 | staff_sessions、ops_nodes、ops_resource_grants                                      | R1       |
| 0003 | ops_jobs、ops_job_events、ops_audit_events、ops_dispatch_outbox、ops_resource_locks | R1       |
| 0004 | ops_terminal_sessions、ops_terminal_tickets                                         | R2       |
| 0005 | ops_deployments、ops_deployment_resources、ops_deployment_revisions                 | R3       |
| 0006 | ops_node_credentials、ops_enrollment_tokens、ops_alert_rules / events               | R4       |

0001 重建 users 表时复制全部现有字段、索引、约束和触发器，保留 user_id 和密码哈希；不能只根据本文列出的字段重建。先在旧库副本验证外键、官网认证和既有角色。撤销角色不通过丢弃旧表回退。

### 6.3 R1 核心表字段

| 表                  | 字段和约束                                                                                                                                                                                                                                                              | 索引 / 说明                                                            |
| ------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| staff_sessions      | token_hash PK、user_id、auth_version、created_at_ms、last_seen_at_ms、idle_expires_at_ms、expires_at_ms、reauthenticated_at_ms、revoked_at_ms                                                                                                                           | user_id；expires_at_ms；不存原始 Cookie                                |
| ops_nodes           | id PK、agent_identity UNIQUE、name、transport（local/remote）、status（unknown/online/offline/disabled）、agent_version、protocol_version、capabilities_json、boot_id、last_seen_at_ms、created_at_ms、updated_at_ms                                                    | status；远程地址不接受用户任意输入形成 SSRF                            |
| ops_resource_grants | id PK、user_id、scope_type（node）、scope_id、actions_json、created_by、created_at_ms、revoked_at_ms                                                                                                                                                                    | user_id + scope_type + scope_id；修改必须增加权限版本                  |
| ops_jobs            | id PK、actor_user_id、session_hash、node_id、kind、target_id、idempotency_key、request_hash、status、attempt、lease_epoch、lease_until_ms、deadline_at_ms、cancel_requested_at_ms、progress_json、result_json、error_code、created_at_ms、started_at_ms、finished_at_ms | actor_user_id + idempotency_key UNIQUE；node_id + status；创建时间分页 |
| ops_job_events      | job_id、seq、event_type、payload_json、created_at_ms                                                                                                                                                                                                                    | PK(job_id, seq)；只放结构化事件和有界输出片段                          |
| ops_audit_events    | id PK、actor_user_id、actor_name_snapshot、node_id、resource_type、resource_id、action、phase、outcome、request_id、job_id、metadata_json、created_at_ms                                                                                                                | node_id + created_at_ms；actor + created_at_ms；job_id；不设置级联删除 |
| ops_dispatch_outbox | id PK、job_id UNIQUE、status、available_at_ms、lease_until_ms、attempt、created_at_ms                                                                                                                                                                                   | status + available_at_ms；与任务、意图审计同事务写入                   |

`session_hash` 仅用于执行前检查发起会话是否撤销，不作为凭据传给 主机监控服务。用户和节点被停用后保留任务与审计，不能级联删除历史证据。

实现字段补充：`ops_jobs.execution_id` 为可空 UNIQUE，首次派发时落库后保持稳定；`state_version` 用于状态转换 CAS，避免异步完成覆盖已结束状态。另建 `ops_resource_locks(resource_key PRIMARY KEY, job_id, lease_epoch, lease_until_ms)`，使用数据库事务获取/更新，释放时核对持有 job 和 epoch。

幂等键长度限制 16–128 字符，在任务记录保留期内有效。请求摘要使用规范化后的动作、节点、目标和参数计算；不能直接对不同键序的原始 JSON 字符串计算后误判冲突。完成任务清理前，必须确认 outbox 和资源锁均不再引用它。

### 6.4 R2 / R3 表字段

| 表                       | 字段和约束                                                                                                                                                                                                                            |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| ops_terminal_sessions    | id PK、actor_user_id、staff_session_hash、node_id、target_type（host/container）、target_id、os_user、state、created_at_ms、connected_at_ms、last_activity_at_ms、expires_at_ms、closed_at_ms、close_reason、exit_code、recording_ref |
| ops_terminal_tickets     | ticket_hash PK、terminal_session_id、staff_session_hash、expires_at_ms、consumed_at_ms；事务化一次消费                                                                                                                                |
| ops_deployments          | id PK、node_id、name、environment、customer_id NULL、license_id NULL、product_type NULL、product_id NULL、management_mode（observed/managed）、status、current_revision_id、created_at_ms、updated_at_ms                              |
| ops_deployment_resources | id PK、deployment_id、node_id、resource_type、runtime_id、runtime_name、ownership_verified_at_ms；节点 + 类型 + runtime_id 唯一                                                                                                       |
| ops_deployment_revisions | id PK、deployment_id、revision_number、template_id、template_version、rendered_config_ref、config_hash、image_digests_json、change_summary_json、job_id、status、created_by、created_at_ms；deployment_id + revision_number 唯一      |

实例与客户/许可证是否属于同一商业主体由业务服务事务化校验；逻辑商品没有统一 SQL 表时使用受校验的 product_type + product_id，不建立指向不存在表的外键。当前版本不从商业记录删除级联销毁容器。

### 6.5 数据保留默认值

| 数据                 | 默认保留                  | 实现方式                                 |
| -------------------- | ------------------------- | ---------------------------------------- |
| 运维审计             | 180 天                    | 按批清理，管理员可延长；清理行为另记审计 |
| 完成任务和结构化事件 | 30 天                     | 任务正在运行时禁止清理                   |
| 任务文件输出         | 7 天、单任务 20 MiB       | 达限保留截断标记，不静默吞结果           |
| 实时指标             | 每节点 15 分钟内存窗口    | 环形缓冲；进程重启不承诺保留             |
| 终端票据             | 30 秒有效，消费后定期清理 | 哈希存储，不保留明文                     |
| 终端内容录像         | 默认关闭                  | 启用时单独加密、访问授权与保留策略       |

审计是服务端强制记录，但首期 SQLite 日志不宣称“防篡改”；有合规要求时增加异地只追加存储。

## 7. 控制平面 API 契约

### 7.1 共通规则

- 服务内路径以 `/api/ops` 开头；生产浏览器请求为 `/operation-api/api/ops/...`。WS 同样通过该代理前缀，避免同时维护两套不一致地址。
- 业务接口继续用 `{message, data}`；失败可新增 `error: {code, requestId, details}`。旧前端忽略新增字段即可。
- 分页采用 `limit`（默认 50、最大 200）和不透明 cursor；按稳定字段 + ID 排序。
- 资源变更使用 POST 动作接口返回 202 和 jobId；拒绝请求返回 4xx，不先创建成功任务。
- 写请求携带 `Idempotency-Key` 和 CSRF Token；幂等键绑定操作者和请求摘要，相同键不同内容返回 409。
- 节点离线、Docker 不可用、字段不支持采用不同 error.code。
- HTTP 默认 10 秒；任务执行超时由任务类型配置；日志/终端使用独立长连接限制。
- `details` 不包含 Token、密钥、完整环境变量、未经脱敏的 Inspect。

示例：

```json
{
  "message": "重启任务已创建",
  "data": {
    "jobId": "job_01",
    "status": "queued",
    "eventsUrl": "/api/ops/jobs/job_01/events"
  }
}
```

```json
{
  "message": "目标节点当前离线",
  "data": null,
  "error": {
    "code": "OPS_NODE_OFFLINE",
    "requestId": "req_01",
    "details": { "nodeId": "node_01" }
  }
}
```

### 7.2 API 清单

| 方法       | 路径（省略 /api/ops）                       | 作用                                  | 权限 / 阶段                      |
| ---------- | ------------------------------------------- | ------------------------------------- | -------------------------------- |
| GET        | /context                                    | 当前能力、节点范围、开关与限制        | 有效员工会话；R1                 |
| GET        | /nodes                                      | 只列可见节点                          | nodes.read；R1                   |
| GET        | /nodes/{nodeId}                             | 节点状态、能力、版本、数据新鲜度      | nodes.read；R1                   |
| PATCH      | /nodes/{nodeId}                             | 改显示名、启停接入，不改系统 hostname | nodes.manage；R1                 |
| GET        | /nodes/{nodeId}/containers                  | 容器分页、过滤、状态                  | containers.read；R1              |
| GET        | /nodes/{nodeId}/containers/{id}             | 脱敏详情                              | containers.read；R1              |
| GET        | /nodes/{nodeId}/containers/{id}/logs        | 有界历史日志，非跟随                  | containers.logs；R1              |
| GET        | /nodes/{nodeId}/containers/{id}/logs/events | SSE 跟随流                            | containers.logs；R1              |
| GET        | /nodes/{nodeId}/metrics/latest              | 主机 / GPU 最近样本                   | metrics.read；R1                 |
| GET        | /nodes/{nodeId}/metrics/events              | SSE 样本流                            | metrics.read；R1                 |
| GET        | /nodes/{nodeId}/metrics/window              | 内存窗口，非跨日历史                  | metrics.read；R1                 |
| POST       | /nodes/{nodeId}/containers/{id}/actions     | body.action=start/stop/restart/remove | 对应动作能力；R2                 |
| POST       | /nodes/{nodeId}/images/pull                 | 创建拉取任务                          | images.pull；R3                  |
| GET        | /jobs                                       | 按资源范围过滤的任务列表              | ops.jobs.read；R1                |
| GET        | /jobs/{jobId}                               | 状态和结果                            | ops.jobs.read；R1                |
| GET        | /jobs/{jobId}/events                        | SSE 事件，支持 Last-Event-ID          | ops.jobs.read；R1                |
| POST       | /jobs/{jobId}/cancel                        | 请求取消；不等于立即取消成功          | 动作所有者或管理权限；R2         |
| GET        | /audit                                      | 游标分页、动作/用户/节点筛选          | ops.audit.read；R1               |
| GET / POST | /grants                                     | 查看/创建节点授权                     | ops.grants.manage；R1            |
| DELETE     | /grants/{grantId}                           | 撤销并更新 auth_version               | ops.grants.manage；R1            |
| POST       | /terminal-sessions                          | 校验目标、创建会话和短时票据          | terminal.host/container；R2      |
| GET        | /terminal-sessions/{id}                     | 查询会话状态                          | 本人或审计管理权限；R2           |
| GET        | /terminal-sessions/{id}/ws                  | WebSocket 入口                        | Cookie + Origin + 首帧票据；R2   |
| POST       | /terminal-sessions/{id}/close               | 终止会话                              | 本人或管理权限；R2               |
| GET / POST | /deployments                                | 部署实例列表/创建                     | deployments.read/write；R3       |
| POST       | /deployments/{id}/preview                   | 只生成变更预览，不执行                | deployments.write；R3            |
| POST       | /deployments/{id}/apply                     | 提交已确认配置摘要，生成任务          | deployments.write + 重新认证；R3 |
| GET        | /deployments/{id}/revisions                 | 查看版本与任务                        | deployments.read；R3             |

员工会话补充：`DELETE /api/session`、`GET /api/session/csrf`、`POST /api/session/reauth`。官网 `/api/website/auth/*` 不与这些接口共用员工权限。

### 7.3 SSE 连接规范

- fetch 流式读取，携带 Cookie；只读订阅不以 URL 传长期 Token。
- `event` 类型包含 snapshot、update、gap、heartbeat、error；`id` 为可恢复事件序号。
- 浏览器断线指数退避，最大 30 秒；重新连时重查权限，页面隐藏后停止订阅。
- 任务事件可从持久化 seq 恢复；日志流和指标流只是尽力恢复，发生间隙必须发送 gap。
- 浏览器切换节点或组件卸载立即 Abort；不得出现旧节点数据写回新节点页面。
- 每个请求记录 requestId，SSE 起止使用独立审计，不让 HTTP 审计等待流结束后才记授权结果。

### 7.4 最小错误码清单

| HTTP | error.code                                              | 客户端处理                                 |
| ---- | ------------------------------------------------------- | ------------------------------------------ |
| 401  | SESSION_EXPIRED                                         | 关闭流，要求重新登录                       |
| 403  | OPS_FORBIDDEN / CSRF_INVALID                            | 显示无权限或刷新会话，不自动重试写操作     |
| 403  | REAUTH_REQUIRED                                         | 完成重新认证后由用户重新提交               |
| 404  | OPS_RESOURCE_NOT_FOUND                                  | 提示资源不存在或不可见，不查询其他节点补偿 |
| 409  | OPS_TARGET_CHANGED / OPS_RESOURCE_BUSY                  | 刷新目标或等待已有任务                     |
| 409  | OPS_IDEMPOTENCY_CONFLICT                                | 结束当前提交，不随机换键透明重试           |
| 409  | OPS_PROTECTED_RESOURCE                                  | 显示保护原因及带外维护指引                 |
| 409  | OPS_JOB_NOT_CANCELLABLE                                 | 保留实际运行状态                           |
| 410  | OPS_TERMINAL_TICKET_EXPIRED                             | 重新创建会话；不复用已消费票据             |
| 422  | OPS_CAPABILITY_UNAVAILABLE                              | 展示未支持、驱动或权限原因                 |
| 429  | OPS_QUOTA_EXCEEDED                                      | 显示配额与 Retry-After，释放无用订阅       |
| 503  | OPS_DISABLED / OPS_NODE_OFFLINE / OPS_AUDIT_UNAVAILABLE | 保留现有数据与明确状态，不派发执行         |
| 504  | OPS_AGENT_TIMEOUT                                       | 显示通信超时；已存在 job 时查询 job 状态   |

未分类内部错误返回脱敏的 500 和 requestId，详细原因仅在服务端日志中定位。终端 auth 首帧失败通过受控 WS close code 结束，不能在已 Upgrade 后假装发送普通 HTTP 错误。

## 8. 主机监控服务 通信与执行协议

### 8.1 本机传输选择

R0 验证“HTTP JSON + WS over UDS”。REST 承担握手、命令、快照、状态；内部 WS 承担二进制终端和有界日志流。只保留一种正式传输实现，不同时引入 gRPC 和自定义裸 TCP。

API 通过 `主机监控服务Transport` 抽象调用 `hello`、`snapshot`、`dispatch`、`query_execution`、`open_stream`、`cancel`。后续远程出站长连接实现同一接口，不让业务服务直接知道 UDS 或 TCP。

### 8.2 内部协议端点

| 端点                            | 内容                                                   |
| ------------------------------- | ------------------------------------------------------ |
| GET /v1/hello                   | 节点身份、bootId、协议版本范围、主机监控服务 版本、能力和限制 |
| GET /v1/snapshot                | 有界主机和 GPU 快照                                    |
| GET /v1/containers              | 容器快照；完整 ID，主机监控服务 侧也限制数量                  |
| POST /v1/executions             | 创建带 executionId 的结构化执行                        |
| GET /v1/executions/{id}         | 从 主机监控服务 日志查询已接收/运行/结束结果                  |
| POST /v1/executions/{id}/cancel | 尽力取消，仅支持声明可取消的任务                       |
| GET /v1/streams/{streamId}      | WS Upgrade，绑定既有执行/会话                          |
| POST /v1/sessions/{id}/close    | 明确关闭 PTY 或终端执行                                |

每个内部请求携带 requestId / traceId，禁止开放通用 `run(command: string)`。动作类型由枚举限制，如 container.start、container.restart、image.pull、terminal.open。主机终端属于独立的高权限协议，而不是所有任务都能变相执行 shell。

### 8.3 命令信封

```json
{
  "protocolVersion": 1,
  "requestId": "req_01",
  "executionId": "exe_01",
  "jobId": "job_01",
  "nodeId": "node_01",
  "actor": { "userId": "usr_01" },
  "leaseEpoch": 1,
  "deadlineAtMs": 1794196800000,
  "action": "container.restart",
  "target": { "containerId": "full-docker-container-id" },
  "parameters": { "stopTimeoutSeconds": 10 },
  "parametersHash": "sha256-of-canonical-action-target-parameters"
}
```

示例中的时间和 ID 仅示意，不可作为测试中固定的有效期限。

主机监控服务 校验 nodeId、协议版本、deadline、目标、动作参数和本地保护策略。`actor.userId` 是审计归属，不是 主机监控服务 独立信任的浏览器身份；本机 主机监控服务 信任的是被授权的控制平面通道。

### 8.4 幂等与执行日志

- 主机监控服务 在 `/var/lib/yaya-host-monitor/executions.sqlite3` 保存独立执行日志，不共享运营数据库。
- 接收命令先事务化登记 executionId、request hash、leaseEpoch、state，再调用 Docker / PTY。
- 同 executionId 同内容返回既有状态；同 ID 不同内容拒绝。历史执行保留至少 7 天，控制平面的自动对账窗口不得长于它。
- leaseEpoch 用于拒绝旧派发，不是 Docker 的原子 fencing；执行中的 restart 仍可能在崩溃边界结果未知。
- 重启后扫描未完成记录，优先检查外部状态；无法证明完成就返回 unknown，不自动重复重启或删除。

### 8.5 能力声明和版本协商

能力示例：docker.list、docker.events、docker.exec、terminal.host、metrics.host、gpu.nvidia、gpu.processes、compose.managed。每项包含 available、reason、可选版本和限制。

控制平面与 主机监控服务 选择双方版本范围交集；无交集时节点显示 incompatible，只允许展示版本信息，不允许执行。R4 实施明确的 N/N-1 兼容矩阵；R0/R1 不空口承诺所有旧 主机监控服务 向前兼容。

心跳每 10 秒，超过 30 秒未成功标记 offline；主机监控服务 重启导致 bootId 改变时重置增量指标基线，并关闭旧终端会话状态。快照时间同时记录 capturedAt 与 receivedAt，不能依赖主机时钟绝对同步判断全部状态。

## 9. 持久化任务与恢复

### 9.1 状态机

```text
queued → dispatching → running → succeeded
   │          │           ├── failed
   │          │           ├── cancelled（只在执行方确认停止后）
   │          └── unknown ←┘
   └── cancelled

unknown → 对账确认 → succeeded / failed / cancelled
unknown → 无法证明 → 保持 unknown，管理员查看并决定后续操作
```

`cancelRequestedAt` 是请求属性，不等于状态已经 cancelled。动作进入不可中断阶段后，取消接口返回 `OPS_JOB_NOT_CANCELLABLE` 或标记待安全点停止；不能在 UI 提示“已取消”但后台继续执行。

### 9.2 创建、派发与审计事务

1. 接收请求，验证会话、CSRF、能力、节点、目标、保护策略、重新认证和参数。
2. 事务中写入 job、审计 intent 和 dispatch_outbox。任一失败整体回滚，不派发。
3. 返回 202；Dispatcher 领取 outbox，生成 executionId 和 leaseEpoch。
4. 派发前重查开关、发起账号/会话状态、资源授权、节点在线和目标保护状态。
5. 主机监控服务 持久化接收，再执行；控制平面持久化 accepted / running 事件。
6. 完成后同事务更新 job 和完成审计。控制平面暂时无法落库时保留 主机监控服务 结果，等待对账，页面不虚报成功。
7. 审计存储不可写时，新的写任务和终端连接 fail closed；只读监控可带健康告警继续提供。

本机执行协议的 requestId / executionId / jobId 贯穿前后端和两侧日志。主机监控服务 只接收来自控制平面的动作，不提供浏览器绕过入口。

### 9.3 并发、锁与超时

| 项目                        | 初始默认值    | 说明                                          |
| --------------------------- | ------------- | --------------------------------------------- |
| 节点执行并发                | 4             | 可配置，上限先固定为 16                       |
| 同容器写操作并发            | 1             | 键为 nodeId + 完整容器 ID                     |
| 镜像拉取并发                | 1 / 节点      | 防止耗尽网络和磁盘                            |
| start/stop/restart 任务超时 | 120 秒        | 不把 stopTimeout 与整个任务 deadline 混为一谈 |
| 镜像拉取超时                | 15 分钟       | 进度刷新不无限延长绝对期限                    |
| 租约 / 心跳                 | 30 秒 / 10 秒 | 过期触发查询，不直接重派非幂等动作            |
| 排队任务有效期              | 10 分钟       | 到期不再执行过时的用户意图                    |

资源锁跨 Dispatcher 生效；首期单实例控制平面可使用数据库锁记录或等价机制，但服务重启必须从持久化状态重建，不能仅用内存 Mutex 假装具备恢复能力。

### 9.4 故障处理清单

| 故障点                             | 处理                                                          |
| ---------------------------------- | ------------------------------------------------------------- |
| DB 提交前失败                      | 不执行；客户端可用同幂等键重试                                |
| DB 提交后 API 响应丢失             | 同幂等键返回原 jobId                                          |
| 主机监控服务 接收后响应丢失               | query execution；不创建新 executionId 盲重派                  |
| Docker 完成但 主机监控服务 未记完成即崩溃 | 对账外部状态；restart 不可仅以 running 证明完成，保留 unknown |
| 节点断网                           | 页面显示结果未知；不当作执行失败后自动重试                    |
| 用户撤销权限                       | 排队停止派发；运行中操作在安全点停止；终端 30 秒内断开        |
| 控制平面重启                       | 扫描 dispatching / running，向 主机监控服务 对账，再恢复订阅         |
| 主机监控服务 执行历史已过保留期           | 标记人工核对，不自动创建替代任务                              |

## 10. 容器管理落地设计

### 10.1 列表和详情

列表字段：完整 ID、短 ID、名称、镜像引用/摘要、运行状态、健康状态、创建时间、端口摘要、是否受保护、管理归属、最近样本时间。

支持按名称/镜像搜索，状态和 managed / observed 过滤。名称只用于展示和检索，动作传完整 ID。每次写前 主机监控服务 再 Inspect 验证 ID；容器已被替换时返回目标不存在或变更冲突，不自动操作同名新容器。

Inspect 使用白名单 DTO。默认不返回环境变量值、Registry 凭据、完整宿主挂载细节和敏感标签。需要扩大可见范围时增加独立权限，不将原始 Docker JSON 原样返回。

### 10.2 保护与接管

- 平台 web、api、入口代理、主机监控服务 依赖组件，以及显式配置的数据库/基础容器加入保护清单。
- 保护同时依据受控 Compose 项目/服务身份、平台部署清单和实时容器 ID；用户可自行伪造的 label 不独立承担保护决策。
- 首期对保护资源禁止 stop/restart/remove/terminal；必要维护通过既有带外部署流程操作。
- 发现的第三方容器默认为 observed；本机管理员可明确启用变更管理，记录接管事件。
- 普通用户只能操作数据库授权的容器集合；不能先只筛列表，再允许任意 ID 写接口。
- remove 默认只删除已停止容器；force 和删除关联卷首期禁用，后续单独设计。

### 10.3 日志流

历史默认 tail=200，最大 2,000 行、总响应最大 2 MiB。跟随流最长 30 分钟，达到期限提示重新连接。限制每用户最多 4 条、每节点最多 20 条日志流。

Docker 非 TTY 日志需要解复用 stdout/stderr，TTY 使用原始字节；增量 UTF-8 解码不能把跨帧中文截断。单行超长截断并标记，流量过大时缓冲限额 1 MiB，发送 gap 并限流或断流。

部分 logging driver 不支持读取、容器未记录日志、容器已删除，分别给出原因。日志文本使用安全文本渲染，不转为 innerHTML；不得自动执行终端转义或日志内脚本。

### 10.4 统计

- CPU 根据相邻计数器增量计算；首个样本、计数器回退或重启返回 null，而不是 0。
- 明确 CPU 百分比口径：100% 为一个逻辑核，可能大于 100%；UI 显示可用逻辑核数量。
- 内存显示使用量、限制和百分比，明确是否扣 cache，统一处理 cgroup v1/v2。
- 网络/块设备先显示累计量，再用时间差计算速率；bootId / 容器开始时间变化时重置基线。
- 只采集可见列表/选中详情需要的容器，设置每节点订阅上限；通过 Docker events 更新缓存，断线后全量同步。

## 11. 终端落地设计

### 11.1 建连流程

1. 浏览器 POST terminal-sessions，提交 nodeId、targetType、完整 targetId、允许的 OS 用户和初始 cols/rows。
2. 服务端验证权限、目标、重新认证时效、连接配额与审计可写，创建 pending 会话及 30 秒一次性票据。
3. 返回 sessionId 和明文票据一次，票据仅存在前端内存；不进 localStorage、URL、日志、错误报告或浏览器追踪附件。
4. 浏览器建立同源 WSS，握手先校验精确 Origin、员工 Cookie、sessionId 和归属。
5. 5 秒内必须发送第一条 auth JSON 帧，服务端事务化消费票据，并核对绑定员工会话、目标及有效期。
6. 只有票据校验成功后才调用 主机监控服务 打开 PTY / Exec；未认证的 WS 不占宿主 PTY。
7. 前端收到 ready 后启用输入；断开时清理本地 xterm 和订阅，不能自动创建新 shell 伪装成原会话恢复。

票据用安全随机数生成并哈希存储。首期一个 session 只允许一次 attach；重连需要新建会话，避免多浏览器竞争同一个输入流。

### 11.2 帧协议和连接限额

| 方向 / 类型        | 内容                                    |
| ------------------ | --------------------------------------- |
| 首帧 text auth     | type=auth、ticket                       |
| 双向 binary        | 原始 stdin / stdout 字节                |
| 客户端 text resize | cols 20–400、rows 5–200；节流到 100ms   |
| 客户端 text close  | 显式关闭请求，不传任意 OS signal        |
| 服务端 text ready  | 会话 ID、目标摘要、生命周期限制         |
| 服务端 text exit   | exitCode、signal（若可得）、closeReason |
| 服务端 text error  | code、脱敏 message、requestId           |

单帧最大 64 KiB；输入速率默认 64 KiB/s；输出缓存最大 1 MiB，消费者跟不上就明确断开。每用户最多 3 个、每节点最多 10 个终端。主机终端默认 15 分钟无用户输入断开、绝对 2 小时到期；输出持续刷屏不重置用户空闲时间。

WebSocket ping 只判断连接存活，不作为用户活动。终端输出不进入通用 HTTP response_body 审计，不触发 JSON 响应缓冲中间件。

### 11.3 主机 PTY 与系统账户

- 固定专用非 root 系统账户 `yaya-ops-shell`；默认不加入 docker 组、不配置免密通用 sudo、不读平台 secrets。
- root 主机监控服务 创建 PTY 后，通过经过审查的 Linux 会话启动器完成 setgroups / setgid / setuid、HOME / cwd 设置和新的 session / process group；优先使用成熟实现。
- 不在多线程 Rust 进程的 pre_exec 中运行复杂、可能锁死的任意逻辑；R0 必须验证所选库的身份切换方案，必要时用最小独立 launcher。
- 环境变量从允许列表构造；不继承 主机监控服务、API、许可证和短信凭据。
- 身份切换失败立即退出，禁止“降级”为 root shell。主机监控服务 的配置用户名不能由浏览器随意指定。
- 终止时关闭输入、向已校验进程组发 TERM，宽限后 KILL，回收 wait；避免 PID 复用，验证 starttime / pidfd 等机制。
- root shell 与可任意提权的账户首期不开放；后续若需要，作为独立高级能力设计。

### 11.4 容器 Exec 与清理边界

启动优先使用已声明的 shell 参数数组，如 `/bin/sh`；不对用户输入拼接 `sh -c`。不存在 shell、只读执行策略、已停止容器等返回明确原因。

Docker Exec 没有可直接等同“关闭连接即结束整个进程树”的通用保证。R0 必须实测前后台进程、忽略 EOF、持续输出和容器重启。受支持的终端清理方案需限定执行身份、可信 PID / 进程组和容器归属，必要时使用受控会话 helper。

不能为清理终端而重启整个业务容器，也不能只关闭 WS 就写“进程已终止”。无法确认结束时 session 进入 cleanup_pending，继续显示残留状态并提供受权限约束的处理；未通过清理验收的镜像不宣称支持“受管终端”，首期可禁用该目标终端。适配范围在兼容矩阵中明确，不用生产容器试错。

### 11.5 审计和内容保护

强制记录创建、连接、拒绝、断开、管理员终止、超时、身份和目标、退出结果。默认不记录完整输入输出，界面明确告知。录像作为后续显式配置；不得把 PTY 屏幕记录宣传为可靠的逐命令审计。

xterm 通过客户端动态导入初始化，使用 ResizeObserver 驱动 fit，卸载 dispose；剪贴板读取/写入需要浏览器用户动作，多行粘贴提示针对真实执行风险，不添加无意义的重复弹窗。

## 12. 主机与 GPU 监控

### 12.1 统一样本结构

```json
{
  "nodeId": "node_01",
  "bootId": "boot_01",
  "sequence": 42,
  "capturedAtMs": 1794196800000,
  "receivedAtMs": 1794196800120,
  "host": {
    "cpuPercent": 12.5,
    "memoryUsedBytes": 8589934592,
    "memoryTotalBytes": 34359738368
  },
  "gpuCapability": { "status": "available", "reason": null },
  "gpus": [
    {
      "vendor": "nvidia",
      "uuid": "GPU-example",
      "name": "GPU model",
      "utilizationPercent": 72,
      "memoryUsedBytes": 12884901888,
      "memoryTotalBytes": 25769803776,
      "temperatureCelsius": 63,
      "powerWatts": 185,
      "processesStatus": "unsupported"
    }
  ]
}
```

不可用数值用 null 并附字段状态，不用 -1 或 0 代替。单位在字段名中明确；百分比、字节、瓦特和摄氏度不能混用。样本 seq 每个 bootId 单调递增，缓存丢弃旧 boot / 旧 seq。

### 12.2 采集策略

| 内容                          | 频率              | 限制                                       |
| ----------------------------- | ----------------- | ------------------------------------------ |
| 主机监控服务 身份 / 能力             | 启动和能力变化    | 驱动恢复可重新探测                         |
| 主机 CPU / 内存 / 网络 / 磁盘 | 5 秒              | 读取 OS 指标，不运行重型诊断命令           |
| GPU 实时指标                  | 5 秒              | NVML 动态加载；访问失败不使 主机监控服务 退出     |
| GPU 进程                      | 10 秒，详情打开时 | 限制条数，默认不返回完整命令行             |
| 控制平面健康心跳              | 10 秒             | 不为每次心跳写一条审计                     |
| 前端可见订阅                  | 最快 5 秒         | 同节点复用上游样本，不按浏览器数量重复采集 |

内存环形窗口 15 分钟，节点总数和每样本 GPU 数设限。超过 15 秒无新样本标记 stale；节点 offline 后保留最近数据但附过期状态。页面显示“采样时间”和连接状态。

### 12.3 GPU 状态分类

必须区分 no_device、driver_unavailable、permission_denied、library_missing、unsupported、available。硬件信息不支持某指标时仅该字段缺失，不能整张 GPU 卡片消失。

GPU 标识使用 UUID。MIG 需要父 GPU UUID + instance 标识；初期仅展示检测到的能力，未验证型号不开放 MIG 管理。主机 PID 与容器关系需经过 namespace / cgroup 校验，关联失败显示“归属未知”；不按进程名猜测客户。

### 12.4 R4 历史指标和告警

选择单一时序栈再实现：Prometheus 或 VictoriaMetrics。NVIDIA 数据中心 GPU 可接 DCGM Exporter，校验对应驱动、DCGM 与 GPU 能力。远程出站节点采用本地采集 + remote_write，不要求中心直连客户内网。

首批告警：节点离线、容器不健康、磁盘使用率、高显存占用、高温。阈值可配，支持持续时长、恢复阈值、静默和重复抑制；不要对单个瞬时样本发重复告警。通知渠道、接收人和凭据是配置，不能写死。

历史保留默认 15 天；估算节点数 × GPU 数 × 指标数 × 采样数后确定磁盘配额。GPU 监控不直接承担推理吞吐统计、显存硬隔离或成本结算。

## 13. 前端页面与交互

### 13.1 公共框架

- 保留 `OperationProvider` 的身份和通用刷新职责，新增 OpsProvider 管理可见节点、选择状态与运维连接。
- 沿用统一左侧分组导航，不增加“运营 / 运维”标签切换；“应用产品”更名“平台管理”，增加 `/platform-versions`。资源监控组追加到同一侧栏，运维路由仍挂在现有认证 Layout 内，不更改既有 URL。
- 节点选择先从服务端拿可见范围，再恢复浏览器保存的 nodeId；已撤销或不存在则清空并选可见默认节点。
- localStorage 使用版本化键，例如 `yaya-ops-ui-v1`；只存 nodeId 和展示偏好，不存票据、凭据、终端内容。
- 保留原导航缓存键及分组默认收起；不要因新增导航分组使用户既有展开状态丢失。
- 操作成功后由 job 结果触发目标查询刷新；失败保留界面状态；不对失败写请求做透明自动重试。
- 按现有前端约定使用 HeroUI 语义组件、Tailwind 布局；不覆盖库内部类名，不绕过已有变体补丁。

### 13.2 页面任务与验收

| 页面                 | 内容 / 组件                                               | 验收要求                                              |
| -------------------- | --------------------------------------------------------- | ----------------------------------------------------- |
| /ops/overview        | 主机状态、CPU/内存/磁盘、容器状态分布、GPU 摘要、最近任务 | 读失败保留上次样本并标注 stale；无节点显示安装指引    |
| /ops/nodes           | 节点表、版本、心跳、能力、接入状态                        | 首期只有本机也使用节点模型；未知能力有原因            |
| /ops/containers      | 搜索、过滤、容器表、快捷动作、资源指标                    | 无权限不展示写入口，后端拒绝仍正确反馈                |
| /ops/containers/[id] | 概览/日志/统计/终端/部署关联                              | 所有请求包含 nodeId；切换节点清空旧容器详情与流       |
| /ops/terminal        | 新建目标、标签页、连接状态、关闭会话                      | 明确显示主机/容器及账户，禁止无提示自动重连创建 shell |
| /ops/gpu             | 每卡状态、实时曲线、进程表、能力说明                      | 无 GPU 和驱动错误不崩溃；null 显示“不可用”            |
| /ops/jobs            | 状态筛选、发起人、资源、时间、取消能力                    | unknown 不显示成普通失败；取消以服务端最终结果为准    |
| /ops/jobs/[id]       | 结构化步骤、进度、脱敏输出、审计关联                      | 刷新后可恢复；长输出有截断标识                        |
| /ops/audit           | 时间、用户、节点、动作、结果与任务跳转                    | 严格按资源过滤；不展示秘密和 Cookie                   |
| /ops/deployments     | R3 实例、客户、权益、版本、健康和变更                     | 商品版本与部署 revision 分开展示                      |

### 13.3 响应式与可访问性

桌面 1440px、平板 768px、手机 390px 验证。表格允许受控横向滚动，不能撑破页面。终端在手机上可横向查看并显示键盘/尺寸限制；不承诺手机替代完整桌面运维。

按钮和图标提供可读名称，连接状态不只依赖颜色；弹窗焦点回到触发按钮。图表提供数值摘要。停止、重启、删除要显示完整目标摘要，保护资源解释禁用原因。

## 14. 部署实例与商业业务关联

### 14.1 实例模型

`ApplicationProduct` / `AiEmployeeProduct` 是商品或逻辑配置；`Deployment` 是某个节点上的实际部署，两者是不同实体。AI 员工包版本更新不代表模型服务已部署，容器镜像更新也不代表客户商业权益变化。

实例允许无客户、无许可证，用于内部 Redis、数据库和基础设施。客户本地部署只有在节点明确接入后才可观测；不根据许可证记录推断服务器已在线。

### 14.2 Compose 模板边界

R3 首批只支持维护者审核过的模板。模板定义版本、所需变量、秘密变量、允许端口/卷、最低资源、健康检查和升级说明。Registry 凭据和实际秘密只保留在节点或后续凭据服务中，前端只得到引用和掩码。

禁止普通模板使用 privileged、任意宿主目录、Docker socket、host PID、host network、设备任意透传；GPU 设备请求使用单独能力和经过验证的模板。禁止远程 include、任意 build context 和浏览器指定可执行路径。

主机监控服务 若调用 Docker Compose CLI，使用固定可执行路径、参数数组和专用目录，不构造可插值 shell 命令。工作目录由实例 ID 派生并规范化；拒绝路径穿越、符号链接逃逸和外部资源接管。

### 14.3 变更流程

1. 选择节点、模板版本、实例名、环境和可选客户/商品关联。
2. 校验资源、商业关联、模板能力和秘密引用。
3. render 后执行结构化校验，生成端口/卷/镜像/环境键名的脱敏差异及 configHash。
4. 预览有效期默认 10 分钟，绑定用户、目标与旧 revision；有变更或过期必须重新预览。
5. 用户提交 configHash 和 expectedRevision；重新认证后创建部署任务。
6. 节点拉取已确认镜像，保存实际 digest，执行 Compose，等待健康检查。
7. 写入 revision 和资源映射，关联任务/审计；失败保存步骤与恢复指引。

首次部署与更新的结果必须分开；没有 healthcheck 的容器只能记“进程运行”，不能宣称“业务健康”。共享卷、数据库迁移和外部依赖不能被通用镜像回退覆盖。

### 14.4 权益和交付

关联 customerId、licenseId 时验证主体一致，使用既有服务读取而不复制签发规则。许可证到期只显示状态并限制后续受授权的产品动作；不自动停容器、删卷或结束客户服务。

运维部署成功不自动标记订单已回款，不绕过“回款后签发”规则。若要自动更新交付状态，新增明确的交付事件流程，并接受原业务校验。

## 15. 部署、配置与升级

### 15.1 部署改造清单

1. Dockerfile 增加独立 web-runtime 和 api-runtime target；API builder 复制协议 crate。
2. 新建 `compose.ops.yaml`，包含 web、api、proxy；主机监控服务 运行宿主机 systemd。
3. web 不挂数据卷、密钥和 主机监控服务 socket，不继承 API 的许可证私钥环境。
4. api 非 root 运行，使用既有数据卷，增加只含通信 socket 的目录挂载与 group_add。
5. proxy 支持 TLS、WebSocket Upgrade、SSE 禁缓冲、合理超时和大小限制。
6. 原单容器部署在 R1 可作为只读过渡；开放 R2 写操作和终端之前完成分离验收。
7. 原 `CONTAINER_NAME-data` 卷名必须保留；切换服务名后不能因为 Compose 自动命名而产生空库。
8. 明确 SQLite 文件、Skill 包、头像、应用包、知识库的实际路径；先核对历史相对路径与挂载，必要时迁移到统一数据根目录，不能遗漏现有文件资产。
9. publish.ps1 增加新构建目标、配置验证和发布后冒烟；安装/升级 主机监控服务 使用独立显式步骤，普通业务发布不自动更换宿主特权组件。

### 15.2 生产反向代理规则

| 路径                        | 上游 / 策略                                                       |
| --------------------------- | ----------------------------------------------------------------- |
| /                           | web，仅页面和静态资源                                             |
| /operation-api/api/ops/*    | api，管理访问范围、Cookie、长连接规则                             |
| /operation-api/api/session* | api，登录限速、CSRF 和重新认证                                    |
| /operation-api/*            | api，保留现有管理台同源接口兼容                                   |
| 既有公开业务入口            | 保留原域名/端口或映射到 api；逐条列明市场、许可证、官网认证等路由 |

升级前从 `main.rs` 与 `website_auth::router` 生成路由清单，确认哪些调用来自客户平台、官网、服务商和管理台。不能简单对整个 `/api` 加同一 IP 白名单，也不能把整个管理 API 作为客户公开接口。

生产浏览器统一由代理访问；不依赖 Next.js rewrite 承担长连接。开发阶段前端可连接独立开发代理。若保留跨端口开发，API CORS 允许列表包含精确来源和所需 `PATCH`、`Idempotency-Key`、CSRF 头，不能使用通配凭据来源。

### 15.3 配置契约

| 配置                              | 默认                               | 所属 / 含义                                      |
| --------------------------------- | ---------------------------------- | ------------------------------------------------ |
| YAYA_OPS_ENABLED                  | false                              | API，总开关                                      |
| YAYA_OPS_WRITES_ENABLED           | false                              | API，资源变更开关                                |
| YAYA_OPS_HOST_TERMINAL_ENABLED    | false                              | API，主机终端开关                                |
| YAYA_HOST_MONITOR_SOCKET             | /run/yaya-host-monitor/monitor.sock           | API，本机 主机监控服务 地址                             |
| YAYA_OPS_ALLOWED_ORIGIN           | 无生产默认值                       | API，精确管理台 HTTPS Origin；缺失时拒绝生产终端 |
| YAYA_OPS_DATA_DIR                 | /var/lib/yaya-operation-center/ops | API，任务文件和可选录像                          |
| YAYA_SESSION_SECURE_COOKIE        | 生产 true                          | API，本地开发显式 false                          |
| YAYA_OPS_METRICS_INTERVAL_SECONDS | 5                                  | 主机监控服务，采样间隔，下限 2                          |
| YAYA_OPS_AUDIT_RETENTION_DAYS     | 180                                | API，审计保留                                    |
| YAYA_OPS_NODE_JOB_CONCURRENCY     | 4                                  | API，派发上限                                    |

主机监控服务 TOML 单独定义 socket_path、identity_path、execution_db_path、docker_socket、shell_user、protected_resources、日志和连接限制。配置文件 root 管理、不可由终端账户改写。启动时做字段验证，日志打印脱敏的配置摘要。

配置中的 UID/GID、目标 OS 用户、driver 路径和保护资源在安装时验证。不要直接把本文示例默认值当作已存在系统账户。

### 15.4 主机监控服务 安装包与服务

- 输出 Linux x86_64 二进制、SHA-256、版本清单、systemd unit 和安装脚本；ARM64 在有实际机器验证后再声明支持。
- 安装脚本检测 systemd、Docker、权限、目录、端口/Socket 冲突和已有版本；不自动安装/替换 GPU 驱动。
- systemd 使用必要范围的可写目录和权限；ProtectSystem、PrivateTmp、设备访问等沙箱项逐项实测，不复制会破坏 NVML/PTY 的示例配置。
- 更新使用新文件 + 原子切换 + 重启 + 健康验证；旧版本保留可回退。Socket 目录和节点身份不删除。
- 卸载默认保留身份与执行历史，不删除业务容器、数据卷或客户文件；彻底清理必须单独命令且列明路径。
- 发布来源和校验通过才安装，不通过未校验的 curl 管道执行在线脚本。

## 16. 分阶段任务清单

所有任务初始状态均为未开始。角色缩写：BE=控制平面后端，AG=主机监控服务/系统，FE=前端，QA=测试，OP=部署。角色可以由同一开发者承担，不表示已有对应团队。

### 16.1 R0：环境与技术验证

| ID     | 责任  | 具体任务 / 主要文件                                                                   | 完成条件                                            | 依赖   |
| ------ | ----- | ------------------------------------------------------------------------------------- | --------------------------------------------------- | ------ |
| ENV-01 | BE/OP | 记录现有提交/未提交基线、端口、数据目录、公开接口、备份方式到 environment-baseline.md | 无 secrets 入文档；能从副本恢复原平台               | 无     |
| ENV-02 | AG    | 建 host-monitor 与 ops-protocol 空骨架，验证 UDS hello/JSON/双向流和容器内挂载               | 重启 主机监控服务 后 API 可恢复；权限错误有明确结果        | ENV-01 |
| ENV-03 | AG    | 非生产 Docker 上验证列表、Inspect、日志、stats、Exec 和事件恢复                       | TTY/非 TTY、无 shell、不同日志驱动均有记录          | ENV-02 |
| ENV-04 | AG    | Linux 非特权 PTY 与容器 Exec 清理 PoC                                                 | 身份、resize、子进程、断线/超时/重启清理范围实测    | ENV-02 |
| ENV-05 | AG/QA | NVML 真机及无驱动环境验证；形成兼容矩阵和依赖锁定清单                                 | 真实卡型/驱动数据；缺硬件明确标 blocked，不伪造通过 | ENV-02 |

R0 出口：ENV-01 至 ENV-05 有证据；终端清理和身份边界无未说明缺口。某项验证失败则调整该能力支持范围和后续任务，不用 Mock 替代真实执行验收。

### 16.2 R1-A：公共基础

| ID     | 责任 | 具体任务 / 主要文件                                        | 完成条件                                             | 依赖           |
| ------ | ---- | ---------------------------------------------------------- | ---------------------------------------------------- | -------------- |
| FND-01 | BE   | 提取 AppState 需要的身份/DB 适配，新增 ops router 和错误码 | OPS_ENABLED=false 原业务启动与路由行为正常           | ENV-01         |
| FND-02 | BE   | 编号迁移器、0001–0003、备份恢复测试                        | 新库/旧库/重复启动/中断恢复通过                      | FND-01         |
| FND-03 | BE   | 员工持久会话、退出、CSRF、重新认证、auth_version           | 过期、撤销、禁用用户、角色变化测试通过               | FND-02         |
| FND-04 | BE   | action + node scope 授权和授权管理接口                     | 财务/运营/服务商无法越权，直接 API 同样拒绝          | FND-03         |
| FND-05 | BE   | 强制 ops_audit、敏感 DTO 脱敏、存储失败策略                | 管理台 Origin 不跳过；审计写失败不派发写操作         | FND-04         |
| FND-06 | BE   | jobs/outbox、资源锁、Dispatcher、恢复与 主机监控服务 接口         | 幂等/重复派发/响应丢失可对账，无盲重试               | FND-05, ENV-03 |
| FND-07 | AG   | 主机监控服务 配置、身份、hello、能力、执行日志与保护策略          | 非授权 Socket 用户被拒，未知 action 不执行           | ENV-03, ENV-04 |
| FND-08 | FE   | 运维工作区、OpsProvider、节点选择、权限门、错误状态        | 原导航缓存兼容；无权限无入口；直达页受保护           | FND-04         |
| FND-09 | OP   | 新 API/Web 镜像 target、Compose/proxy 骨架、UDS GID 接入   | Web 无 socket/私钥；原数据卷完整；HTTPS 流能通过代理 | ENV-02, FND-01 |

### 16.3 R1-B：只读面板

| ID     | 责任  | 具体任务 / 主要文件                         | 完成条件                                       | 依赖                           |
| ------ | ----- | ------------------------------------------- | ---------------------------------------------- | ------------------------------ |
| OBS-01 | AG/BE | 本机节点发现、心跳、能力变化、offline/stale | Docker 不可用时其他监控和运营仍正常            | FND-07, FND-04                 |
| OBS-02 | AG/BE | 容器缓存、列表/详情 DTO、Docker events 重连 | 删除/重建同名容器不混淆；敏感配置不泄露        | OBS-01                         |
| OBS-03 | AG/BE | 历史/跟随日志、限流、gap、SSE 权限复查      | 非 TTY 解复用正确，慢消费者不导致内存无界      | OBS-02, FND-05                 |
| OBS-04 | AG/BE | 主机和容器统计、增量基线、环形窗口          | 首样本/null、重启、计数器回退、cgroup 测试通过 | OBS-02                         |
| OBS-05 | AG/BE | NVML 能力检测、GPU 样本和进程权限           | 真机可读，无库/无卡/无权限状态区分             | ENV-05, OBS-01                 |
| OBS-06 | FE    | 总览、节点、容器、日志、GPU 页面            | stale、空态、断线、无能力均有可理解展示        | FND-08, OBS-03, OBS-04, OBS-05 |
| OBS-07 | FE    | 任务和运维审计查询页、SSE Hook              | 资源范围过滤，刷新任务不丢状态，旧订阅被取消   | FND-06, FND-08                 |
| OBS-08 | QA    | R1 真机只读冒烟、浏览器回归和长连接测试     | 24 小时采集无持续内存增长，既有运营回归通过    | OBS-06, OBS-07, FND-09         |

R1 出口：只读管理面板可在 Linux 上运行，能解释“无 GPU / 无驱动 / 节点离线”。所有写开关仍关闭，不将 R1 标为本轮全部完成。

### 16.4 R2：容器操作与终端

| ID     | 责任  | 具体任务 / 主要文件                                     | 完成条件                                         | 依赖                   |
| ------ | ----- | ------------------------------------------------------- | ------------------------------------------------ | ---------------------- |
| ACT-01 | AG/BE | start/stop/restart/remove 动作、保护清单、observed 接管 | 保护容器不可操作；无卷删除；完整 ID 和双侧校验   | OBS-08, FND-06         |
| ACT-02 | FE    | 动作确认、幂等提交、任务进度和结果刷新                  | 双击只建一任务；unknown 不提示成功；失败保留页面 | ACT-01                 |
| ACT-03 | BE    | 0004、terminal session/ticket、WSS 网关和授权轮询       | 一次票据、Origin、CSRF、到期/撤销、配额测试通过  | FND-03, FND-05, FND-09 |
| ACT-04 | AG    | 受支持容器 Exec、输入输出、resize、清理状态             | 断线/超时不虚报清理成功；支持镜像写入兼容矩阵    | ACT-03, ENV-04         |
| ACT-05 | AG    | 指定非 root 账户 PTY、环境白名单、进程组清理            | whoami 正确；读不到 secrets；身份切换失败不启动  | ACT-03, ENV-04         |
| ACT-06 | FE    | xterm、fit、会话标签、关闭、粘贴和错误提示              | 卸载无连接泄漏；多行/中文/ANSI/尺寸正确          | ACT-04, ACT-05         |
| ACT-07 | QA/BE | 权限绕过、票据重放、撤销、崩溃、慢消费者测试            | 无未解决的高权限绕过、未知执行有可查证据         | ACT-02, ACT-06         |
| ACT-08 | QA/OP | Linux 实机全链路与运营回归、发布文档                    | 部署隔离、备份恢复、终端生命周期均有报告         | ACT-07                 |

R2 出口：三项核心能力全部通过真实环境验收。GPU 缺硬件则 GPU 验收仍未完成；不能只根据 Mock 关闭任务。后续 R3/R4 可以独立排期。

### 16.5 R2 发布任务

| ID     | 责任  | 具体任务                                           | 完成条件                                               | 依赖           |
| ------ | ----- | -------------------------------------------------- | ------------------------------------------------------ | -------------- |
| REL-01 | OP    | 主机监控服务 安装/升级/卸载脚本和校验清单                 | 重复安装幂等，升级保留 identity，卸载不删业务数据      | ACT-08         |
| REL-02 | BE/OP | 数据库与文件备份/恢复演练、迁移报告                | 在隔离环境恢复客户、许可证、Skills、知识文件和运维记录 | ACT-08         |
| REL-03 | QA/OP | 自定义数据卷名、旧公开接口、反向代理兼容验收       | 升级后无空库、无客户认证/许可证回归                    | REL-01, REL-02 |
| REL-04 | QA    | 性能、配额、长时间运行和故障注入                   | 第 17 节门槛有环境与结果记录                           | REL-03         |
| REL-05 | OP    | release-checklist、runbook、版本制品、逐级开关发布 | 有停用/回退路径，非生产演练通过后才进入生产发布        | REL-04         |

### 16.6 R3：商业交付闭环

| ID     | 责任  | 具体任务                                    | 完成条件                               | 依赖   |
| ------ | ----- | ------------------------------------------- | -------------------------------------- | ------ |
| BIZ-01 | BE    | 0005、实例/资源/revision 模型与业务关联校验 | 可无客户部署；跨客户许可证关联拒绝     | REL-05 |
| BIZ-02 | AG/BE | 模板规范、Compose 校验、秘密引用、配置差异  | 危险配置拒绝；预览不执行；输出不泄密   | BIZ-01 |
| BIZ-03 | AG/BE | apply 任务、digest、健康检查和版本记录      | 配置过期/并发变更拒绝，失败可定位      | BIZ-02 |
| BIZ-04 | FE    | 实例页、预览确认、版本和任务详情            | 商品/员工版本与部署 revision 不混淆    | BIZ-03 |
| BIZ-05 | FE/BE | 客户与许可证详情增加只读实例关联            | 原回款/签发流程和状态不被绕过          | BIZ-04 |
| BIZ-06 | QA/OP | 部署失败、持久卷、升级与恢复验证            | 数据恢复限制可见，未承诺自动数据库回滚 | BIZ-05 |

### 16.7 R4：多节点与持续运营

| ID       | 责任  | 具体任务                                   | 完成条件                                   | 依赖     |
| -------- | ----- | ------------------------------------------ | ------------------------------------------ | -------- |
| SCALE-01 | BE/AG | 0006、注册令牌、节点专用 CA、证书轮换/撤销 | 不复用许可证 RSA 密钥，令牌一次性且有 TTL  | REL-05   |
| SCALE-02 | BE/AG | 主机监控服务 出站 mTLS、多路复用、版本协商        | NAT/断网/背压/撤销有效，旧 epoch 流清理    | SCALE-01 |
| SCALE-03 | BE/OP | 时序存储、DCGM 适配、remote_write          | 客户内网只需出站，指标端点不公网裸露       | SCALE-02 |
| SCALE-04 | BE/FE | 告警规则、持续时长、静默、恢复和通知       | 可去重、能恢复、记录通知结果，不写死收件人 | SCALE-03 |
| SCALE-05 | QA    | 多节点授权、长连接、凭据轮换、跨租户边界   | 串节点、伪节点、过期证书、规模压测通过     | SCALE-04 |

## 17. 测试与验收矩阵

### 17.1 测试分层

| 层次        | 覆盖内容                                                 | 环境                                     |
| ----------- | -------------------------------------------------------- | ---------------------------------------- |
| 单元 / 契约 | 权限组合、状态机、参数白名单、指标计算、协议序列化       | Windows 可运行纯逻辑；Linux 补齐系统模块 |
| SQLite 集成 | 迁移、约束、幂等、任务/outbox/audit 原子性、过期票据竞争 | 临时数据库，不使用开发真实库             |
| 主机监控服务 集成  | UDS 权限、Docker API、PTY、进程回收、NVML 降级           | Linux 非生产主机，专用测试容器           |
| 浏览器 E2E  | 页面/权限/导航/订阅/终端呈现/错误恢复                    | 延续 Playwright 8780，Mock 覆盖交互契约  |
| 真实端到端  | 浏览器→代理→API→UDS→主机监控服务→Docker/PTY/GPU                 | Linux + Docker + NVIDIA 真机             |
| 故障与负载  | 断线、重启、重复执行、存储满、慢消费者、长时间采样       | 隔离环境，禁止对生产容器注入故障         |

Mock 不证明宿主执行、权限切换或硬件采集正确。测试报告区分 passed / failed / blocked / not-run。

### 17.2 必测场景

| 编号    | 场景                                          | 通过标准                                  |
| ------- | --------------------------------------------- | ----------------------------------------- |
| AUTH-01 | 运营、财务、服务商直接调用写接口和 WS         | 无权限，无 主机监控服务 调用                     |
| AUTH-02 | 可见 node A 的用户猜 node B / 容器 ID / jobId | 不泄露 B 详情，也不可操作                 |
| AUTH-03 | 改角色、撤销授权、停用用户、退出              | REST 立即生效；流和终端 30 秒内关闭       |
| AUTH-04 | 伪 Origin、无 CSRF、旧票据、票据并发使用      | 握手/写请求拒绝，最多一个合法 attach      |
| DB-01   | 全新库、历史库、重复启动、迁移失败            | 数据不丢、迁移可解释、无半完成运维写入口  |
| JOB-01  | 同键同内容/不同内容、API 响应丢失             | 原 jobId / 409；不重复创建动作            |
| JOB-02  | 主机监控服务 接收后断线、Docker 后崩溃               | 查询或 unknown；不能盲重启/盲删除         |
| AUD-01  | 管理台请求、审计盘满、DB 锁                   | 有 intent/result；新高权限动作失败关闭    |
| CTR-01  | 保护容器、同名重建、资源已删除                | 不误操作新资源；保护始终有效              |
| LOG-01  | TTY/非 TTY、中文跨帧、超长行、driver 不支持   | 解析正确、缓冲有界、原因准确              |
| PTY-01  | whoami、HOME、环境和文件权限                  | 专用账户，无法读取平台秘密或改 主机监控服务 配置 |
| PTY-02  | 断网、浏览器关闭、超时、后台进程              | 宿主进程回收；容器清理结果真实可见        |
| GPU-01  | 无卡、无 NVML、驱动异常、权限不足             | 主机监控服务 和运营可用；状态准确、非假零        |
| GPU-02  | 真机、多卡、GPU UUID、字段不支持              | 单位/数值合理；不混卡、不伪造进程归属     |
| UI-01   | 1440/768/390、键盘、切节点、刷新和返回        | 不溢出、不串数据、不重复建连接            |
| OPS-01  | 主机监控服务 / API / Web / proxy 分别重启            | 运营可用、节点恢复、任务对账、旧终端关闭  |
| UPG-01  | 旧卷、旧公开端口/域名、升级和回退             | 原客户业务正常、文件资产完整              |

### 17.3 初始性能门槛

基准环境暂定 4 vCPU / 8 GiB Linux、20 个轻量容器、5 个浏览器监控客户端、1–2 张 NVIDIA GPU。ENV-01 记录实际机器后调整并冻结，测试不达到该环境时不得直接声称通过。

- 采样间隔 5 秒，UI 新样本展示延迟 p95 ≤ 10 秒。
- 缓存容器列表接口 p95 ≤ 500ms；快照/Inspect 慢调用给出 deadline，不阻塞订单 API。
- 主机监控服务 稳态目标 CPU ≤ 单核 5%、RSS ≤ 200 MiB；控制平面运维缓存增量目标 ≤ 200 MiB。不含图表浏览器内存、外置 Exporter 和镜像下载缓冲。
- 在健康、低延迟测试网络下，终端输入到显示 p95 目标 ≤ 200ms，并记录测量方法。
- 全配额订阅和 24 小时采集不出现持续无界内存增长；超配额返回明确错误。
- 运维采集开启后，原订单/许可证基准延迟增幅目标 ≤ 20%；先测基线再比较，不用绝对值猜测。
- 性能目标是验收基准，尚非已测结果；超标先定位优化或调整支持规模，并更新记录。

### 17.4 执行命令

以下为相应代码创建后使用的验证入口；当前尚未创建的目录和脚本不能视为已有能力。

```powershell
# 既有前端回归入口
pnpm --dir web run lint
pnpm --dir web run typecheck
pnpm --dir web run test:e2e
pnpm --dir web run build

# 后端与新增 主机监控服务；每个清单均需在仓库内存在
cargo test --manifest-path api/Cargo.toml
cargo test --manifest-path host-monitor/Cargo.toml
cargo clippy --manifest-path api/Cargo.toml --all-targets
cargo clippy --manifest-path host-monitor/Cargo.toml --all-targets

# 目标配置创建后进行 Compose 校验
docker compose -f deploy/compose.ops.yaml config --quiet
```

Linux 专用 tests 使用显式标签/独立脚本，不让“平台不支持所以跳过”被报告为真实能力通过。完整 Next standalone 镜像构建在 Linux / Docker 环境验证，Windows 符号链接限制不能作为删除 standalone 配置的理由。

## 18. 发布、回退与运行手册

### 18.1 R2 发布门槛

- [ ] 所有 R0/R1/R2/REL 任务完成，或非核心例外有明确范围、负责人和关闭条件。
- [ ] Linux Docker、非 root PTY、NVIDIA 真机验证有报告；三项核心能力无 blocked。
- [ ] 员工会话、CSRF、WS Origin、票据、节点权限和保护清单通过。
- [ ] 审计不可写、主机监控服务 掉线和 unknown 任务均有可操作说明。
- [ ] Web/API 容器分离，Web 无 Socket、私钥和运营数据卷。
- [ ] 数据库与所有文件资产已备份，并在隔离环境恢复成功。
- [ ] 许可证、官网认证、市场、订单、财务、服务商原功能回归通过。
- [ ] 24 小时稳定性、配额、性能及长连接代理验证通过。
- [ ] 主机监控服务、API、Web、协议版本、SHA-256、镜像 digest 和数据库迁移版本已归档。
- [ ] 发布开关、回退步骤、维护窗口和重新登录说明已写入 runbook。

### 18.2 发布顺序

1. 完成非生产环境的完整安装/升级演练并归档结果。
2. 保持所有运维开关关闭，确认原服务健康，备份 DB、文件和部署配置。
3. 部署兼容版本的 主机监控服务，但不开放远程网络监听。
4. 部署新 API/Web/proxy，执行迁移并核对旧卷、原业务和兼容入口。
5. 开总开关，仅授权管理员查看；确认节点、Docker 和 GPU 快照。
6. 开容器写开关，仅操作专用测试容器验证任务与审计。
7. 验证重新认证后开启容器终端，再单独开启主机终端。
8. 扩大用户授权；记录生效时间、版本和回退联系人。

本计划只定义执行步骤，不代表已授权本轮直接修改生产主机或发布服务。

### 18.3 回退分层

| 层面         | 操作与约束                                                           |
| ------------ | -------------------------------------------------------------------- |
| 功能回退     | 先关主机终端，再关写操作，最后关闭运维；拒绝排队任务并回收活跃会话   |
| 服务回退     | 保留证据后回滚 API/Web 镜像；先确认旧版本是否支持已迁移 schema       |
| 主机监控服务 回退   | 使用保留二进制，确认协议兼容，保留 identity 和执行日志               |
| 数据回退     | 停写并恢复一致性快照；恢复备份会丢失备份后的业务写入，须明确处理窗口 |
| 工作负载回退 | 平台回退不自动回滚已执行容器动作；按任务记录逐一处理                 |

新增字段通常可向后兼容，但 users 约束变更、会话结构和状态语义需专门验证。禁止运行破坏性 down migration 后假装无损回退；上线后有新商业数据时优先修复前进。

### 18.4 运维手册必须包含

主机监控服务 无法连接、Socket GID 错误、容器平台自我保护、Docker driver 不支持日志、PTY 账户错误、GPU 驱动缺失、数据 stale、任务 unknown、审计磁盘满、备份恢复、证书过期（R4）等故障的“现象 → 检查命令 → 处置 → 验证”。

日志使用结构化字段 requestId / jobId / executionId / nodeId，屏蔽 Cookie、票据、密码、环境秘密。平台健康拆分 live / ready / ops-degraded：主机监控服务 不在线不能让许可证 API 被编排器反复重启。

## 19. 工作量、依赖与风险

### 19.1 初步工作量

以下是规划用人日，覆盖前后端、系统执行、测试和部署，不是承诺排期；以 R0 完成后重新估算为准。

| 包                                        | 粗估人日 | 说明                                |
| ----------------------------------------- | -------- | ----------------------------------- |
| R0 环境 / 协议 / Docker / PTY / NVML 验证 | 4–7      | 硬件不到位会产生等待，不计为已完成  |
| R1 基础 + 只读面板                        | 18–28    | 含会话、授权、迁移、审计、任务和 UI |
| R2 操作 + 双终端                          | 20–32    | 进程清理、身份边界和恢复是主要变量  |
| R2 发布、恢复、稳定性                     | 5–8      | 不省略真实环境测试                  |
| R3 部署和业务闭环                         | 15–25    | 模板数量和数据库升级策略影响较大    |
| R4 多节点和历史告警                       | 20–35    | 证书、网络、租户规模影响较大        |

R0–R2 发布粗估合计 47–75 人日。实际日历时间取决于人力、经验、兼容范围和验证环境，不能简单除以人数得到承诺日期。任务表可以先落入 issue 系统，按依赖排迭代；不要求本轮创建外部任务。

### 19.2 关键路径

```text
环境验证
  → 迁移 / 会话 / 资源授权
  → 独立审计 / jobs / 主机监控服务
  → 只读真机闭环
  → 容器写操作 + 终端网关
  → PTY / Exec 生命周期
  → 权限与故障测试
  → 恢复演练 / 兼容检查 / 发布
```

协议冻结后 FE 可以用 Mock 开发，不能用 Mock 通过替代 AG/BE 的集成完成。主机监控服务 和前端可以作为角色并行推进，但同一资源/协议变更要先更新契约。

### 19.3 风险与处理

| 风险                        | 影响                         | 处理 / 停止条件                                                       |
| --------------------------- | ---------------------------- | --------------------------------------------------------------------- |
| 没有真实 Linux / GPU 环境   | PTY/NVML 无法验收            | 先补测试主机；相关任务保持 blocked，不标核心功能完成                  |
| Docker root 等价控制        | 主机被越权管理               | 专用 主机监控服务、最小接口、API/Web 隔离、权限/审计；存在绕过则不开放写能力 |
| 容器 Exec 无可靠清理        | 遗留后台进程                 | R0 验证并限定支持镜像；不可清理时呈现真实状态或禁用                   |
| 原业务与运维共享 SQLite     | 写锁和业务延迟               | 有界写频率、批量清理、任务文件分离、压测；超阈值再评估数据库拆分      |
| 相对文件路径遗漏            | Skills / 头像 / 知识文件丢失 | ENV-01 列资产，恢复演练验证，不只备份 DB                              |
| 旧 API 暴露方式改变         | 客户许可证/官网失败          | 明确路由清单、兼容入口、升级前后契约对比                              |
| 采样和终端流无界            | 内存/网络耗尽                | 消息、缓冲、频率、用户和节点配额全部落地                              |
| 后续把任意 Compose 当普通写 | 宿主逃逸和秘密泄露           | R3 限受控模板；高级能力另立安全与验收范围                             |
| 功能扩散到完整 1Panel       | 无法及时交付核心三项         | R2 范围冻结；网站/数据库等另外立项                                    |

### 19.4 开发期间变更约定

以短分支/小 PR 交付：一个 PR 对应一个可验收任务或紧密依赖的一组任务。每个 PR 包含实际行为、契约变更、迁移影响、验证证据和剩余限制。不为分支整洁回退仓库当前未提交的商业功能。

修改协议、权限、迁移或任务状态机时同步本文与具体契约文档。每个里程碑提交兼容矩阵和验收报告，不能把未执行的测试写为“通过”。

## 20. 首个迭代的执行清单

第一个迭代先交付一个真实只读闭环，不直接大规模制作页面：

1. 完成 ENV-01，确认非生产 Linux、Docker、GPU 型号/驱动和现有文件资产。
2. 建独立 `host-monitor`、`ops-protocol`，完成 UDS hello 与快照，验证 API 容器权限和 主机监控服务 重启。
3. 并行验证 Docker 日志/Exec、非 root PTY、NVML，形成支持范围和所选依赖版本记录。
4. 建 ops router、迁移器、持久会话和固定运维权限；先完成 `/api/ops/context`、`/nodes`、容器只读接口，不以临时鉴权绕过正式基础。
5. 前端使用统一分组导航，增加节点状态和容器只读列表，保留默认收起与缓存行为。
6. 在 Linux 上走通“浏览器 → 入口代理 → API → UDS → 主机监控服务 → Docker”，记录权限拒绝和 主机监控服务 离线结果。
7. 评审 R0 验证结果和实际工时，冻结 R1/R2 兼容范围；随后按第 16 节继续完成，不停在演示版。

首个迭代产物：环境基线、技术验证报告、协议初稿、依赖锁定、只读端到端截图/日志、未完成项与下一步任务。整个项目的完成标准仍然是 R2 的完整验收与可发布制品。

## 21. 2026-11-10：本机客户实例运行闭环

本轮范围已落地：持久运行状态快照、5 秒间隔后端核对、Docker 断连/容器缺失分类、容器客户与实例关联、白名单详情、实例直达日志、受控启停重启、任务互斥/审计与 API 重启中断恢复。仅操作当前 Docker 下的已绑定客户容器，第三方容器只读，不增加远程服务器选择；旧迁移保持原内容，新增第 4 号迁移。

验收使用既有离线测试实例完成日志→停止→状态同步→启动→重启→API 重启后核对，保留两个原数据卷。结果与限制见 [验收记录](./instance-runtime-e2e-2026-11-10.md)。后续仍待独立迭代：运行版本升级/回滚、备份恢复演练、终端隔离、GPU/宿主采集生产验收及 Linux 真实环境兼容验收。

## 2026-11-10：终端、日志增强与实例资源监控交付

平台实例及容器详情已接入真实 TTY、窗口缩放、连接/断开审计与空闲超时；支持实时日志、时间范围、stdout/stderr、搜索、下载和部署失败跳转。已绑定客户容器保存 CPU、内存、网络及容器磁盘指标趋势，提示异常退出/OOM/频繁重启。Windows Docker Desktop 已完成真实验收，宿主/GPU 按环境能力提供；数据卷占用、Linux 真机及 GPU 生产验证未纳入本次完成范围。

详情见 [开发与验收记录](./container-observability-e2e-2026-11-10.md)。API 连接终端期间重启后已验证会话中断和 Shell 清理；客户容器与两个数据卷保留。
