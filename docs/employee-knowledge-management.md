# 员工专属知识库管理与客户同步

2026-11-06。首批管理功能已编码，真实安装/数据库链路待手测，尚不包含 RAG。

在 AI 员工编辑模态框增加知识区：先保存员工再上传原件，上传/移除知识是独立操作，会立即递增员工版本。支持格式与限额见页面；SQLite 自动初始化 employee_knowledge_documents。客户端更新通过现有员工包端点获取可选 knowledge manifest，并按许可证下载文档、核验 hash 后事务保存。

许可证 JWT 不携带文档内容，旧无知识员工包兼容。移除文档不修改客户自建知识，客户需更新员工后从当前知识集合移除该资料；实时远端撤回传播尚未实现。删除员工触发清理该员工的知识资源。单文件 8 MiB，当前有效文档最多 20 份/40 MiB。

本轮 cargo check（独立 target）、定向 ESLint、TypeScript 通过。未执行回归/浏览器测试、实际 SQLite 升级或客户市场安装操作。

完整合同、权限及待验收项见相邻项目 yaya-low-code/docs/agent-v2-knowledge-management-report.md 和 agent-v2-knowledge-rag-architecture.md。启动沿用根目录 start-dev.ps1，客户需配套升级 API/Web 并执行知识迁移114；本轮没有更改正式商品、许可证、用户角色或业务数据。

## 专项测试补充（2026-11-06）

`cargo test --locked --manifest-path api/Cargo.toml --target-dir api/target/knowledge-validation employee_knowledge_sqlite_lifecycle -- --nocapture`：1 passed / 0 failed。使用全新临时 SQLite 并重复初始化，实际验证上传、hash、幂等、版本递增、非法输入、未登录/无许可证拒绝、移除原件字节与员工删除清理。直接调用真实处理器，不等同 HTTP、浏览器或真实许可证下载安装验收。测试未修改正式商品和许可证。
