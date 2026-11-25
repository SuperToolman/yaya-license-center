# 前端架构与样式约定

## 实现边界

沿用 Next.js 16 App Router、React 19、HeroUI 3 和 Tailwind 4。HeroUI 负责组件外观和语义色；Tailwind 负责布局、间距、尺寸和响应式排布。不建立 CSS Modules 外观体系，不直接覆盖 `.chip`、`.modal__dialog` 等库类，不使用内联颜色补丁。

`web/app/styles.css` 只加载 Tailwind、HeroUI 和原有全局主题变量。原有灰蓝主题保留，组件尺寸使用 HeroUI 的 `size`，宽编辑器及头像裁剪使用局部布局尺寸。原有全局弹窗、字距、滚动条覆盖已移除，滚动条回归组件默认行为。

## 串色根因及修复

复现：下架员工 A 后，员工 B 的文本仍为“上架”，但 DOM class 从 `chip--success` 变成 `chip--default`。因此不是员工状态数据被修改，也不是简单的 CSS 优先级问题。

`tailwind-variants@3.3.0` 的 slot resolver 复用同一组函数并更新共享 `currentProps`。HeroUI 的 `useMemo` 保留了上一实例的 slot 函数，后续渲染可能读取最后一次变体调用的属性。独立调用也能复现：创建 success slots，再创建 default slots，原 success slots 的 `base()` 会返回 default。

使用 `web/patches/tailwind-variants@3.3.0.patch` 修复 ESM 和 CJS：每次 slots 调用建立独立 resolver，保持调用的属性独立。版本不变，没有修改 node_modules 作为交付方式。根目录与 web 子目录的 pnpm 配置、锁文件均记录补丁；Docker 在依赖安装前复制 patches。

该补丁牺牲跨调用 slot resolver 复用，换取正确的实例隔离；后续升级依赖时先运行 `tests/dependency.spec.ts` 和浏览器颜色测试，确认上游已修复再移除补丁。

## 目录与职责

- `app/(operation)/<domain>/page.tsx`：路由入口，URL 不变。
- 各域 `components/`：页面、表格、编辑器、确认弹窗；`hooks/`：多资源数据装配。
- `app/components/ui/`：薄封装的 HeroUI 表格、标签、统计、弹窗和操作组件。`OperationUi.tsx` 保留兼容导出。
- `app/hooks/useOperationQuery.ts`：可取消查询、重试、保留上次成功结果，隔离旧 key 与旧请求。
- `app/hooks/useOperationMutation.ts`：同步 ref 锁阻止重复提交，统一错误及 finally 状态释放，卸载后跳过回调。
- `app/hooks/useOperationData.ts`：订阅运营页已有刷新事件的资源查询。
- `app/lib/operation-api.ts`：兼容原有 `{response,payload}` 返回，新增严格数据读取和命令助手；处理 HTTP、网络和无效 JSON。
- `app/lib/types/`：按业务域划分类型；`operation-types.ts` 保留兼容类型导出；`format.ts` 管理金额和日期格式化。
- `app/provider-portal/components/ProviderPortal.tsx`：独立服务商门户，复用请求、类型和格式化。

已拆除跨业务 `ReadPages.tsx`，财务、应用、服务商、返利、用户和日志分别拥有页面。员工及 Skills 表格和删除确认分别独立，客户编辑器独立。订单现有编辑、回款、签发和详情逻辑保留。

`OperationProvider` 仅持有会话和通用刷新计数，不承载业务列表。读取失败保留已加载列表；写入失败保留表单，仅成功后关闭及刷新。删除资格每次打开重查，不保留上次资格错误。接口、Cookie 和后端业务规则不变。

## 验证命令与结果

在仓库根目录执行：

```powershell
pnpm --dir web run lint
pnpm --dir web run typecheck
pnpm --dir web exec playwright install chromium
pnpm --dir web run test:e2e
pnpm --dir web run build
```

测试启动独立 8780 端口，输出至 `.next-test`，所有业务接口由 Playwright 拦截，不操作真实上下架、订单或许可证。`PLAYWRIGHT_CHANNEL=msedge` 可选用已安装 Edge。

已通过 26 项回归：依赖 slot 隔离；13 个页面路由；重复 Skills、空 Skills、多 Skills、未知状态；状态切换和路由往返颜色；失败重试；重复提交；删除资格重查；ZIP 上传；订单保存、回款、签发契约；非 JSON 响应；离页取消查询；1440、768、390px 弹窗布局。

ESLint、TypeScript 检查通过。Windows 本地生产构建通过编译、类型检查、17 个静态页面生成；standalone 最终打包因本机不允许创建符号链接而报 EPERM，不能记为完整构建通过。保持部署所需 `output: standalone`，没有为了绕过权限改变部署输出。Docker/Linux 完整构建需在对应环境验证。

当前未提交的后端和订单/许可证功能为重构基线；本次不回退这些变更。
