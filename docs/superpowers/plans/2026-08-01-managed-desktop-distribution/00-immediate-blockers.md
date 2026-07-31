# Task 00：立即阻断 P0 安全与确定性故障

## 目标

在共享 schema 和大规模重构前先止损：移除已知凭据、禁止破坏性 Skill 更新、让不可安装版本停止无意义重试，并修复渠道/记忆中已经确定的单点断链。此任务不替代 Task 03/08/09 的完整架构。

## 执行条件

- 这是实现任务，开始前仍需满足各仓授权规则，特别是 Fusion API 必须有用户明确“开始做”。
- 单 Agent 串行完成，避免与后续任务修改同一文件。
- 每个修复先建立失败证据，再做最小变更。

## scope_write

- 桌面 `src-tauri/src/system_skills/` 中凭据和 destructive update gate。
- 桌面 `src-tauri/src/commands/skill_market/` 的 artifact-less 失败识别与错误分类。
- `src-tauri/src/commands/chat_channel.rs`、channel list/add/edit/config 的已确认连接与 merge 问题。
- `src-tauri/src/user_memory/context.rs` 和记忆设置页的硬编码 fallback/marker 破坏问题。
- 必要的定向回归测试；桌面测试只由远端 CI 运行。

## 禁止修改

- SQL、后端领域/API、router/bootstrap、依赖、lockfile、打包配置。
- 为临时恢复下载而取消对象摘要、ZIP 内容摘要、路径或大小上限。
- 完整重做渠道 readiness、记忆 harvest 或版本中心。

## P0-1：系统 Skill 凭据与 destructive update

1. 立即轮换暴露的仓库凭据；代码提交只能证明旧值已删除，轮换证据单独保存且不含新值。
2. 删除源码中的用户名/密码和 URL credential 拼接。
3. 后续若仍需临时 Git 读取，只从部署/用户受信 credential provider 取值，日志不打印 URL userinfo。
4. dirty checkout 不再自动 force reset；返回“存在本地修改，自动更新已停止”。
5. 在 Task 04/06 完成 TOS 系统 Skill 前，允许只读现有版本，但默认暂停自动 Git 更新。

验收：secret scan 不再命中值；dirty fixture 更新后文件摘要不变；无凭据时返回配置错误而非匿名反复重试。

## P0-2：Skill 安装错误止损

当前逐文件版本的 `packageSize` 是 raw size，`objectSha256` 也不是实际 ZIP 摘要。最小止损：

- 客户端识别 metadata 不完整/空对象摘要并立即返回 `artifact_not_ready`，不做三次相同下载。
- UI 显示“制品准备中/当前版本暂不可安装”，不再误报网络不稳定。
- 保留 size、object sha、content sha 和 ZIP 安全校验；禁止仅删除 expected-size 判断。
- 已有真实 legacy ZIP 且 metadata 完整的版本保持可安装。
- 完整修复由 Task 01/03 创建真实 artifact、回填并 TOS 直下。

验收：问题版本零下载请求或只做一次 metadata 判定，不循环重试；真实 ZIP 版本不回归。

## P0-3：渠道确定性断链

最小修复：

- 企微在 connect/test/auto-connect 前不要求 channel keyring token，改查 wecom-cli auth。
- 创建 enabled 渠道保存凭据后立即 connect；失败返回“已保存但连接失败”。
- disabled -> enabled 调 reconcile/connect；enabled -> disabled 幂等 disconnect。
- edit 和微信 auth 使用后端 merge patch，保留 `channel_workspace_root`、default agent 和未知字段。
- 所有修复复用一个最小 helper，避免 create/toggle/auth 再次分叉。

验收：三类渠道最小配置测试；企微无 channel token 可连接；编辑前后内部字段完全相同。

## P0-4：记忆错误写入与 marker 破坏

最小修复：

- 删除提示中的 `C:/Users/Administrator/...` shell fallback；MCP 失败明确返回不可用，不指示模型写文件。
- 设置页展示内容与持久原文分离；保存时保留 entry marker，或暂时禁止会丢 marker 的整文保存。
- 前端至少透传后端 availability/companion/candidate diagnostic，不能把故障显示成“active”。
- 不在本任务新增自动 harvest；交 Task 09。

验收：非 Administrator 用户不会产生错误路径写入；编辑一个条目后其他 marker 和 entry ID 不变；bridge down 能显示原因。

## 验证

- 对每个 P0 保存失败前/修复后 evidence 和缺陷 ID。
- `git diff --check` 与静态调用链审查。
- 当前机器不运行桌面 build/test/start；远端 CI 运行定向测试和三渠道 smoke。
- secret 检查只报告路径和类型，不在日志/报告回显凭据。

## 完成定义

- 已知凭据已轮换且源码无值。
- dirty Skill 不会被自动覆盖。
- 错误 Skill 版本不再反复消耗流量并误导用户。
- 新建/启用渠道的已确认断链修复。
- 记忆不再写硬编码用户路径或因 UI 保存清除 marker。
- 所有完整重构项均移交后续 owner，没有在热修中发明临时平行架构。
