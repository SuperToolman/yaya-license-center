**目标结构**

```text
app/
  layout.tsx
  login/page.tsx
  (operation)/
    layout.tsx
    orders/
      page.tsx
      components/
        OrdersPage.tsx
        OrderModal.tsx
        PaymentModal.tsx
    skills/
      page.tsx
      components/
        SkillsPage.tsx
        SkillModal.tsx
        DeleteSkillDialog.tsx
    ai-employees/
      page.tsx
      components/
        AiEmployeesPage.tsx
        EmployeeModal.tsx
        DeleteAiEmployeeDialog.tsx
    ...
  components/
    OperationPageLayout.tsx
    OperationSidebar.tsx
    OperationPageHeader.tsx
    OperationProvider.tsx
    OperationAuthGate.tsx
    LoadingState.tsx
    ErrorState.tsx
    ConfirmAction.tsx
  lib/
    operation-api.ts
    operation-types.ts
```

所有 React 组件文件和组件名使用 PascalCase。页面专属组件只放在该页面目录的 `components/`；跨页面复用组件才放 `app/components/`。

**重构步骤**

1. 建立路由组 `(operation)`
   不改变现有 URL，例如 `/skills` 仍是 `/skills`，但运营后台页面统一受 `(operation)/layout.tsx` 管理。

2. 拆掉根布局劫持
   根 `app/layout.tsx` 只保留 HTML、全局样式和 Metadata。删除 `OperationApp` 对 pathname 的拦截，不再由根布局替换 `children`。

3. 建立运营后台布局
   `(operation)/layout.tsx` 渲染：
   - `OperationAuthGate`
   - `OperationProvider`
   - `OperationPageLayout`
   - `OperationSidebar`

   页面布局只负责侧栏、内容宽度、路由高亮、页面标题区和刷新入口，不渲染任何业务表格或业务模态框。

4. 页面独立声明标题与操作
   使用 `OperationPageHeader` 或 Layout Context，让各 `page.tsx` 声明标题、说明和 actions。

   例如 Skills 页面只声明：
   - 标题：`Skills 管理`
   - action：`导入 ZIP`
   - 内容：`SkillsPage`

   不允许 layout 中再出现 `section === "skills"`。

5. 提取统一 API 层
   将当前 `request`、`post`、错误解析和 Cookie 认证迁移到 `app/lib/operation-api.ts`。
   每个页面只请求自身需要的数据，例如 Skills 页不应加载订单、财务和日志。

6. 提取共享会话状态
   `OperationProvider` 只管理：
   - 当前用户
   - 登录态
   - 登录/退出
   - 通用刷新事件

   不持有订单、Skills、员工、财务等全部业务数据。

7. 先迁移 Skills 页面
   将以下内容从 `operation-center.tsx` 移至 `app/(operation)/skills/components/`：
   - `SkillsPage`
   - `SkillModal`
   - `DeleteSkillDialog`
   - ZIP 导入与 ZIP 更新动作

   完成后 [`app/skills/page.tsx`](E:\OverPorject\yaya-operation-center\web\app\skills\page.tsx) 实际渲染 `SkillsPage`，不再返回 `null`。

8. 按业务域逐页迁移
   推荐顺序：
   1. Skills
   2. AI 员工
   3. 客户
   4. 订单与回款
   5. 许可证
   6. 财务
   7. 应用申请与应用管理
   8. 服务商、返利、用户、日志

9. 清除旧总控
   所有页面迁移完成后，删除：
   - `app/components/operation-center.tsx`
   - `app/components/operation-app.tsx`
   - `sectionMeta`
   - `SectionContent`
   - 所有 `section === ...` 分发逻辑
   - `.backup-broken` 旧文件

10. 验收标准
   - 每个 `page.tsx` 都真实渲染其页面组件。
   - 访问 `/skills` 不会请求订单、财务或日志接口。
   - 页面顶部 action 由当前页面拥有。
   - 不存在 `OperationCenter`、`OperationApp` 或 `SectionContent`。
   - `next typegen`、`tsc --noEmit`、关键页面交互测试通过。
   - 路由切换后保持登录态，不发生全量重复请求。