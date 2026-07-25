# 记忆工具可靠性设计

日期：2026-07-24

状态：已实现，待合入

## 目标

消除 Agent 调用 `append_user_memory` / `propose_user_memory` 时因 MCP 工具未注册、
前缀不匹配、伴生进程版本漂移或启动竞态导致的 `unsupported call`。记忆仍由
iyw-claw 的 `UserMemoryService` 独占，不能安装或同步第二套记忆系统。

## 现状依据

- `f9d5a96` 已将 Skill 改为按 MCP 工具后缀匹配，但模型仍可能在工具列表未确认时调用。
- `7398f43` 已增加 companion `tools/list` readiness，但版本判断仍使用完整 package version。
- `4fe0bd8` 已处理 Windows Skill 运行时占用，不能再引入会话期间覆盖 Skill 的写入竞态。
- 宿主已有文件锁、跨资源事务、候选状态和 UI 直写入口，继续复用这些能力。

## 设计

### 1. 会话级工具路由

每个 MCP companion token 记录协议版本和实际 `tools/list` 工具集合。协议版本覆盖 wire
结构与工具 schema 的兼容承诺，package version 不再代替协议版本。
记忆能力只有在当前 token 已完成 readiness、工具集合包含对应工具、宿主 bridge 可用、
存储策略允许写入时才标记为可用。全局 companion 探测只负责选择候选二进制，不能单独
授权当前会话。

### 2. 稳定兼容性

wire protocol 使用独立的 `protocol_version` 和工具 schema 集合校验；package version
只用于诊断。旧 companion 若协议和所需工具兼容，可以继续工作；协议或 handler 不兼容时
应返回结构化的不可用结果，而不是让宿主产生裸 `unsupported call`。

### 3. Skill 路由规则

Skill 按以下顺序选择入口：当前工具列表中的完整 `iyw-claw-mcp` 名称、唯一后缀匹配、
已声明 schema 的原生 memory 入口。没有唯一且已列出的工具时不得调用裸名称、不得使用
shell 或直接编辑记忆文件。工具不可用时必须说明“本次未改变持久记忆”。

### 4. 托底与幂等

MCP 调用失败不能静默丢失。工具返回结构化错误并指向聊天消息旁现有的宿主 Memory
操作；该入口绕过 MCP，但仍调用同一个 `UserMemoryService`。自动解析助手文本作为隐藏
写入通道会扩大提示注入面，因此不采用。短暂 transport failure 只使用完全相同的请求
重试一次；确认记忆按内容 ID 去重，候选观察按 source/turn key 去重。

### 5. 可观测性

每次记忆路由记录 route、session、agent、工具集合摘要、协议版本、结果状态和错误码，
不得记录 token、密钥或完整用户内容。诊断接口应能区分 storage、policy、bridge、
companion、tool-list 和 transport 失败。

## 验收标准

1. 只有当前会话真实收到并确认的工具才会出现在记忆能力和上下文指引中。
2. 旧版本/缺工具/未 readiness 的 companion 不会让模型看到一个不可调用工具。
3. MCP 工具名不匹配时不会调用裸名称，也不会产生宿主 `unsupported call`。
4. MCP 传输失败会返回可诊断结果和宿主托底动作，重复重试不会产生重复记忆。
5. 现有 memory 文件、候选审核、用户 UI 和隐私规则保持不变。
