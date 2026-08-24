# Skill Market 旧版路由卡兼容设计

日期：2026-08-24

## 目标

允许不含 `routing` frontmatter 的旧版 Skill 发布到 Skill Market，兼容公开和私有可见性，
同时继续阻止格式错误的新版路由卡进入市场。

验收标准：

1. 不含 `routing` 的公开和私有 Skill 均能通过客户端发布前校验。
2. 合法的 `routing` 保持现有解析、规范化和发布行为。
3. 已声明但字段缺失、类型错误或内容无效的 `routing` 继续返回明确错误。
4. 缺少 frontmatter 或缺少非空 `description` 的 Skill 继续被拒绝。
5. 旧版 Skill 安装后保持 `routing_status=missing`，不冒充有效结构化路由。

## 当前行为

发布和新增版本都会先调用 `validate_routing_descriptions`。该函数读取上传包中的
`SKILL.md`，确认 frontmatter 与描述存在后，调用 `parse_skill_routing`。解析器将没有
`routing` 的旧格式返回为 `SkillRoutingError::Missing`，当前发布校验会把该结果统一转换为
`invalid_input`，因此请求尚未发送到 Fusion API 就失败。

本地 Skill 扫描已经兼容缺失路由卡：它保留普通 `description`，并将状态设置为
`SkillRoutingStatus::Missing`。因此兼容缺口只存在于市场发布入口，不需要修改存储、下载、
安装或 inventory contract。

## 兼容规则

发布校验按以下顺序处理每个 `SKILL.md`：

1. Base64 必须合法，解码结果必须是 UTF-8。
2. 文件必须包含有效且闭合的 YAML frontmatter。
3. `short-description` 或顶层 `description` 必须是非空字符串。
4. `parse_skill_routing` 成功时接受现有结构化路由卡。
5. `parse_skill_routing` 返回 `Missing` 时按旧格式接受，不区分公开或私有可见性。
6. `parse_skill_routing` 返回 `Invalid` 时继续拒绝，并保留具体字段错误。

完全缺少和格式错误必须严格区分。只要 frontmatter 中出现 `routing`，作者就表达了采用新格式
的意图；字段拼写、类型或必填内容错误不能静默退回旧格式。

## 数据流

兼容逻辑仅修改桌面端共享发布前校验：

```text
上传文件 -> 解码 SKILL.md -> 校验 frontmatter/description
         -> routing 合法 -> 发送 /skills 或 /skills/versions
         -> routing 缺失 -> 作为旧格式发送同一请求
         -> routing 无效 -> 本地返回 invalid_input
```

Fusion API 的 multipart contract 和数据库结构不变。安装后的扫描仍从实际 `SKILL.md` 读取
元数据；旧格式自然得到 `routing_status=missing`，合法新格式得到 `valid`。

## 不采用的方案

- 不放行无效 `routing`：这会隐藏作者的格式错误，并造成发布结果与作者意图不一致。
- 不根据长 `description` 自动生成路由卡：客户端无法可靠推断排除条件、别名和调用时机。
- 不在上传包中静默写入字段：发布内容必须与作者选择的文件一致。
- 不按可见性分支：本次要求公开和私有 Skill 使用相同的旧格式兼容规则。

## 错误处理

Base64、UTF-8、frontmatter、描述和无效路由卡继续使用现有 `invalid_input` 错误结构。
只有精确的 `SkillRoutingError::Missing` 被视为兼容情况。服务端请求失败仍沿用现有网络与业务
错误处理，不把服务端错误误报为本地路由校验问题。

## 验证

遵守仓库规则，不新增测试文件，也不默认运行桌面构建或测试。交付前执行：

- Rust 格式检查或对目标文件执行 `rustfmt`。
- 静态检查 `publish_core` 与 `add_version_core` 的共同校验路径。
- 分别检查合法、缺失和无效 `routing` 三个解析分支。
- 确认 inventory 仍将旧格式投影为 `routing_status=missing`。
- 执行 `git diff --check` 并确认最终暂存范围只包含本次文件。
