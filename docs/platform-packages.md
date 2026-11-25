# 平台离线镜像 ZIP 发布与版本管理（格式 v3）

## 制作与交付

在 E:\OverPorject\yaya-low-code 使用 Node.js 24 和 Docker Buildx：

```powershell
npm run package:platform
# 或 PowerShell 入口
./scripts/package-platform.ps1
# 自定义输出和目标架构
npm run package:platform -- --output E:/artifacts/yaya --platform linux/amd64
```

版本直接复用 /settings/about 的 CHANGELOG 解析函数。脚本从当前工作树构建 Linux 镜像，用临时容器验证 API /healthz 与 Web /login，通过后执行 docker image save，流式计算归档摘要并生成 dist/releases/<版本>-<UUID>.zip。发布不执行 docker push、login，不需要镜像仓库账号。首次构建仍可能联网下载 Rust/Node/Valkey 基础镜像和依赖；客户部署端只导入业务镜像。

默认 linux/amd64，可通过 --platform 或 YAYA_PLATFORM_IMAGE_PLATFORM 指定 linux/arm64。构建完成后核对镜像 ID 和实际架构；目标 Docker 架构必须与包一致。镜像标签为 yaya-platform:<版本>-<UUID>。Docker CLI 可通过 DOCKER_CLI 指定，Windows 自动识别用户目录安装的 Docker Desktop CLI。

压缩包采用 ZIP64，只包含两个普通文件：manifest.json 和 image.tar；后者为完整 docker save 归档，包含运行环境与镜像层，不是源码。失败时不发布 ZIP，并清理临时导出目录和半成品。当前不提供增量包、源码包、仓库引用包或自动上传。

## 清单契约

```json
{
  "packageType": "yaya-platform",
  "formatVersion": 3,
  "artifactType": "offline-image",
  "version": "1.13a",
  "packageId": "7e375d56-7207-4f4d-8bb0-1192328ec398",
  "builtAt": "2026-11-09T00:00:00.000Z",
  "image": {
    "id": "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    "tag": "yaya-platform:1.13a-7e375d56-7207-4f4d-8bb0-1192328ec398",
    "platform": "linux/amd64",
    "archive": "image.tar",
    "size": 123456,
    "sha256": "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
  }
}
```

示例 size 和 sha256 仅展示结构，实际必须匹配 image.tar。镜像 ID 是本地镜像配置的内容标识，归档 SHA-256 校验传输完整性；两者不能混用。包不包含客户环境变量、客户数据库或许可证私钥。镜像仍可被拥有 Docker 管理权限的人导出分析，发布前须检查构建上下文和镜像内的敏感文件。

## 上传与版本管理

平台超级管理员进入平台管理 → 平台版本管理，上传完整 ZIP，查看版本、UUID、大小、镜像 ID、架构、构建与上传时间。API GET/POST /api/platform-versions、GET /api/platform-versions/{id}/download 均要求 platform_admin；变更校验 Origin。

ZIP 上限 8 GiB，image.tar 展开上限 16 GiB，manifest 上限 8 MiB。采用流式上传、流式 SHA-256 和下载，不把镜像归档整体读入内存。拒绝旧格式、未知字段、非法 UUID/时间/版本/架构/标签、缺少归档、大小或 SHA-256 不符、CRC 错误、目录、链接、重复条目、多余文件和路径穿越。上传不调用 Docker，不执行归档内容；tar 是否能被 Docker 导入、是否包含所声明镜像和所需镜像层，在部署时由 docker load 和 image inspect 再验证。

UUID 或整个 ZIP SHA-256 重复返回 409。先写随机临时文件并校验，再以不覆盖现有文件的硬链接保存并提交元数据，失败清理半成品。下载流式返回原 ZIP。当前没有数字签名或发布审批，校验和只证明一致性，管理员上传权限属于信任边界。

只显示和选择 offline-image 制品；既存源码包及仓库清单包不再支持。数据库迁移历史保持不变，已有实例和审计数据不自动删除。

## 实例上线与存储

新建实例选择客户与新版本，填写域名、容器名、端口、环境变量并部署。运营 API 校验 ZIP 入库摘要、归档摘要后，只展开固定的 image.tar 到任务临时目录；使用本机 Docker 导入，核对镜像 ID/架构，再以镜像 ID 和 --pull never 创建容器。无需仓库凭据和运行端编译工具。后续启动、数据卷、环境加密、许可证公钥和健康检查流程见 [客户实例部署](./platform-delivery.md)。

包存储默认为数据库同级 platform-packages，可用 YAYA_PLATFORM_PACKAGES_DIR 覆盖。备份须同时包含 SQLite、ZIP、环境加密密钥和客户数据卷。ZIP 存储盘、临时盘和 Docker 数据盘都要有足够空间：发布端需容纳导出 tar + ZIP，部署端需容纳 ZIP + 展开 tar + 导入镜像。失败或健康检查超时不自动删除客户容器和数据卷。

反向代理/网关需单独放宽上传体积至 8 GiB 加 multipart 开销，并提供足够上传超时。当前无分片上传和断点续传；连接中断需重新上传。业务自身调用短信、模型等外部服务仍可能联网。

## 验证

脚本测试覆盖完整归档、摘要、禁用仓库/源码参数、健康检查失败和导出失败清理。Rust 测试覆盖格式和摘要、路径、超限、权限、上传下载、重复包、12 MiB 请求穿过旧全局限制、固定归档提取、实例和任务恢复。Playwright 覆盖版本上传、镜像信息、实例部署和归档导入错误。

2026-11-09 已完成真实低代码平台 1.13a 的镜像构建、启动自检、离线 ZIP 导出（约 507 MiB）、真实运营页面上传、新建客户实例、本机 Docker 导入与启动，以及容器重启后的数据库/平台文件持久化检查。Web /login 和 API /healthz 均返回 200；详见 [真实验收记录](./offline-release-e2e-2026-11-09.md)。尚未验收客户业务流程、短信、许可证激活、公网 HTTPS、运行中版本升级和完整备份恢复。
