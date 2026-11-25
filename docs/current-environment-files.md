# 当前环境文件管理

入口：系统 → 文件（`/ops/files`），仅平台管理员可访问。

管理当前部署环境的文件。Linux 原生发布由独立的 `yaya-file-agent.service` 执行文件操作，API 通过 Unix Socket 调用；Windows 本地开发仍使用 API 进程权限。容器部署仍只能访问进程所在容器及挂载目录，不会因为本次改动自动获得宿主文件访问能力。文件页面不依赖资源监控接口或 Docker，不提供远程文件服务。

支持目录浏览、路径跳转、上级目录、当前目录名称搜索、每页 100 项、权限和 UID/GID、文件大小和修改时间、新建文件/目录、单文件上传、流式下载、UTF-8 文本预览、文件和空目录删除。

表格操作为打开、下载、更多三个图标入口；目录不提供直接下载。更多包含删除、重命名、复制路径、属性。重命名需要修订标识，禁止覆盖同名目标及重命名管理根目录/系统顶级目录。复制路径为当前环境的实际路径，属性按需读取最新元数据。

目录大小只在点击“计算”后扫描，统计普通文件的逻辑字节数；跳过符号链接和特殊文件，硬链接按每个目录项计数。最多扫描 100000 项、128 层、20 秒，权限不足或超过限制时明确失败，不显示不完整结果。结果为计算时刻的统计，支持重新计算；刷新或文件变更后清除页面中的统计缓存。

`YAYA_FILES_ROOT` 限定管理根目录。Linux 默认 `/`；Windows 开发环境默认 API 启动时的工作目录。页面路径相对管理根目录。Linux 原生发布的 API 使用普通账号，文件 Agent 以 root 和受限 capability 集运行，允许平台管理员管理范围内的系统文件；不会保存系统账号密码，不通过 SSH 本机或 Docker 执行文件操作。只读挂载、SELinux 等仍可拒绝操作。容器部署需显式挂载要管理的目录。

上传单文件上限 500 MiB，分块写入随机临时文件，完成后无覆盖发布；同名冲突不覆盖，失败或请求取消清理临时文件。API 异常崩溃可能留存 `.yaya-upload-*` 临时文件，可人工检查清理。仅 UTF-8 普通文本支持预览（256 KiB 上限），HTML 等作为转义文本显示。下载使用 attachment 和 nosniff，不直接托管任意 HTML 页面。

路径以 capability 目录解析，禁止 `..`、设备路径与 Windows ADS，符号链接及特殊文件仅展示，不能作为最终操作对象。管理根目录及 Linux 系统顶级目录禁止删除，其他目录仅支持空目录删除。删除前提交文件修订标识，文件变化时要求刷新再确认。没有提供递归删除、在线编辑覆盖、压缩解压、权限修改或任意 Shell。

变更要求管理员会话与合法 Origin，读取/下载/预览和变更沿用运维鉴权、审计；变更另外记录目标路径、操作人与请求/完成事件。内容和文件字节不写入审计记录。配置 Agent 后，API 转发前先写审计记录；Agent 不连接业务数据库，也不加载平台私钥，执行事件记录至 journal。Socket 的调用 UID、服务端 UID 和协议版本均验证。服务不可用返回 503，不静默回退。

## API

- GET `/api/ops/local/files?path=/&search=&page=0`：目录列表，最多扫描 20000 项。
- GET `/api/ops/local/files/preview?path=...`：UTF-8 文本预览。
- GET `/api/ops/local/files/download?path=...`：文件下载。
- GET `/api/ops/local/files/size?path=...`：按需递归计算目录大小。
- GET `/api/ops/local/files/properties?path=...`：读取属性及实际路径。
- POST `/api/ops/local/files/upload?path=...`：multipart 单文件上传。
- POST `/api/ops/local/files/mutate`：mkdir/create/delete/rename，删除和重命名携带 expectedRevision。

文件管理操作不会启动或停止容器，也不会改变客户实例的升级、备份和恢复流程。

## 本地验证（2026-11-11）

- 文件模块及接口测试 5 项通过，覆盖权限、Origin、路径越界、同名不覆盖、上传/下载、删除修订校验、重命名、目录大小、属性和审计。
- Playwright 浏览器测试 4 项通过，覆盖浏览/搜索/预览/下载、上传/新建/删除、权限提示、不依赖监控服务、局部切换目录、按需计算大小及更多菜单。
- TypeScript 和新增页面 ESLint 检查通过。
- 本地 API 已重新编译并启动，健康检查通过；使用管理员真实会话读取当前目录成功（Windows 开发环境根目录为 API 工作目录）。
- Windows 本机在 standalone 收集阶段因创建符号链接缺少权限而失败（EPERM）；随后在生产 Linux 服务器完成完整 Rust release 与 Next.js standalone 构建，并成功发布。

## 生产发布（2026-11-11）

- 使用 `deploy/publish.ps1` 发布，版本目录为 `/opt/yaya-operation-center-service/releases/0.1.5-20261111092944-cd2f043e`。
- 升级前备份：`/opt/yaya-operation-center-service/backups/0.1.5-20261111092944-cd2f043e`。
- SSH 只读检查确认 current 指向该版本，统一目标、API、Web、监控服务均 active，已安装 `YAYA_FILES_ROOT="/"`。
- 公网 `/ops/files` 返回 200，Web 代理 API 健康检查成功，未登录文件接口返回 401。
- 线上临时文件读写测试命令被自动执行审核拦截，未执行。此前提供的本地管理员凭据在线上返回账号或密码错误，未重置账号，故本次未验证线上登录后的文件操作；本地接口及浏览器验证结果见上文。

## 线上目录大小排查（2026-11-11）

随后使用服务器已安装的管理员凭据建立临时会话，在真实线上页面以 Playwright 点击计算，凭据与会话未输出至日志。

- `/media`、`/srv`：接口为 0 字节，页面为 0 B；另以服务器文件系统直接枚举确认均为空目录。
- `/etc/profile.d`：接口返回 10877 字节、16 个文件，页面为 10.6 KiB；重新计算结果一致。服务器直接统计文件长度之和也为 10877 字节。
- `/mnt`：接口明确返回访问权限不足，页面显示错误，不渲染为 0 B。
- 发布目录 `deploy/host/systemd`：接口返回 1946 字节、4 个文件。

当前统计为普通文件内容的逻辑字节数，不含目录自身的元数据，不等于 `du` 的磁盘块占用。该轮线上测试未复现“所有目录均为 0 B”，也未修改业务文件。

## 全局通知接入修正（已于 2026-11-12 发布）

文件页读取错误通过既有 `useErrorNotification` 通知并去重；主动操作失败通过全局 `toast.danger` 提示，成功沿用全局 `toast.success`。移除页面和模态框中的重复错误段落，表单失败时保留输入。下载先请求同一接口的 `check=true` 分支验证文件存在及可读，再交由浏览器流式下载，避免将整个文件缓存至 JavaScript 内存；预检查后的并发文件删除或下载中断仍由浏览器处理。

当时服务使用独立账号 `yaya-operation-center`，目录大小递归统计遇到无法读取的目录会整体失败，不显示不完整结果。后续 Agent 改造见下节；API 本身仍不提权。

验证：文件接口测试 5 项、浏览器测试 5 项、TypeScript 和 ESLint 通过，含读取错误去重、目录大小失败、表单失败保留输入、下载预检查错误进入全局通知。

## 本机文件 Agent（已于 2026-11-12 发布）

- 独立进程模式：`yaya-operation-center-api --file-agent`。复用文件操作代码与构建产物，启动 Agent 不初始化业务数据库、签名密钥或 Web API，也不启动 Docker、终端与监控任务。
- IPC：`YAYA_FILE_AGENT_SOCKET=/run/yaya-file-agent/files.sock`；`YAYA_FILE_AGENT_ALLOWED_UID` 固定为 API 服务账号的 UID，发布器自动写入。目录 root 所有，Socket 模式 0660；只允许该 UID 连接，root 的其他客户端也会拒绝。API 校验 Agent peer UID 为 0。
- 请求只接受既定路由，内部身份由 API 从已认证会话生成；浏览器提交的身份头、Cookie 和 Authorization 不转发。没有任意 Shell、远程主机地址、系统密码或自定义执行命令。
- 文件服务有独立最小环境文件 `shared/file-agent.env`。systemd 使用 root 用户和文件访问所需的 capability 集，不授予网络管理、挂载、ptrace 或进程控制能力；不监听 TCP。
- 上传、下载双向流式传输，不在 API 聚合完整文件；请求参数 32 KiB，上传 500 MiB，最多 8 个并发操作；请求处理 15 分钟超时。目录大小扫描队列保存路径而非已打开目录，避免宽目录树耗尽句柄。
- 管理根目录仍经 capability 路径解析，不允许路径越界与最终符号链接操作。系统顶级目录删除/重命名保护保留，非空目录不递归删除。
- 发布器包含单元安装、健康检查、统一停止/启动、配置保留与冷备份。恢复到引入 Agent 前的版本时移除新单元并恢复旧的配置与数据。
- Windows 的 Named Pipe 特权服务、polkit 动态授权、多主机管理不在本次实现中；Windows 使用已有进程账号访问方式。

验证：Windows 全量 Rust 测试 100 项通过；Linux 文件测试 6 项及配置/服务依赖测试 9 项通过。使用服务器独立临时目录、随机端口、独立数据库和测试签名密钥完成非 root API + root Agent 的真实调用，验证受保护文件访问、Unix peer 身份拒绝、路径限制、上传下载哈希、重命名/属性/删除、审计和 Agent 停机失败关闭。未替换 current 或重启生产服务。

## Agent 生产发布与验收（2026-11-12）

- 使用 `deploy/publish.ps1` 完成发布，当前版本目录为 `/opt/yaya-operation-center-service/releases/0.1.5-20261112025802-ae3742c0`；冷备份为 `/opt/yaya-operation-center-service/backups/0.1.5-20261112025802-ae3742c0`。
- 修正安装器预检查：首次引入文件 Agent 时不要求旧版本已经安装该单元，原有服务定义仍须存在并匹配安装目录。
- Rust release、Next.js 生产构建及 TypeScript 检查成功。最新二进制的独立 Linux IPC 测试通过（包含非管理员拒绝），配置测试 9 项通过。
- target、API、Web、主机监控和文件 Agent 均 active。API 仍使用 `yaya-operation-center` 账号；文件 Agent 使用 root，Socket 为 `0660 root:yaya-operation-center`。
- 公网 `/ops/files` 返回 200，Web 代理健康检查成功。
- 使用已安装管理员凭据，经真实 Web 代理调用文件接口，`/root` 和此前拒绝访问的 `/mnt` 均可正常列出；未输出凭据或目录内容。
- 在 `/tmp` 创建独立随机测试目录，上传并下载 28672 字节文件，哈希与目录大小一致；重命名、删除通过，测试文件和目录已全部清理，未改动业务文件。
