# 运营平台账号管理

`/users` 管理运营平台登录账号，与客户管理和官网客户账号独立。沿用五种固定角色：`platform_admin`、`platform_operator`、`platform_finance`、`provider_admin`、`provider_sales`。

## 权限与操作

- 平台超级管理员、运营和财务可以查看账号列表；只有超级管理员可以新增、编辑、启停和删除。
- 新增需要账号、初始密码、固定角色和所属组织；账号创建后不可修改。所属组织沿用现有组织 ID 字段，平台账号通常为 `platform`，服务商账号填写服务商 ID。
- 编辑可以修改姓名、角色和组织，并可重置密码；密码留空则保留原密码。
- 当前登录账号仅允许修改姓名，禁止修改自身角色、组织及密码，禁止停用和删除自身。
- 必须保留至少一个启用的超级管理员。修改操作在 SQLite 即时事务中重新检查操作者和目标账号，防止并发操作移除全部管理员。
- 停用后不能登录，已有会话立即失效；重新启用后需要重新登录。密码、角色和组织变化沿用现有会话撤销触发器。
- 删除前必须停用，并在页面确认；删除账号和会话，保留业务数据及历史操作者 ID。历史记录中的姓名若依赖账号实时关联，删除后可能显示为未知用户。
- 新增、修改、启停和删除校验请求 Origin，沿用平台的允许来源配置。

## 接口

| 方法 | 路径 | 请求 |
| --- | --- | --- |
| GET | `/api/users` | 无 |
| POST | `/api/users` | `username`、`password`、`displayName`、`role`、`organizationId`（省略时为 `platform`） |
| PUT | `/api/users/{user_id}` | `displayName`、`role`、`organizationId`、可选 `password` |
| POST | `/api/users/{user_id}/status` | `status` 为 `active` 或 `inactive` |
| DELETE | `/api/users/{user_id}` | 无 |

无需新增数据库迁移，使用既有 users、staff_sessions 和会话撤销触发器。前端操作成功后刷新列表；失败时保留表单或确认弹窗，并显示服务端错误。

## 验证

- API：在 `api` 目录执行 `cargo test user_management`，覆盖账号完整生命周期、会话撤销、权限与来源限制、自身保护、最后一个管理员保护和并发降权。
- 前端：在 `web` 目录执行 `pnpm typecheck` 和 `pnpm exec playwright test tests/users.spec.ts`，覆盖新增失败重试、编辑、启停、删除和只读角色。
