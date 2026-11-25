# Skill 环境变量动态配置

`/skills` 编辑框按 Skill 实际包含的环境变量动态渲染，以真实变量名作为标签。不按名称、包名、BOM 业务或固定字段决定界面，没有环境变量就不出现配置输入项。

## 变量识别

后端读取 Skill 包根目录 `.env.example` 的 `NAME=` 声明，也识别 Python `os.getenv`、`os.environ.get`、`os.environ[...]`，JavaScript/TypeScript `process.env.NAME`/`process.env[...]`、PowerShell `$env:NAME`、Shell `${NAME}` 等常见字面量访问。不执行导入的脚本，不读取宿主机环境变量或 Skill 的真实 `.env` 值。对于计算产生的变量名或未支持的语言，将变量名列入 `.env.example` 即可。示例文件中的值不会作为实际配置导入。

## 数据流

- 列表/详情返回 `environmentVariables: [{name, secret}]` 以及当前 `environmentValues: {NAME: value}`。
- 编辑框动态生成 HeroUI Input。常见 SECRET、PASSWORD、TOKEN、API_KEY、PRIVATE_KEY 名称使用密码输入；这是遮挡显示，不代表值加密。
- `POST /api/skills` 接收 `environmentValues`，校验变量必须出现在 Skill 中，按 Skill 独立保存。可留空，值不会自动套用 URL 规则。
- 沿用现有 Skill 和员工包版本机制。签发许可证和员工安装包携带通用 `environmentValues`，平台按现有不可变员工包流程持久化并纳入内容指纹。
- 替换包后重新识别变量，同名配置保留，移除的变量不会显示或继续分发。购买端需同步员工新版本。
- 旧 `component_library_api_url` 列仅用于兼容先前版本；仅当包确实包含 `COMPONENT_LIBRARY_API_URL` 且通用配置尚未保存时沿用旧值，不用于决定界面。
- 审计响应脱敏整个 `environmentValues`。现有许可证是签名载荷而不是密文，配置会分发给员工安装方，不能用于隐藏运营方不希望交付给客户的私有密钥。

## 运行边界

当前 Agent V2 把当前 Skill 的配置加入员工运行上下文，要求有进程工具时按次传入环境变量，不修改服务全局环境。此改动不新增脚本执行器；若没有进程/环境注入工具，必须明确报告能力缺失，不能宣称系统环境已设置或任务已执行。配置值会进入模型上下文，部署方应仅填写允许在该员工执行环境使用的凭据。

BOM 整理包含 `COMPONENT_LIBRARY_API_URL`、`MOUSER_API_KEY`、`DIGIKEY_CLIENT_ID`、`DIGIKEY_CLIENT_SECRET`。其他 Skill 只按自身代码或声明显示变量；BOM 报价不会被强制要求填写元器件资料库地址。其自身若有供应商 API 环境变量则仍正常显示。
