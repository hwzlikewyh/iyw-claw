# ACP 安全自动续跑设计

## 背景

部分 ACP Agent 在已经执行了若干工具后，以普通 `end_turn` 结束，并在最终文本中只说“接下来执行”“现在用 GPU 转写”等未来动作，没有真正发出下一条工具调用。当前 Claw 会把这类回合视为正常完成，用户只能手动再次发送“继续”。

本设计只处理 PC 端用户直接会话。Chat Channel、自动化、delegation child、viewer 和其他后台入口不自动续跑。

## 目标与非目标

目标：

- 覆盖所有 ACP Agent，但只使用协议无关的 host-side 规则。
- 在正常 `end_turn` 后识别有限、可解释的未完成证据。
- 每个用户回合最多自动续跑一次。
- 不生成 Claw 可见的用户消息，不写入 Claw 会话展示历史。
- 续跑期间可被用户 Stop，且不与权限、问题、队列或断连状态冲突。
- 第二次仍未完成时显示可操作的“任务可能未完成”状态，不再循环。

非目标：

- 不调用额外模型做裁判。
- 不自动批准权限、删除、发布、推送、凭据输入或其他需要用户确认的动作。
- 不保证内部 continuation prompt 不出现在 Agent 自己的原始 transcript 中；标准 ACP 没有通用隐藏 prompt 类型。
- 不改变 Fusion 协议转换和上游重试语义。

## 方案选择

采用“host-side 混合判定 + 单次内部 continuation prompt”。

仅强化系统提示的方案成本低，但线上提示已经要求 Agent 持续完成任务，仍无法覆盖截图场景。额外模型评估虽然覆盖面更广，但增加费用、延迟、网络失败点和隐私边界，且结果不可稳定复现。规则判定可在本地测试、审计和限流，适合作为第一版安全门禁。

## 触发范围

只有满足以下条件才允许进入判定：

- 连接来自 PC 用户直接会话；以 `owner_window_label` 和连接来源排除 `chat_channel:`、automation、delegation child、viewer。
- 回合以 `TurnComplete(stop_reason="end_turn")` 结束。
- 回合没有 `cancelled`、transport error、terminal connection error、compaction 或启动恢复失败。
- 本回合观察到 Agent 输出，且当前 connection 没有权限、问题、channel confirmation、active tool、delegation、terminal、browser background work 或 durable input 等待。
- 当前用户 turn generation 尚未执行过自动续跑。

以下情况必须抑制自动续跑：明确完成、明确 blocker、向用户提问、等待授权或确认、敏感动作未授权、空回复、非正常 stop reason。

## 未完成判定

判定使用两个独立证据源，命中任一即可成为候选，随后经过抑制条件过滤：

### Plan 证据

最近一次有效 PlanUpdate 中仍有 `pending` 或 `in_progress` 项，并且本回合没有对应的完成/失败结论。Plan 只作为候选证据，不单独覆盖权限等待、用户提问和明确 blocker。

### 承诺文本证据

从本回合最后一段 assistant 文本提取 bounded、脱敏的尾部文本，仅识别明确的未来执行句式，例如“接下来执行”“现在运行”“让我修改后测试”“用 GPU 转写”。候选文本必须指向一个可执行动作，且该动作在承诺后没有对应工具调用或工具结果。

不使用任意“将来时”或普通建议句作为触发条件；无法确定时 fail-open，按普通完成处理。

判定结果保存为内部 `AutoContinuationEvidence`，包括 `reason_code`、证据类型和有限摘要，不保存完整回复。

## 状态与并发

每个 connection 的当前用户 turn 维护以下瞬态字段：

- `auto_continue_attempted`
- `auto_continue_in_flight`
- `auto_continue_source_generation`
- `auto_continue_reason`
- `auto_continue_outcome`

在 `TurnComplete` 处理路径中使用 generation compare-and-set：只有仍属于同一 source generation、且 claim 尚未被其他路径消费时，才能设置 `auto_continue_in_flight=true`。claim 成功后最多发送一个内部 prompt。

用户新 prompt、Stop、disconnect、replacement、restart claim、权限响应和 durable input dispatch 与 continuation claim 串行化。用户新输入优先：如果新输入先取得 prompt lock，自动续跑 claim 失效；如果 continuation 已取得 claim，Stop 可以取消它。

## 内部续跑

使用固定 host instruction，不拼接用户正文或敏感字段：

```text
继续完成当前用户请求。不要复述计划；执行尚未完成且已获授权的步骤。
如果需要新的用户授权、选择或信息，明确说明阻塞并停止。
```

内部 prompt 不走用户消息广播、不创建 optimistic message、不写入 Claw conversation user-message history、不增加用户消息计数。它仍可能被上游 Agent 写入自身 transcript，这是标准 ACP 的协议限制。

续跑 prompt 使用现有 ACP prompt 发送链路和 prompt lock，沿用图片、能力策略、上下文和错误处理，不创建平行发送通道。

## UI 状态

新增临时 connection 状态事件 `auto_continuation`，字段包括 source generation、attempt、reason code 和 phase。事件进入 live snapshot，但不持久化为用户消息。

第一阶段：

- 连接继续显示 prompting。
- 显示“检测到任务未完成，正在继续”。
- Stop 保持可用。

续跑成功完成：

- 恢复 connected/pending review 的现有流程。
- 不显示内部 prompt。

续跑后仍未完成或内部 prompt 入队失败：

- 恢复 connected。
- 显示“任务可能未完成”操作条。
- `继续` 是用户真实操作，发送普通 prompt 并进入历史。
- `停止` 仅关闭操作条，不再自动续跑。

页面刷新或 attach 时从 snapshot 恢复阶段状态；旧事件按 event sequence 去重，不能重复续跑。

## 错误与安全

- 判定器异常：记录 bounded error，fail-open，不阻断正常聊天。
- continuation 入队失败：不重试，进入未完成操作条。
- continuation 发生 transport/Host failure：沿用现有 terminal error/disconnect 语义，不改写为“未完成”。
- 用户 Stop：发送现有 Cancel，结束 source 或 continuation turn，不重新申请续跑。
- 任何权限、问题、确认或敏感动作等待：不自动续跑。
- 自动续跑计数按 source generation 限制为 1；连接恢复或页面重载不能清零该计数。

## 可观测性

新增结构化日志字段：

- `connection_id`
- `conversation_id`
- `source_generation`
- `attempt`
- `reason_code`
- `evidence_kind`
- `phase`
- `outcome`
- `elapsed_ms`

不记录完整 assistant 文本、内部 continuation prompt、工具参数、token、密钥或文件内容。现有 `[ACP] prompt started/completed` 继续区分用户 prompt 和内部 continuation，新增 `is_auto_continuation` 字段。

## 测试矩阵

后端单元测试：

- Plan pending 触发、Plan completed 不触发。
- 明确未来动作但没有工具调用触发。
- 普通建议、明确完成、阻塞、提问、权限等待不触发。
- cancelled、transport error、empty、compaction 不触发。
- 同一 generation 并发事件只有一个 claim。
- 第二次仍未完成只产生操作条状态，不产生第三次 prompt。
- 用户新 prompt/Stop 与 continuation claim 的竞态。
- 内部 prompt 不生成 UserMessage/UserPromptSent 或 Claw history 写入。

前端测试：

- 临时 auto_continuation 状态渲染。
- 刷新 snapshot 恢复状态且不重复续跑。
- 操作条的继续/停止动作。
- 内部 continuation 不出现在用户消息列表。

验证限制：遵循 iyw-claw 当前仓库规则，不在本机运行 Cargo/Tauri build/test；Rust 测试由 CI 执行，当前本机执行 rustfmt、定向 TypeScript、Prettier 和可运行的前端聚焦测试。

## rollout 与回退

第一版默认启用但仅限 PC 直接会话。通过结构化日志统计触发率、续跑成功率、二次未完成率和用户 Stop 率。若误续跑率异常，可通过运行时紧急开关全局关闭；关闭后恢复现有普通 `TurnComplete` 行为，不影响连接和历史。
