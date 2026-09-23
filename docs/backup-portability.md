# 备份与换机恢复

## 验收范围

- 导出固定包含会话记录，不显示“包含会话内容”选项。
- 新备份包含原生智能体记录、宿主 ACP 历史、聊天目录、会话附件和托管产物。
- 恢复使用目标机器的存储配置，重定位托管目录；会话文件失败不能静默报告成功。
- 加密口令可重试，上传和恢复阶段可见，处理中禁止重复操作。

## 实现边界

沿用 ZIP 清单校验、暂存、重启应用的现有流程。新归档使用格式版本 2，旧客户端拒绝
读取，防止忽略新增数据后误报成功。新客户端兼容格式版本 1。迁移信息作为私有的
`backup-paths.json` 条目保存并参与校验，不更改 API 字段、数据库 schema 或依赖。
只调整暂存数据库，不修改正在运行的数据库。
原生会话文件在重启时写入目标目录；已有文件默认保留，覆盖必须明确选择。
覆盖前将原生会话文件保存到目标数据目录的 `.iyw-claw-restore-backup/external-*`。
写入使用同目录临时文件与原子发布，失败保留 pending marker 和暂存副本供重试。

## 数据范围

| 内容 | 处理方式 |
| --- | --- |
| 应用数据库、上传文件、偏好、记忆 | 沿用现有快照和校验 |
| 原生会话 | 使用运行中配置的目录，增加星河归档会话及标题索引 |
| 原生 SQLite 会话库 | `VACUUM INTO` 一致性快照，包含已提交 WAL |
| 宿主 ACP 历史 | 完整备份 `acp-transcripts` |
| 普通聊天工作目录与附件 | 备份 `chat-sessions`、`conversation-attachments` |
| 托管产物 | 备份 `task-artifacts`，重定位其数据库路径 |
| 本机运行环境配置 | 保留目标机器的存储根目录、profile overrides 和安装根目录记录 |

恢复时仅重定位已知托管根目录下的结构化路径字段、文件 URI、流光项目路径索引，
不做会话正文全文替换；历史消息中的普通文本路径保留原文。清理旧的历史分页缓存，
避免相同会话 ID 命中旧机器内容。运行中的原生数据库有 WAL/SHM 时禁止覆盖。

旧备份能恢复其实际包含的数据，缺失的会话正文与附件不能补回。
普通项目源码不在备份内；继续项目会话需要另行迁移项目目录。
系统钥匙串中的登录凭据与智能体运行时不随备份迁移，目标机器必须完成环境配置。

## 会话续接依据

应用以会话 ID 读取原生记录，而非数据库保存完整对话正文。远山上游适配器的
`readResumedSession` 在所有项目目录搜索会话，`getOrCreateSession` 同时传入
原会话 ID 和目标工作目录。星河适配器使用 `thread/resume`，会话目录由目标机器
的 profile 环境变量确定。宿主 ACP 自存历史仍使用原会话 ID。

- 上游来源：https://github.com/zed-industries/claude-agent-acp/blob/main/src/resumed-session.ts
- 上游来源：https://github.com/zed-industries/claude-agent-acp/blob/main/src/acp-agent.ts
- 本地入口：`harness/codex/src/acp_agent/acp_mapping.rs`
- 历史入口：`src-tauri/src/commands/conversations.rs`、`src-tauri/src/parsers/factory.rs`

## 验证记录

已通过前端 TypeScript 检查、定向 ESLint、Rust 语法格式化和 `git diff --check`。
按仓库约束没有运行桌面构建、Rust 编译或自动化测试，不以静态检查代替双机恢复实测。
真实验收仍需在两台已配置运行环境的机器上：导出包含完成对话和附件的新备份，恢复并
重启，核对消息与附件，再向同一会话发送新消息。应覆盖口令错误、目标目录不同、已有
会话库冲突、磁盘写入失败及中断后重试。不同智能体版本的原生恢复兼容性需分别验证。
