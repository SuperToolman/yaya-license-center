# 官网手机号客户注册与登录

官网：`E:\OverPorject\yaya-low-code\yaya-website`（8744）。运营 API：8779。注册信息直接存入运营平台 `customers` 表，后台客户管理无需额外同步。

## 身份流程

1. `POST /api/website/auth/send-code` 接收 `mobile`，使用阿里云短信发送，不提前返回客户是否存在。
2. `POST /api/website/auth/verify` 接收 `mobile`、`code`。只有短信校验成功后才查找客户。
3. 唯一且启用的已有客户获得 `state=authenticated` 和客户会话。未查到客户则返回 `state=registration_required` 与 10 分钟有效的一次性注册票据。
4. `POST /api/website/auth/register` 接收 `registrationToken`、`companyName`、`contactName`。事务内重新检查手机号，创建客户并建立会话，消费注册票据。新客户 `name=companyName`、`contact_name=contactName`、`contact_phone=已验证手机号`、`status=active`、`notes=来源：官网手机号注册`。
5. `GET /api/website/auth/session` 使用 `Authorization: Bearer <客户会话>` 读取本人资料。`POST /api/website/auth/logout` 撤销该会话。

公司名称和联系人只在新客户注册步骤必填，不改写已有客户档案。重复手机号的历史客户数据会拒绝登录，须由运营人员处理歧义。禁用客户不能登录。并发注册在 SQLite IMMEDIATE 事务内串行重查，不产生重复客户。客户身份不具有管理人员权限，不创建管理员或自动开通低代码实例与授权。

## 配置与运行

运营平台 `deploy/.env` 配置阿里云短信环境变量，参数名称与低代码平台一致，见 `deploy/.env.example`。本地 `api/start-dev.ps1` 仅从配置文件加载短信白名单参数；容器由 Compose 传入。设置后重启 API。官网设置 `OPERATION_CENTER_API_URL` 指向该运营 API；这个地址仅在官网服务端使用。

新增 `website_sms`、`website_sms_limits`、`website_registration`、`website_sessions` 四张表，启动幂等建表，不改动现有客户表结构。验证码保存随机盐摘要；令牌保存 SHA-256 摘要。短信过期 60 秒，发送间隔 60 秒，每手机号每小时 10 次、每直连来源 IP 每小时 30 次，验证码累计 5 次错误即失效。发送失败仍保留冷却时间和计数；发送未成功的验证码不可用于登录。

官网中转将票据和会话写入 HttpOnly Cookie，生产仅 HTTPS 使用 Secure Cookie，不把令牌返回浏览器 JS。响应禁止缓存。审计脱敏 `sessionToken`、`registrationToken`。官网 POST 校验同源 Origin。外网需在可信网关按真实客户端 IP 设置额外限流；运营 API 当前按直连 IP 计数，不直接相信外部 `X-Forwarded-For`。

## 验证

`cargo test --offline website_auth` 覆盖新旧客户、验证码有效期与错误次数、发送限流、公司/联系人必填、票据失效与重放、禁用/重复手机号、客户资料变化、客户与管理员身份隔离、审计脱敏，以及两个数据库连接同时注册同一手机号。测试使用临时或内存数据库，不发送真实短信，不修改业务客户。
