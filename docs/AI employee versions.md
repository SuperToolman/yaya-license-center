# AI 员工版本

AI 员工仅使用 version。市场目录、授权快照、平台安装接口均返回员工自身版本，不再将 Skill 版本相加，也不暴露 packageVersion。

员工配置保存和状态变化按既有规则递增员工版本。Skill 更新会递增所有引用该 Skill 的员工版本，平台比较员工版本即可发现 Skill 更新。启动时不重新计算版本。历史数据库的 package_version 遗留列不再使用，新建数据库不创建它。

平台旧安装中 templateVersion 可能是合成值，需重新同步以建立明确的 employeeVersion；不可直接把旧合成值改称员工版本。

脚本依赖未就绪不再阻止下载员工与 Skill。平台明确显示该状态，执行服务恢复后可在同版本下再次更新准备依赖；依赖构建拒绝仍保留原安装。Python Skill 应提供完整的 requirements.lock，固定传递依赖并包含 wheel SHA-256；任务容器不在执行中临时 pip 安装。
