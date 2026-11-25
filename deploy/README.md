# 宿主机发布

运营/运维平台直接安装在目标 Linux 宿主机。API、管理台、主机监控和本机文件服务统一安装、统一升级，由 `yaya-operation-center.target` 托管。平台直接访问本机 Docker Engine、采集宿主 CPU/内存/GPU；客户低代码平台继续运行在 Docker 中。

`publish.ps1` 不再构建或启动运营平台容器，不再加载 Compose、挂载 Docker Socket，也没有 `-ContainerName`、`-DisableDockerManagement` 参数。旧 Dockerfile、Compose、entrypoint 和 `.env.example` 只保留作历史部署参考。客户实例的容器管理、离线镜像、终端及备份能力保留。

## 目标系统前提

- Linux、运行中的 systemd、本机 Linux Docker Engine（`/var/run/docker.sock`）。暂不支持 rootless Docker、非默认 Socket 或无 systemd 的系统。
- Rust stable（支持 edition 2024）、C 编译器、Node.js 22+、与锁文件兼容的 pnpm、Python 3.10+、curl、tar、iproute2、util-linux、账户管理工具。构建工具必须在远程非交互安装进程的 PATH 中。
- Node 和 Docker 可执行文件必须能被系统服务账户访问，不应只装在 root 的私有 home 目录中。Docker Socket 应属于可供服务账户使用的非 root 专用组。
- 本地 Windows 的 OpenSSH、scp、tar。远端登录账户为 root，或具备 `sudo -n` 权限。安装器不自动替换系统 Node/Rust，也不安装或重启 Docker。
- 首次及后续发布会在目标宿主机原生编译 Rust 和 Next.js，需要依赖下载通路、足够内存和磁盘；未使用运营平台镜像或 Docker 构建环境。

### 已有 CentOS 7 服务器

已在本次目标服务器验证：使用 `host/bootstrap-centos7.sh` 独立安装 Node 22.22.0 的 glibc 2.17 兼容构建、Python 3.11.11、pnpm 11.1.2 和 Rust。运行环境位于 `/opt/yaya-native-runtime`，不替换系统 glibc、`/usr/bin/python` 或其他业务运行环境。Node 使用 Node unofficial-builds 制品，校验固定 SHA-256；这不代表 CentOS 7 成为官方支持环境。Next.js 16 要求 Node >=20.9，不能简单降到 Node 16/18。

该引导脚本是显式的一次性准备步骤，要求现有 C 编译工具及 zlib 开发头文件。首次迁移前通过 SSH 上传并运行；发布器检测到 `/opt/yaya-native-runtime/environment.sh` 时自动加载环境。Python 仅用于配置、检查与安装，不依赖系统 Python 2。环境镜像下载源及版本固定在脚本内。

## 客户实例桌面端安装包

运营服务将实例的「访问地址」和当前版本写入 Windows x64 安装包。客户安装后可从桌面快捷方式直接打开该实例。实例升级后，只要访问地址相同，已安装的客户端就会打开升级后的页面。

每次随运营平台发布桌面启动程序时，脚本都会查询 NSIS 官网的最新稳定版。现有编译器版本满足要求时直接复用。版本落后或缺少编译器时，通过 Debian 软件源安装独立编译器到本次发布目录，并验证其版本。客户下载时由运营服务直接运行该编译器。

官网或软件源暂不可用时，发布日志会说明原因。有可用的 NSIS 3 时继续复用，首次安装缺少可用编译器时停止发布。Windows 本地开发时可使用 Tauri 缓存中的 NSIS，也可以用 `YAYA_DESKTOP_MAKENSIS` 指定编译器的完整路径。

`publish.ps1` 会从同级的 `yaya-low-code` 仓库构建 Windows 启动程序并随发布上传。也可以通过 `-DesktopLauncherPath` 提供已构建的 `yaya-desktop.exe`。本地开发使用 `api/start-dev.ps1` 时，会读取同级低代码仓库下的 `web/src-tauri/target/release/yaya-desktop.exe`，也可设置 `YAYA_DESKTOP_LAUNCHER`。

客户电脑需要 Microsoft Edge WebView2 Runtime，并能访问实例配置的地址。实例部署在服务器上时，配置客户电脑可访问的域名或 IP 地址。

## 首次初始化发布

本地 `secrets/` 准备 `private.pem`、`public.pem`、`admin-password.txt`。兼容旧 `admin-token.txt` 作为首次管理员密码来源。密钥不进入源码归档。

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -Initialize
```

默认使用 OpenSSH 的密钥或交互认证。需要密码认证时，从终端安全读取，不把密码写入脚本：

```powershell
$sshPassword = Read-Host 'SSH password' -AsSecureString
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -SshPassword $sshPassword -Initialize
```

指定私钥可以使用 `-SshKeyPath C:\path\to\private-key`。

默认目录 `/opt/yaya-operation-center-service`，Web 8778、API 8779。可以设置 `-RemoteDir`、`-WebPort`、`-ApiPort`；Web 和 API 端口不能相同。域名部署使用 `-WebOrigin https://operation.example.com`；客户容器访问授权 API 的地址使用 `-LicenseApiUrl https://operation-api.example.com`。未配置时首次根据服务器 IP 和端口生成，升级时保留已安装的公开地址；调整公开端口时应同时显式调整这两个地址。客户容器不得使用 `127.0.0.1` 访问宿主 API。

API 和 Web 默认监听 `0.0.0.0`。安装器不修改防火墙、云安全组或 TLS；通过反向代理开放访问时，需要转发 WebSocket Upgrade 才能使用终端和实时日志。

## 第 2 次及之后：覆盖升级

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root
```

后续发布属于宿主机覆盖升级：替换已安装程序、重写平台维护的路径/端口与 systemd 服务定义，保留业务数据、许可证密钥、初始管理员密码、短信配置及主机身份。不再次传 `-Initialize`，不重新创建业务账户。这里的覆盖升级不等于删除持久数据或客户 Docker 容器。

安装流程：

1. 检查依赖、服务归属和 Docker 权限，对同一安装目录加发布锁。
2. 在旧服务仍运行时原生构建新 API、管理台和主机监控程序；构建失败不会切换旧服务。
3. 停止整个平台，确认所选端口没有其他进程占用；不会杀死占用端口的第三方程序。
4. 备份整个 `shared/` 和旧服务定义。平台写进程已停止，SQLite/WAL、附件、离线包、加密密钥与监控身份一起保存。
5. 安装新服务定义，切换 `current`，启动统一 systemd 目标。发布程序会检查 API、管理台、Web→API 代理、服务账户的 Docker 连通性与真实主机内存指标。

有写入暂停时间。升级期间客户 Docker 容器继续运行；平台自己的备份不包含客户容器数据卷，客户恢复点继续由既有业务功能管理。部署健康检查是基础检查，不能代替业务升级和 GPU 实机验收。

## 目录与服务

```text
/opt/yaya-operation-center-service/
  current -> releases/<API版本-UTC构建时间-随机编号>
  releases/                      原生二进制、Next.js standalone 和发布脚本
  shared/
    operation-center.env         systemd 环境配置（root 0600）
    secrets/                     许可证密钥及初始管理员密码
    data/                        SQLite、runtime/skills、头像、离线包及加密密钥
    monitor/                     主机稳定节点身份
  backups/<本次发布编号>/         升级前 shared 快照、旧版本路径和旧服务定义
  build-cache/                   Rust 构建缓存，不是业务数据
```

构建源代码暂存于 `/tmp/yaya-operation-center-<发布编号>/source`；成功后清理上传的密钥副本和源码压缩包，构建目录、缓存、旧发布及备份暂不自动清理，便于失败诊断。不要把唯一业务文件写到 releases、build-cache 或 `/tmp`。

平台以 `yaya-operation-center` 系统账户运行，安装器将其加入 Docker Socket 所属组。API 和监控使用同一 UID，自动配置本机 Socket 权限与 `YAYA_HOST_MONITOR_ALLOWED_UID`，并启用 `YAYA_OPS_ENABLED=true`；不是额外部署一套产品。Docker 组仍具有高权限，不能将其理解为低权限隔离。

```sh
systemctl status yaya-operation-center.target
systemctl restart yaya-operation-center.target
systemctl stop yaya-operation-center.target
journalctl -u yaya-operation-center-api -u yaya-operation-center-web -u yaya-host-monitor -u yaya-file-agent -n 100
```

可选配置参考 `host/operation-center.env.example`，实际编辑 `shared/operation-center.env`，然后重启统一目标。`shared/data` 同时是 API 的持久工作目录，兼容现有代码中的相对 `runtime/skills` 路径。

文件管理使用可选环境变量 `YAYA_FILES_ROOT`。发布器从 `host/operation-center.env.example` 合并默认值（Linux 为 `/`），再读取服务器已有的 `shared/operation-center.env`；已有自定义值在后续发布时保留。systemd 通过 `EnvironmentFile` 将配置传入 API，无须增加发布命令参数。若需限定目录，在服务器的配置文件中设置，例如：

```ini
YAYA_FILES_ROOT="/srv/customer-files"
```

该目录必须已存在。发布器会生成单独的 `shared/file-agent.env`（root 0600），只包含根目录、Socket、允许的 API UID 和管理台来源；文件服务不会加载 API 的签名密钥、数据库和短信凭据。直接调整根目录时需同步修改两个环境文件，下次发布以 `shared/operation-center.env` 为准重新生成。修改后执行 `systemctl restart yaya-file-agent.service yaya-operation-center-api.service`。

`yaya-file-agent.service` 以 root 身份运行固定文件接口，API/Web 继续以普通 `yaya-operation-center` 账号运行；并未把整个平台改成 root。Socket 位于 `/run/yaya-file-agent/files.sock`，父目录 root 所有且不可由 API 账号写入；文件服务用内核 Unix peer credentials 校验调用 UID，API 同时验证服务端为 root。文件服务不监听 TCP，不提供 Shell 或任意命令，上传下载通过 IPC 流式传输。并发最多 8 个操作，完整请求处理上限 15 分钟，目录计算另有 20 秒/100000 项限制。

平台管理员经鉴权后可在配置根目录范围内操作系统文件，范围可以包含敏感系统配置；`YAYA_FILES_ROOT` 是文件服务的管理边界，不改变 API 安装目录、持久数据目录或客户数据卷。SELinux、只读挂载和文件系统自身限制仍可能拒绝操作。没有接入 polkit，也不保存 root 密码。文件执行事件进入 Agent 的 journal，API 另将调用人、操作及目标写入运维审计数据库；若请求前审计不可用，则拒绝执行。

本机 Windows 开发保留进程账号访问方式，当前没有实现 Windows Named Pipe 特权服务。Linux 若配置了文件 Socket 但服务不可用，返回明确错误，不自动回退到 API 文件权限。已有文件页面和路由保持一致，错误通过全局通知显示。

发布前验证：`python3 -m unittest discover -s deploy/host/tests -v`；`deploy/host/tests/file_agent_e2e.py` 可在 Linux root 下对指定构建产物运行隔离验证，不安装 systemd 单元、不修改生产数据库。

客户数据卷备份需要小型工具镜像。安装时优先复用本地 `docker:27-cli`，否则复用/下载 `alpine:3.21`，以不可变镜像 ID 写入配置；已配置自定义工具镜像时只校验其已存在。这里只用于操作客户数据卷，不是把运营平台放入容器，运行时备份任务不会联网拉取镜像。

## 失败与恢复

新程序启动可能迁移数据库，不能只把 `current` 改回旧程序。切换后的发布失败会停止平台并保留数据，不擅自把数据库切回旧快照。安装器会输出对应恢复命令，例如：

```sh
bash /opt/yaya-operation-center-service/releases/<新发布编号>/deploy/host/restore.sh \
  /opt/yaya-operation-center-service \
  /opt/yaya-operation-center-service/backups/<新发布编号>
```

恢复脚本停服后一起恢复旧程序、旧服务配置及升级前持久数据，再检查基础健康状态。较新的数据保留在 `failed-shared-<时间>` 中，便于核对；恢复将平台记录回退到备份时刻，不回退客户 Docker 容器的实际状态。首次安装失败没有旧原生版本可以恢复，需要检查失败原因后重新初始化或人工恢复；不会假装回滚成功。

## 从旧容器部署迁移

本发布器的正常升级对象是已安装的宿主机版本。第一次从旧容器迁移时，显式指定容器名，不传 `-Initialize`：

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -MigrateContainer yaya-license-center
```

安装器校验目标确为 `operation-center` Compose 服务，读取旧密钥及允许保留的短信/公开地址配置，构建成功后停旧容器、复制完整旧数据到 `backups/legacy-<发布编号>`，迁移到 `shared/data`，兼容旧数据库文件名和 Skill 工作目录。原生健康检查通过后移除旧平台容器，原数据卷、镜像和冷备份保留。未显式指定时检测到旧容器会拒绝空库上线。

旧容器迁移失败时不会删除旧容器；原生启动后发生失败会停原生服务并保留诊断，需要核对迁移数据库后恢复旧容器或修复原生服务。旧卷没有被新程序迁移修改，可用于人工恢复。旧镜像内的临时文件、非标准挂载及自定义路径仍须迁移前核对。

## 当前验收状态

本地已通过 9 项发布配置测试（含 WSL Linux 的 systemd 单元依赖验证）、20 项相关 Rust 测试、PowerShell/Shell 及生成的远程引导脚本语法检查、自定义 API 端口转发检查。主机监控服务的 Linux 目标代码编译检查通过；这些不等于已运行服务器服务或完成 GPU 实机验收。

2026-11-11 已在目标 CentOS 7 服务器完成旧容器迁移和一次正常 `publish.ps1` 覆盖升级。旧平台容器移除，8 个其他业务容器保持运行，平台四个 systemd 单元 active 且统一目标 enabled。已验证管理员登录、客户/订单/许可证接口、Docker 列表/资源、真实主机指标、主机身份保留和数据库完整性。备份恢复实际回退、全新空机初始化、Docker 写操作/终端浏览器流程和 GPU 实机不属于本次通过范围，详情见 [验收记录](../docs/host-deployment-e2e-2026-11-11.md)。
