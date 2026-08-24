# Skill routing description 取消长度门禁设计

日期：2026-08-24

## 目标

允许 Skill 市场发布和本地路由解析使用超过 240 个字符的
`description` 与 routing card，同时保留 frontmatter、routing card 结构、
非空字段和必需触发信息校验。

## 方案

- 删除发布校验对 top-level `description`/`short-description` 的 240 字符拒绝。
- 删除共享 `SkillRoutingCard` 对字段总字符数的 240 字符拒绝，使市场发布、
  inventory 扫描和 Agent 投影采用同一套可解析数据。
- 保留 `ROUTING_DESCRIPTION_MAX_CHARS` 常量及 inventory 的统计和超限标记，
  以避免改变现有响应字段和 UI 兼容性；该标记不再阻断发布或解析。

## 验证

- 静态确认发布与版本新增都继续调用结构校验。
- 使用 `cargo fmt --check` 和 server-runtime `cargo check` 验证编译与格式。
- 桌面模式 `cargo check` 若被仓库既有错误阻断，应记录具体文件与错误，
  不把 server-runtime 结果表述为桌面构建通过。
- 用 `git diff --check` 检查补丁。
