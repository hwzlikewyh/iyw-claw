# 星河恢复与模型切换修复

## 验收与范围

- 恢复同一个 session ID，保留已有对话、权限、MCP 和工作目录。
- 模型切换收到原生设置应用通知后才确认；随后发送的消息等待确认。
- 已知 SQL 换行校验和差异不阻塞状态库，其他初始化错误在启动阶段明确报告。
- 同步核验 Codex 最新正式版与本项目锁定提交。

## 日志结论

2026-09-14 的故障日志包含 `state_5.sqlite` 的 migration 1 校验和不一致、
`list_turns is not supported yet`、`sqlite state db unavailable for thread goals`
以及标题生成中的未知 `token_budget` 配置字段。

0.154.0 的本地分页历史读取要求可用的状态库和线程元数据；状态库初始化
失败后继续启动会让恢复、目标操作和取消发生延迟错误。日志没有数据库中的
实际校验和值，因此尚不能确认故障机的差异仅来自 LF/CRLF。

代码中的另一个问题是把 `thread/settings/update` 的空响应当作设置生效。
上游只确认操作入队，实际生效通过 `thread/settings/updated` 通知返回。

## 实现

- `codex-state` 沿用锁定上游生产源码与全部迁移；测试专用项未引入。
- SQL 使用 LF。启动时只为匹配相同 SQL 的 LF/CRLF 校验和调整内存中的
  migrator。原库、迁移记录、校验门禁和迁移锁均保留，不自动删库或重建。
- worker 使用上游 fallible state initializer，保留最深层错误并进行脱敏。
- ACP 设置请求等待相同线程、相同目标值的原生通知，等待上限 15 秒，
  小于宿主 20 秒超时。快速发送的消息至多暂存一条，失败或关闭时结束等待，
  取消时不发送暂存消息。Fast tier 同时识别原生 `priority` 和兼容值 `fast`。
- 标题配置改用 `features.token_budget` 的结构化设置。

## 上游核验

2026-09-14 查询官方 releases/latest，最新正式版仍为 `rust-v0.154.0`，
发布日期为 2026-09-09；更新的 0.155.0-alpha 属于预发行版本。
项目同步脚本核验 tag object `36eab01061df3cde5f95ec20a526777b430091ba`
和提交 `6b9826e3aa83b1a5947db50f4332cb9c65f1b340` 均一致。

来源：https://github.com/openai/codex/releases/tag/rust-v0.154.0

官方文档网站返回 403，本次按项目锁定的官方源码核对配置和协议语义。
没有更换默认模型、引入 alpha 或移动分支依赖。

## 验证与发布

Windows x86_64 worker 的 `cargo check --lib --locked --offline` 已通过。
按仓库 AGENTS.md 要求未新增或运行测试。静态审查覆盖设置请求、通知确认、
超时、取消、关闭和状态库初始化，迁移 SQL 与官方源码逐文件核对。

交付的是源码修复，故障机需要安装包含新 worker 的应用构建才能生效。
尚未验证故障机数据库、登录后的恢复/切模流程或签名安装包。
若仍报迁移校验和冲突，需备份后取得故障库的迁移校验信息，继续比对；
不应直接删除 `state_5.sqlite` 或修改 `_sqlx_migrations`。

`iyw-image-workflows` 已在此前提交中退出源码捆绑。现有 retired 列表是
升级清理用途，应保留以避免旧安装残留；本次未发现新的受 Git 跟踪删除。
