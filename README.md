# Yaya License Center

Yaya License Center 是一个独立部署的许可证签发与管理服务。它提供受管理员令牌保护的管理台和 API，使用 RSA（RS256）签发许可证，并将许可证状态存储在 SQLite 中。

## 功能

- 管理员登录、许可证签发和列表查看
- 为平台及可选模块设置独立有效期
- 使用 RS256 签发可离线校验的 JWT 许可证
- 激活、状态检查和永久销毁许可证
- SQLite 持久化，并可自动迁移旧版 JSON 记录
- Docker Compose 部署，数据卷和密钥与应用代码分离

## 技术栈

- API：Rust、Axum、SQLite（rusqlite）、jsonwebtoken
- 管理台：Next.js、React、TypeScript
- 部署：Docker、Docker Compose

## 项目结构

```text
api/        Rust API 服务
web/        Next.js 许可证管理台
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
  admin-token.txt   管理员令牌
```

可使用 OpenSSL 生成一组开发密钥：

```powershell
New-Item -ItemType Directory -Force secrets | Out-Null
openssl genrsa -out secrets\private.pem 2048
openssl rsa -in secrets\private.pem -pubout -out secrets\public.pem
[guid]::NewGuid().ToString('N') | Set-Content -NoNewline secrets\admin-token.txt
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
| `LICENSE_CENTER_ADMIN_TOKEN` | 管理员令牌 | 必填 |
| `LICENSE_CENTER_HOST` | API 监听地址 | `127.0.0.1` |
| `LICENSE_CENTER_PORT` | API 监听端口 | `8779` |
| `LICENSE_CENTER_DB_PATH` | SQLite 数据库路径 | `api/.license-center.sqlite3` |

## API 概览

除健康检查外，管理接口需要管理员会话 Cookie，或在请求头中使用 `Authorization: Bearer <admin-token>`。

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `GET` | `/healthz` | 服务健康检查 |
| `POST` | `/api/session` | 使用管理员令牌创建会话 |
| `GET` | `/api/session` | 查询当前会话状态 |
| `POST` | `/api/licenses` | 签发许可证 |
| `GET` | `/api/licenses` | 列出许可证 |
| `POST` | `/api/licenses/{license_id}/activate` | 激活许可证 |
| `GET` | `/api/licenses/{license_id}/status` | 校验许可证状态 |
| `POST` | `/api/licenses/{license_id}/destroy` | 永久销毁许可证 |

## Docker 部署

首次部署会上传本地 `secrets/` 目录中的密钥和管理员令牌：

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-license-center -Initialize
```

后续发布会保留服务器上的密钥与 SQLite 数据卷：

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-license-center
```

默认将管理台暴露在 `8778`，API 暴露在 `8779`。可通过 `-WebPort` 和 `-ApiPort` 调整。低代码平台应配置 API 地址，例如 `http://example.com:8779`，而不是管理台地址。

更多部署约定请见 [deploy/README.md](deploy/README.md)。

## 数据与安全

- 许可证数据默认保存为 SQLite 文件；容器部署时保存到独立 Docker 数据卷。
- 若旧版 `.license-center-licenses.json` 和 `.license-center-revocations.json` 存在，服务会在首次启动时导入它们。
- `secrets/`、数据库、日志及本地构建文件均已加入 `.gitignore`。
- 生产环境请使用 HTTPS，安全保管私钥与管理员令牌，并定期备份 SQLite 数据卷。

## 许可证

尚未声明开源许可证。使用、复制或分发前请先与项目维护者确认授权条款。
