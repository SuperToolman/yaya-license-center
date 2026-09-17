# Yaya Operation Center

Yaya Operation Center 是一个独立部署的运营管理平台。它提供受固定角色账号保护的客户、订单、财务、AI 员工商品和许可证管理能力，使用 RSA（RS256）签发许可证，并将运营数据存储在 SQLite 中。

当前版本：**0.1.5**

## 功能

- 固定角色账号登录和运营管理台（平台超级管理员、运营、财务、服务商管理员、服务商业务员）
- 客户档案、启用与停用管理
- 订单创建、未回款订单编辑、SaaS/本地部署类型、部分回款、取消和交付状态管理
- 财务经营汇总和收款流水
- AI 员工商品目录、价格、计费周期和上下架管理
- AI 员工人格、系统提示词和封装 Skills 管理
- AI 员工、人格与 Skills 使用系统自动版本；聚合包版本用于客户平台检测和同步更新
- Skills 允许工具分组、风险与确认策略配置，以及 ZIP 导入
- 仅允许从已回款订单签发许可证，签发后自动完成交付
- 为平台及可选模块设置独立有效期
- 为每个 AI 员工设置独立有效期
- 使用 RS256 签发可离线校验的 JWT 许可证
- 激活、状态检查和永久销毁许可证
- 按稳定客户主体返回最新许可证，支持客户平台发现新增模块和 AI 员工授权并提示管理员确认更新
- 运营请求审计日志，支持按级别筛选并查看 IP、接口、状态码和耗时
- 应用上线申请、运营审核和受许可证保护的已通过应用目录
- 服务商管理、订单归属、提成规则、返利台账和结算批次
- AI 员工头像裁剪上传，并在运营端和市场目录展示
- SQLite 持久化，并可自动迁移旧版 JSON 记录
- Docker Compose 部署，数据卷和密钥与应用代码分离

## 技术栈

- API：Rust、Axum、SQLite（rusqlite）、jsonwebtoken
- 管理台：Next.js、React、TypeScript、HeroUI、Tailwind CSS 4
- 部署：Docker、Docker Compose

## 项目结构

```text
api/        Rust API 服务
web/        Next.js 运营管理台
deploy/     Docker Compose、镜像构建和远程发布脚本
secrets/    本地开发或首次部署所需密钥（不会提交）
```

## 前置条件

- Rust stable 与 Cargo
- Node.js 22+ 与 pnpm
- Docker 与 Docker Compose（用于容器部署）

## 配置密钥

在项目根目录创建 `secrets/`，并准备以下文件。此目录已被 Git 忽略，绝不能提交私钥或管理员令牌。

```text
secrets/
  private.pem       RSA 私钥，用于签发许可证
  public.pem        RSA 公钥，用于校验许可证
  admin-password.txt 平台超级管理员初始密码
```

可使用 OpenSSL 生成一组开发密钥：

```powershell
New-Item -ItemType Directory -Force secrets | Out-Null
openssl genrsa -out secrets\private.pem 2048
openssl rsa -in secrets\private.pem -pubout -out secrets\public.pem
$initialPassword = Read-Host '设置平台超级管理员初始密码' -AsSecureString
$initialPasswordBstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($initialPassword)
[IO.File]::WriteAllText('secrets\admin-password.txt', [Runtime.InteropServices.Marshal]::PtrToStringBSTR($initialPasswordBstr))
[Runtime.InteropServices.Marshal]::ZeroFreeBSTR($initialPasswordBstr)
```

## 本地运行

安装前端依赖：

```powershell
pnpm install
```

从项目根目录启动 API 和管理台：

```powershell
.\start-dev.ps1
```

管理台默认地址为 `http://127.0.0.1:8778`，API 默认地址为 `http://127.0.0.1:8779`。

也可以分别启动：

```powershell
cd api
.\start-dev.ps1

cd ..\web
pnpm dev
```

API 支持的环境变量：

| 变量 | 说明 | 默认值 |
| --- | --- | --- |
| `LICENSE_SIGNING_PRIVATE_KEY_PEM` | RSA 私钥 PEM 内容 | 必填 |
| `LICENSE_SIGNING_PUBLIC_KEY_PEM` | RSA 公钥 PEM 内容 | 必填 |
| `YAYA_OPERATION_CENTER_INITIAL_ADMIN_USERNAME` | 首次初始化的平台超级管理员账号 | `admin` |
| `YAYA_OPERATION_CENTER_INITIAL_ADMIN_PASSWORD` | 首次初始化的平台超级管理员密码 | 必填 |
| `YAYA_OPERATION_CENTER_RESET_ADMIN_PASSWORD` | 一次性重置既有 `admin` 账号密码；重置后应立即清除 | 可选 |
| `YAYA_OPERATION_CENTER_HOST` | API 监听地址 | `127.0.0.1` |
| `YAYA_OPERATION_CENTER_PORT` | API 监听端口 | `8779` |
| `YAYA_OPERATION_CENTER_DB_PATH` | SQLite 数据库路径 | `api/.yaya-operation-center.sqlite3` |
| `YAYA_OPERATION_CENTER_WEB_ORIGIN` | 运营管理台公开来源，用于过滤管理台自身 API 请求 | 本地自动识别 `localhost:8778` |

升级期间仍兼容原来的 `LICENSE_CENTER_*` 环境变量。

## API 概览

除公开市场目录和受许可证保护的测试购买接口外，管理接口需要登录后的会话 Cookie。固定角色由服务端校验，不支持在后台自定义角色或权限。

AI 员工市场测试购买接口使用平台当前许可证作为 Bearer Token。运营端从签名主体识别客户，并自动完成建单、测试回款和累计权益许可证签发。

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `GET` | `/healthz` | 服务健康检查 |
| `POST` | `/api/session` | 使用账号密码创建会话 |
| `GET` | `/api/session` | 查询当前会话状态 |
| `GET / POST` | `/api/users` | 平台超级管理员列出或创建固定角色账号 |
| `POST` | `/api/licenses` | 按 `orderId` 从已回款订单签发许可证 |
| `GET` | `/api/licenses` | 列出许可证 |
| `GET / POST` | `/api/customers` | 列出或创建客户 |
| `POST` | `/api/customers/{customer_id}` | 更新客户资料 |
| `POST` | `/api/customers/{customer_id}/status` | 启用或停用客户 |
| `GET / POST` | `/api/orders` | 列出或创建订单 |
| `PUT` | `/api/orders/{order_id}` | 编辑未回款订单及其部署类型、商品和预计回款日期 |
| `POST` | `/api/orders/{order_id}/payment` | 登记订单回款 |
| `POST` | `/api/orders/{order_id}/cancel` | 取消未回款订单 |
| `GET` | `/api/finance/summary` | 查询财务经营汇总 |
| `GET` | `/api/transactions` | 查询收款流水 |
| `GET / POST` | `/api/ai-employees` | 列出或保存 AI 员工商品 |
| `POST` | `/api/ai-employees/{employee_id}/status` | 上架或下架 AI 员工商品 |
| `GET` | `/api/market/ai-employees` | 公开列出已上架的 AI 员工商品（不包含内部封装配置） |
| `GET` | `/api/market/ai-employees/{employee_id}/package` | 使用当前平台许可证读取已购买 AI 员工的最新完整安装包 |
| `POST` | `/api/market/ai-employees/{employee_id}/test-purchase` | 使用当前平台许可证主体完成调试购买、测试回款和自动签发（验收接口） |
| `PUT` | `/api/ai-employees/{employee_id}/avatar` | 上传 AI 员工头像 |
| `GET / POST` | `/api/application-submissions` | 管理应用上线申请；客户平台提交时使用许可证认证 |
| `POST` | `/api/application-submissions/{submission_id}/review` | 审核应用上线申请 |
| `GET` | `/api/market/applications` | 使用平台许可证查询已通过的应用目录 |
| `GET / POST` | `/api/providers` | 列出或创建服务商 |
| `GET / POST` | `/api/commission-rules` | 查询或配置提成规则 |
| `GET` | `/api/commissions` | 查询返利台账（服务商仅见所属数据） |
| `POST` | `/api/settlement-batches` | 创建服务商结算批次 |
| `GET` | `/api/logs?limit=500` | 查询最近的运营请求审计日志（需要管理员认证） |
| `POST` | `/api/licenses/{license_id}/activate` | 激活许可证 |
| `GET` | `/api/licenses/{license_id}/status` | 校验许可证状态，并在存在同主体更新时返回最新许可证及版本元数据 |
| `POST` | `/api/licenses/{license_id}/destroy` | 永久销毁许可证 |

## Docker 部署

首次部署会上传本地 `secrets/` 目录中的密钥和管理员令牌：

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-operation-center -Initialize
```

后续发布会保留服务器上的密钥与 SQLite 数据卷：

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-operation-center
```

默认将管理台暴露在 `8778`，API 暴露在 `8779`。可通过 `-WebPort` 和 `-ApiPort` 调整。低代码平台应配置 API 地址，例如 `http://example.com:8779`，而不是管理台地址。

更多部署约定请见 [deploy/README.md](deploy/README.md)。

## 数据与安全

- 许可证数据默认保存为 SQLite 文件；容器部署时保存到独立 Docker 数据卷。
- 若旧版 `.license-center-licenses.json` 和 `.license-center-revocations.json` 存在，服务会在首次启动时导入它们。
- `secrets/`、数据库、日志及运行时构建文件均已加入 `.gitignore`；AI 员工头像和 Skill 包应由持久化运行时目录管理。
- 生产环境请使用 HTTPS，安全保管私钥与管理员令牌，并定期备份 SQLite 数据卷。
- 审计日志不保存请求体、Authorization 或 Cookie；JSON 返回结果最多保存 32 KB，并自动脱敏许可证正文、Token、密码、API Key 和密钥字段。日志默认最多保留 90 天和 50,000 条记录。
- 日志中的 IP 用于安全审计和异常排查。若部署在反向代理后，应在代理层做好访问控制和 TLS 终止，不要直接把管理 API 暴露到公网。

## 许可证

尚未声明开源许可证。使用、复制或分发前请先与项目维护者确认授权条款。
