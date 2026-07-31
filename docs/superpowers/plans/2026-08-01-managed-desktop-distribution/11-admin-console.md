# Task 11：Fusion 后台设置与任务控制台

## 目标

重构 `iyw-fusion-api/docs/admin` 的管理信息架构，让管理员不用手写 JSON 即可管理 Skill、Agent、工具、PC 版本策略、镜像任务、强制规则和回滚。

## 依赖

- Task 02/03/05 的 admin API 和 DTO 已冻结。
- 只消费 API，不在静态 JS 中复制领域规则。

## scope_write

- `iyw-fusion-api/docs/admin/` HTML/CSS/JS/assets
- 管理页专用说明文档

## 禁止修改

- Go handler/domain/application、SQL、router、根页面服务逻辑、第三方前端依赖。
- 使用 JS Number 解析或比较 Snowflake ID。

## 信息架构

左侧一级导航：

- 总览
- PC 应用版本
- Agent 平台
- SDK/CLI/基础工具
- Skill
- 分发策略
- 后台任务
- 客户端事件
- 审计日志

保留当前轻量静态管理页技术，不因 UI 优化引入大型框架。建立共享的 request、error、pagination、dialog、toast 和 ID helper，减少各页复制。

## 总览

展示可操作指标：

- 当前 stable/beta PC 推荐版本。
- 被阻断/最低安全 Agent 和工具。
- Skill artifact pending/failed 数。
- job queue depth、oldest wait、running、dead。
- 最近 24h 下载/激活/回滚失败率。
- TOS 巡检异常和 catalog revision。

异常项可点击进入已筛选列表，不做装饰性大卡片。

## Skill 管理

- 筛选 audience：global/organization/owner private；publisher 和 distribution 独立筛选。
- 详情显示 org、owner、版本、raw/artifact size、sha、artifact status、build job。
- 上传完成后显示“构建中”，任务成功才允许发布。
- 强制策略用版本/PC/channel/target/arch/org 选择器配置。
- 发布前预览可见用户范围、依赖闭包和受影响客户端。
- 不再让“公开/私有”承担全部语义。

## Agent 与工具

- 平台/工具、版本、artifact、policy 分层展示。
- 创建 draft、上传/镜像、验证、发布、暂停、撤回、security block、recommended/minimum-safe。
- recipe component 只从后端 allowlist 下拉选择；不能手写命令、包名或 URL。
- 兼容矩阵用 target/arch/client range 表格；冲突即时显示服务器 validation。
- 版本策略变更前显示会升级、保持、阻断和可能回滚的估算。

## 后台任务

- 表格列：ID（字符串）、type、status、progress、attempt、scheduled、lease、duration、last error。
- 详情时间线展示 event/checkpoint 脱敏摘要。
- retry/cancel 操作有二次确认和 reason；stale running 与正常 running 明确区分。
- dead letter 支持按 type 批量选择，但批量重跑需限制数量并预览。
- 自动刷新可暂停；页面不可见时停止轮询。

## 简化配置

- 二元设置用 Switch/checkbox，模式用 segmented control，范围用 select/slider/input。
- 版本字段带 SemVer validation；时间显示本地时区并提交 UTC。
- object key、签名、完整 URL 等内部字段默认不显示。
- JSON 仅作为只读高级诊断，不作为正常编辑入口。
- destructive command 使用明确动作名和影响对象，不用模糊“确定”。

## 安全

- admin token 只保存在当前 session memory；默认不写 localStorage。
- 401/403 清理会话并回登录，不无限重试。
- 所有 HTML 使用 escape helper；禁止把 API 字符串拼进未转义 innerHTML。
- 下载票据不写日志、DOM dataset 或持久缓存。
- 审计显示 actor/operation/target/result，不显示密钥和完整 payload。

## 性能与可用性

- 列表 server-side 分页/筛选，页大小有上限。
- 防止重复提交；mutation 时相关按钮禁用并显示进度。
- 自动刷新使用 ETag/revision，避免全页重渲染。
- 长表格 sticky header、稳定列宽、横向滚动仅在真正矩阵页面出现。
- 移动端至少能完成紧急暂停/回滚/任务重跑；复杂编辑可提示使用桌面宽屏。

## 验证

- fixture 覆盖空、loading、error、partial、large data、long Chinese/English、Snowflake 最大值。
- Playwright 覆盖登录、Skill 发布、策略预览、artifact failed、任务 retry/cancel、Agent block/rollback。
- 检查所有 ID 不经过 Number/parseInt，一律字符串。
- 进行键盘、focus、对比度、错误提示和响应式截图检查。

## 完成定义

- 日常管理不需要手写 JSON。
- 版本、策略、任务和审计之间可顺畅跳转。
- 高风险动作有影响预览、确认、结果和审计。
