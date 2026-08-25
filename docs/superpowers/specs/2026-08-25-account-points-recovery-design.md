# 402 点数恢复设计

## 背景

Gateway 返回 402/`insufficient_points` 时，当前 ACP 错误展示会结束本轮回复，但不会刷新全局账号点数。充值后，`IywAccountProvider` 仍可能保留旧的 `balance_points`，发送前门禁也可能继续阻止用户提交。

## 目标与非目标

目标：

- 仅针对明确的点数不足错误触发一次全局账号 profile 刷新。
- 保留失败前的用户输入，不自动重新提交可能已经产生副作用的请求。
- 刷新到正余额后，用户可通过明确的“重试”动作恢复发送。
- 刷新失败时保留原错误和输入，不清空已有账号状态。

非目标：

- 不对网络错误、限流、模型不可用等其他错误自动刷新点数。
- 不在没有幂等凭据时自动重放请求。
- 不改变 Gateway 或账户服务接口。

## 设计

### 事件与状态流

1. `AcpConnectionsProvider` 收到稳定错误码 `insufficient_points`，或从 402 响应分类为余额不足。
2. 它调用上层 `IywAccountProvider` 暴露的 `refreshProfile`，请求实时 `iyw_account_get_profile`。
3. 当前发送输入保持在现有草稿/恢复队列中；错误提示增加“点数已刷新后可重试”的动作入口。
4. 刷新成功且 `balance_points > 0` 时，账户 Context 更新，发送前门禁自然解除；用户点击重试后才重新提交。
5. 刷新失败时不清除最后一次成功的 profile，不自动重试原请求，并保留可重试输入。

### 组件边界

- `IywAccountProvider` 继续作为唯一 profile/points 状态源，并提供稳定的刷新回调。
- `AcpConnectionsProvider` 只负责识别点数错误、发起刷新和向聊天错误状态传递恢复信息，不复制账户状态。
- `ConversationDetailPanel`/输入组件复用现有草稿或持久队列，新增的重试动作必须经过同一发送入口和点数门禁。
- 账户面板提供已登录状态下的手动刷新入口，便于充值后的主动恢复。

### 安全约束

- 单个错误事件只触发一次刷新；复用 Context 现有 in-flight 请求去重。
- 刷新响应受现有 generation/mounted guard 保护，旧登录会话不能覆盖新状态。
- 不在 402 回调里直接调用发送函数，避免重复扣费或重复创建会话 turn。

## 验证

- Context 测试：402 触发刷新、刷新成功更新余额、刷新失败保留旧 profile。
- 发送流程测试：失败输入仍可见；只有用户点击重试才提交；余额为 0 时仍受门禁保护。
- 静态检查：定向 TypeScript/ESLint、`git diff --check`；按仓库规则不默认运行完整测试或桌面构建。
