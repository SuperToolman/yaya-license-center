# 平台版本包上传

## 交互

`/platform-versions` 的“上传平台包”直接打开浏览器文件选择对话框，选定 ZIP 后立即打开 HeroUI 上传模态框并开始请求，不再在页面内显示独立文件选择控件、上传结果或上传错误。

模态框展示文件名、真实传输百分比、传输大小、平均速度、已耗时及按当前平均速度估算的剩余传输时间。文件传输完成后切换到“服务器正在校验”，显示校验等待时长，不虚构校验进度或预计时间。收到 API 入库成功结果才显示成功并刷新列表。错误保留在模态框中，可以使用同一文件重新上传。关闭终态模态框后可选择其他包。

上传与校验期间不能直接关闭弹窗，需使用“取消上传”中断连接；取消不代表能够撤销已经提交到服务器的校验或入库。网络中断或校验阶段取消后，先刷新列表确认结果再重试。后台校验期间的临时文件清理由既有 API 负责。

## 线上 HTTP 500 根因

2026-11-12 读取生产 Web journal：14:07:33 出现 `Request body exceeded 512MB for /operation-api/api/platform-versions`；14:08:03 出现 `Failed to proxy ... Error: socket hang up / ECONNRESET`。当前磁盘仍有约 14 GiB 可用，此次明确证据指向 Web 转发限制。

原页面通过 Next rewrite 转发平台包。Next 请求体克隆上限设置为 512 MiB，超出后截断，而 rewrite 默认上游超时为 30 秒。两项限制都不适用于 API 已支持的最大 8 GiB 离线镜像 ZIP。rewrite 返回非 JSON 错误后，页面统一解析器显示“运营服务返回了无效响应（HTTP 500）”。

## 流式上传通道

同源 `/operation-api` 部署上传改走专用 Next Node 路由 `POST /api/platform-package-upload`，绕开 rewrite。请求使用 Node Stream pipeline 和背压直传固定 API `/api/platform-versions`，不调用 `formData()`、`arrayBuffer()`，不在内存克隆整包。流式限制为 8 GiB 加 1 MiB multipart 开销，API 继续精确限制 ZIP 为 8 GiB 并校验 SHA-256、包结构、管理员权限与 Origin。

仅转发必要的认证、Origin 和 multipart 请求头，目标地址由 `YAYA_OPERATION_API_UPSTREAM` 配置，浏览器不能指定上游。上游空闲超时为 30 分钟；浏览器整次请求超时为 1 小时。服务响应保留结构化错误，并限制错误/结果响应读取量为 1 MiB。跨端口开发环境继续直接请求 API，沿用现有认证与 CORS。

## 验证

- 浏览器测试 5 项通过：按钮打开文件选择器、选择后自动上传、模态框结果与列表刷新、错误重试、校验等待/非 JSON HTTP 500、非管理员禁止、手机布局。
- 使用独立上游和真实 Next 开发服务，流式传输 513 MiB（537919488 字节）完整到达上游，认证及 Origin 头保留；延迟 31 秒响应也成功，不再触发旧的 512 MiB / 30 秒限制。测试没有上传或修改生产版本包。
- 以上为本地实现和测试；服务器已安装版本尚未包含此次修改，需要发布后才会启用新通道。
