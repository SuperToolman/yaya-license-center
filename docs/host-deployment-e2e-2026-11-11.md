# 宿主机迁移与升级验收

日期：2026-11-11（北京时间）。目标：47.112.107.44，CentOS 7 x86_64，kernel 3.10、glibc 2.17、systemd 219、Docker 26.1.4。

## 结果

运营平台由旧 Docker 容器 `yaya-license-center` 迁移为宿主机 systemd 服务，旧容器已移除。随后通过 Windows `deploy/publish.ps1` 完整执行一次后续覆盖升级，退出码 0。当前发布：`/opt/yaya-operation-center-service/releases/0.1.5-20261111024019-da3004a9`。

管理台 `http://47.112.107.44:8778`，API `http://47.112.107.44:8779`。公网根页面返回 307（正常重定向），API `/healthz` 返回 code=0。8778/8779 监听进程分别为原生 Next.js 与 Rust API，不再是 docker-proxy。

## 环境与修正

- 独立环境 `/opt/yaya-native-runtime`：Node 22.22.0 glibc-217 兼容构建、Python 3.11.11、pnpm 11.1.2、Rust 1.99.1。未替换系统 glibc、Python 2，也未重启 Docker。
- API、主机监控、Next.js production 均在目标服务器原生构建成功。
- 根 pnpm 锁文件补齐 xterm 依赖，与现有 package.json 对齐；未通过跳过冻结锁文件来发布。
- 原 `/login` 健康检查路径已失效，迁移首次因此触发停服保护；改用根页面及 `/operation-api/healthz` 后通过，正常覆盖升级也通过。
- 平台使用专用系统账户，加入现有 Docker 组；主机监控允许同 UID 的 API 通过 Unix Socket 读取。

## 数据与功能验证

迁移冷备份与升级后数据库比对：users=1、customers=1、orders=4、transactions=4、licenses=3、ai_employee_products=2、operation_skills=2、providers=0。旧密钥、管理员初始密码逐字节一致。使用与 API 同源的 SQLite 编译库执行 `PRAGMA quick_check` 返回 ok；CentOS 系统自带 SQLite 3.7 不支持新索引，不能用于新库验收。

管理员原密码登录成功，读取 `/api/session`、`/api/customers`、`/api/orders`、`/api/licenses`、`/api/ops/local/containers`、`/api/ops/local/inventory`、`/api/ops/nodes`、节点实时指标均成功。验证完注销测试会话，未重置管理员密码。

主机返回 8 个逻辑 CPU、67,387,002,880 bytes 总内存；GPU 返回 `library_or_driver_unavailable`，没有伪造 GPU 指标。普通覆盖升级后节点 identity.json 一致。

`yaya-operation-center.target`、API、Web、host-monitor 均 active，target enabled。其他 8 个业务容器保留原运行状态和运行时长。

## 保留的恢复来源

- 旧 Docker 卷：`yaya-license-center-data`，未删除。
- 容器迁移冷备份：`/opt/yaya-operation-center-service/backups/legacy-0.1.5-host-migration-20261111`。
- 覆盖升级前快照：`/opt/yaya-operation-center-service/backups/0.1.5-20261111024019-da3004a9`。
- 原生首版和旧运营平台镜像保留，不自动清理。

## 尚未验证

未做重启服务器验收、备份恢复实际回退、全新空机初始化、全部业务写入回归、真实浏览器容器终端及 GPU 设备验收。CentOS 7 的兼容构建实测可运行，不等于官方长期支持环境。
