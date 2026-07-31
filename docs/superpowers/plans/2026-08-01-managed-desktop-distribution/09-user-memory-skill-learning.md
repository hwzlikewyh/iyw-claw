# Task 09：用户记忆恢复与 Skill 学习协作

## 目标

恢复用户记忆持续更新，建立 TurnComplete 后的可靠候选采集、可观察状态和用户确认闭环；明确 `self-improving` Skill 只产生候选，桌面 memory service 统一持久化。

## 已确认问题

- 当前只有模型主动调用 `append_user_memory`/`propose_user_memory` 才更新，没有每轮采集任务。
- 相同候选按精确规范化摘要累积，措辞变化会拆成多个 tentative。
- memory guidance 硬编码 fallback 到 `C:/Users/Administrator/.iyw-claw/user-memory.md`。
- 设置页 TS snapshot 丢弃 availability、migration、candidate、capability 和 companion health 诊断。
- 设置页 `sanitizeMemoryContent` 清除 HTML marker 和来源文本，保存后可能破坏 entry ID 去重/纠正。
- 用户看不到最近更新时间、最近失败或队列积压。

## scope_write

- `src-tauri/src/user_memory/`
- `src-tauri/src/acp/memory_turn.rs`
- 记忆相关的 delegation companion/listener 路由
- `src-tauri/src/commands/user_memory.rs`、web handler
- `src/components/settings/user-memory-*`、`src/lib/user-memory-*`、消息记忆动作
- `src-tauri/experts/skills/self-improving/` 中记忆协议文件；注意现有用户脏改动，逐行合并，不覆盖

## 共享文件限制

- `acp/connection.rs`、`manager.rs`、session subscriber、`lib.rs` 的接线由 Task 13 修改。
- 本任务提供 hook/service API 和 integration request。

## 先做运行诊断

在修改前对用户环境收集脱敏 snapshot：

- resolved root/source、三个文档是否存在/可读/可写/etag。
- policy enabled、agent write、per-agent、inherit。
- candidate file diagnostic/count/revision。
- companion expected/detected version、advertised tools、host bridge。
- 最近会话 capability snapshot 和是否实际暴露工具。
- 最近 memory tool call/result 日志。

不能读取并上传记忆正文；只记录大小、摘要和状态。用诊断确定用户的实际阻断原因，再与静态缺陷一起修复。

## 移除错误 fallback

- 删除让模型调用 shell 写硬编码路径的 guidance。
- MCP 路由失败时由 host bridge 返回稳定错误和重试建议；用户明确确认可由 UI 调 host service。
- 任何路径来自 `ResolvedUserMemoryRoot`，不从 prompt 或 Agent 参数输入。
- 已误写 Administrator 路径的数据只做可选迁移检测，需用户确认合并，不自动读取他人目录。

## 采集队列

TurnComplete hook 只提交小型 `MemoryHarvestRequest`：conversation、turn nonce、agent、脱敏/限长的用户与 assistant 语义输入引用、stop reason。实际抽取异步执行，不能阻塞 UI 完成事件。

状态：`queued/extracting/proposed/noop/failed/dead`。要求：

- 有界队列与并发。
- 持久 checkpoint，应用重启可恢复。
- conversation + turn nonce 唯一，防重复。
- 失败分类和有界退避。
- stop reason 非正常、内容过短、只有工具噪音时 noop。
- 不采集密钥、token、健康/财务等敏感推断、仓库临时事实或一次性进度。

## 候选归一与合并

- 先规则规范化，再使用受控语义相似判定；原始内容不外发到未知服务。
- 保存 canonical content、source observations、置信度、冲突关系和最后观察时间。
- 精确重复增加 observation；高相似进入同一候选并保留表述差异。
- 与已确认 memory 冲突时进入 correction candidate，不自动覆盖。
- 达到阈值仅标记 `pending_confirmation`，除非用户在当前对话明确确认，不能静默 append。
- 候选容量按状态清理：先清 terminal oldest，不因达到上限让整个学习永久停止。

## self-improving Skill 边界

Skill 输出结构化 proposal：content、signal、reason、scope、sensitivity flag；不得直接编辑 memory 文件。

- Skill 负责反思、总结可复用偏好和提出候选。
- host 负责身份、turn authorization、风险过滤、去重、事务、审计和最终写入。
- Agent 调用失败不能回退 shell。
- Skill 版本升级不得迁移/覆盖用户 memory。

## UI

完整映射后端 snapshot，不丢字段。页面显示：

- 总开关、Agent 写入、继承和 per-agent。
- resolved root/source、availability、只读/迁移错误。
- companion health、工具暴露和 capability reason。
- 最近采集、最近成功写入、最近失败、积压和处理延迟。
- candidate 分组：tentative/emerging/待确认/confirmed/rejected/superseded。
- confirm/edit/reject/merge/supersede，均带 expected revision。
- 手动“重新扫描未处理回合”和“重建候选索引”，操作前预览范围。

编辑器显示层可以隐藏 marker，但保存必须基于结构化 entry 或原始文本映射，绝不能把隐藏 marker 从持久文件删除。

## 生命周期与会话

- 新会话注入当前 memory snapshot 和工具指导。
- 旧会话保持旧读取代际，但可写权限按 launch token；安全关闭可立即拒绝写。
- 仅 proposal 可用时恢复会话也应重新注入工具指导。
- Turn tracker 在 accepted prompt 开始、TurnComplete/terminal/disconnect 结束；迟到 proposal 必须拒绝且记录 reason。

## 测试矩阵

- 工具可用/缺失/version mismatch/host bridge down。
- root 不存在、只读、marker 被隐藏编辑、transaction recovery。
- 每轮唯一、重复 TurnComplete、应用中断、队列重试。
- 相同/近似/冲突/敏感/临时信息候选。
- 候选达上限后清理并继续学习。
- explicit confirm 同步写；未确认不进入最终 memory。
- subagent inherit on/off、proposal-only resumed session。

## 验证

- 远端 CI 运行 user_memory 领域/事务/candidate/bridge 定向测试。
- 目标环境连续完成多轮对话，看到 harvest 时间推进、候选产生、确认后 memory 更新，并在新会话生效。
- 审计日志不含记忆正文或敏感数据。
- 桌面本机不运行编译/测试，只做静态调用链审查。

## 完成定义

- 用户不依赖模型偶然调用工具也能形成可审核候选。
- 记忆失败原因可见、可重试、不会因一个坏文件永久停止。
- Skill 与 host 的职责单一，不存在直接文件写入旁路。
