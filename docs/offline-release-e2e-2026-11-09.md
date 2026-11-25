# 离线发布与平台实例部署验收记录

日期：2026-11-09。结论：真实低代码平台发布、运营页面上传、新建实例和本机 Docker 部署全链路通过。

## 测试环境

使用现有运营平台 http://127.0.0.1:8778 和 API http://127.0.0.1:8779，通过管理员会话操作。最初用于准备的独立测试服务已经停止；最终客户、版本和实例均登记在现有运营平台中。未重置管理员账号。

## 发布产物

- 版本：1.13a，与设置关于页面使用同一 CHANGELOG 版本解析逻辑。
- 文件：E:\OverPorject\yaya-low-code\dist\releases\1.13a-c943efba-82e9-4414-a04c-7a8c97e21b19.zip
- 大小：531793973 字节（约 507.2 MiB）。
- 包 ID：c943efba-82e9-4414-a04c-7a8c97e21b19
- 镜像 ID：sha256:412abca83d98ba3e4c356d591c52d6260568ef452c439de1b86d223d74196dba
- 架构：linux/amd64
- ZIP SHA-256：35e929e8b2ec95f7d8aacab17e9d4f85a204386a7f4f6e781c9ecbb9bc4e3e5d
- ZIP 格式：v3 offline-image，包含 manifest.json 和完整 image.tar。
- 构建、启动自检、docker save、ZIP 压缩均真实执行，没有向业务镜像仓库推送镜像。构建阶段需要下载基础镜像及依赖；部署阶段从 ZIP 导入镜像并使用 --pull never 创建容器。

## 已保留的客户实例

- 客户：离线发布验收客户-19044280
- 实例：离线平台验收-19044280
- 实例 ID：48f11a2f-240d-4f34-a2ad-ab1f35fd9037
- 容器：yaya-offline-e2e-19044280
- 容器 ID：283d03ebe6d8df4d6075eb8a2cc5d7d7d5af35d16aec509925f9fe27cd8a745d
- 状态：running；部署任务：succeeded / completed。
- 部署任务 ID：b4075e6e-bb09-40e3-a3c0-7f6316eba646
- 登录页：http://127.0.0.1:18887/login
- API 健康检查：http://127.0.0.1:18888/healthz
- PostgreSQL 数据卷：yaya-48f11a2f-240d-4f34-a2ad-ab1f35fd9037-postgres
- 平台数据卷：yaya-48f11a2f-240d-4f34-a2ad-ab1f35fd9037-state

## 实际验证结果

1. 真实镜像构建成功，发布脚本在临时容器中验证 Web/API 后才生成最终 ZIP。
2. Playwright 连接真实运营页面，选择本地 ZIP、点击上传，服务器校验入库成功，列表显示镜像 ID 和平台架构。未使用 API mock。
3. 页面填写所属客户、版本、容器名、Web/API 端口、平台地址和自定义环境变量，点击“创建并部署”成功。
4. 任务经过 validating、loading、checking 到 completed；完整镜像离线导入和容器创建成功。
5. 客户 API /healthz HTTP 200，/login HTTP 200；浏览器登录页已截图检查。
6. supervisorctl 确认 api、web、agent-v2、agent-executor、valkey 全部 RUNNING；PostgreSQL SELECT 1 成功。
7. 自定义环境变量 YAYA_OFFLINE_TEST_MARKER 按页面输入传入容器。
8. 在平台数据目录写入临时标记，在 PostgreSQL 独立测试 schema 写入临时记录；重启容器后两者均保留，Web/API 再次返回 200。测试标记和 schema 已清理。
9. 发布脚本已有 5 项自动测试全部通过。
10. 构建时临时修改的 Docker Desktop 主机代理已按备份恢复并读取确认；测试实例保持运行。

## 测试中修复的问题

- .dockerignore 原先使用 **/runtime，误排 worker/src/runtime/long_context.rs；改为排除明确的 api/runtime，保留已有 agent-v2/runtime 数据目录排除。
- Windows CRLF 入口脚本导致 Linux 报 no such file or directory；Dockerfile.single 在构建时统一入口脚本换行符。
- 发布启动自检原先没有配置生产环境所需的数据库连接和执行器环境；已补齐为容器内服务连接及随机密钥。
- 发布自检早期失败也会附带容器启动日志，便于定位。

## 证据与范围

原始结果：E:\OverPorject\yaya-operation-center\api\runtime\dev\offline-e2e-1794218457395/result.json

页面截图：

- E:\OverPorject\yaya-operation-center\api\runtime\dev\offline-e2e-1794218457395/versions-uploaded.png
- E:\OverPorject\yaya-operation-center\api\runtime\dev\offline-e2e-1794218457395/instance-deployed.png
- E:\OverPorject\yaya-operation-center\api\runtime\dev\offline-e2e-1794218457395/deployment-log.png
- E:\OverPorject\yaya-operation-center\api\runtime\dev\offline-e2e-1794218457395/customer-login.png

本次验收覆盖发布、版本上传、实例创建、Docker 部署、服务启动及重启持久化。未执行客户业务表单操作、短信发送、许可证激活或公网域名 HTTPS 接入测试。测试实例使用本机 HTTP 地址。
