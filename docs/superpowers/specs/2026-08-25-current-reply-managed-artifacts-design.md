# 自动托管成果归属到当前回复设计

日期：2026-08-25

## 背景

当前 `task_artifact` 已能在回合结束时扫描受管 turn 目录并写入当前会话成果区，
但消息下方的 `CurrentReplyArtifacts` 只解析 assistant 消息内显式的
`present_task_files` tool-call。自动托管交付没有对应的消息引用，因此文件虽然已经
登记成功，却不会显示在产生它的 assistant 回复下方。

## 目标

- 自动托管目录中的最终文件同时进入当前会话成果区和对应 assistant 回复下方。
- 保留 `present_task_files` 的严格本轮归属语义，不使用最近时间窗口或会话累计总数推断。
- 新回合没有成果时，不把上一回合成果显示到新回复下方。
- 页面刷新、重新打开会话和桌面重启后仍能恢复归属。
- 保持文件预览、复制、打开、缺失/不可访问状态和现有全量成果浏览行为。

## 非目标

- 不修改 Agent 原始 transcript，不伪造 `present_task_files` tool-call。
- 不改变当前会话 lineage 的“当前会话 + 祖先/后代”全量查询语义。
- 不把源文件、测试、缓存、日志或临时截图自动注册为成果。
- 不引入按时间范围猜测成果归属的前端回退。

## 方案

### 数据归属

在 `conversation` 增加 `last_completed_turn_generation`（默认 `0`），在每次正常回合
完成时更新，即使该回合没有文件；在 `task_artifact` 增加可空 `turn_generation` 字段。
自动托管交付和显式 `present_task_files` 都写入当前 host turn generation；历史旧记录
保持 `NULL`，继续在当前会话成果区可见，但不被自动归属到某一条回复。

`TaskArtifactAccess` 接口增加可选 turn generation。显式工具通过已有
`ParentSessionLookup::current_turn_generation` 动态读取，避免 MCP lease 创建时会话尚未
绑定导致快照为空；自动托管交付直接使用 `CompletedTurnDelivery.turn_generation`。

### 查询接口

保留现有 `list_task_artifacts` 的 current/all 行为，增加可选 `latestTurnOnly` 参数。
该参数只用于当前回复面板：解析当前会话 lineage 内每个会话的
`last_completed_turn_generation`，只返回各自最后完成 generation 的成果。最新回合没有
文件时直接返回空数组，不会回退到上一回合；没有 generation 的旧记录也不进入该面板。
这样不会丢失委派子会话成果，也不会混入兄弟会话。全量成果区不传该参数，因此不受影响。

### 前端行为

`CurrentReplyArtifacts` 不再要求消息 parts 中必须存在 `present_task_files` 引用。
它使用 `latestTurnOnly` 查询当前会话最新已登记 turn 的成果；仍由
`MessageListView` 只挂载在最后一条已持久化 assistant 回复下方。若查询为空则不渲染。

显式 `present_task_files` 的 parts 解析保留为兼容和防串线校验：当消息包含已接受引用
时，只展示引用匹配项；自动托管回复没有引用时，展示 latest turn 查询结果。明确失败或
零接受结果仍不触发输入路径回退。

## 错误与并发

- 回合完成 generation 的更新与成果扫描分开：即使成果目录为空也要推进 generation，
  防止上一回合成果误挂；更新失败记录错误并保持现有交付错误路径。
- 成果写入失败不发 accepted 刷新事件；沿用现有错误日志和错误事件。
- 同一 conversation + path 的重复登记继续 upsert；turn generation 更新为本次登记的
  generation，避免同一最终文件停留在旧回合。
- 列表查询没有可用 generation 时返回空当前回复成果，不影响全量成果列表。
- `task-artifact://changed` 仍在至少一项 accepted 时发出，前端按现有 debounce 重新查询。
- 旧版本数据库通过 migration 增加 nullable 列，不需要数据回填。

## 涉及文件

- Rust DB entity/migration/service、task artifact command、ACP listener 和自动交付。
- TypeScript API、task-artifact hook、当前回复成果组件。
- 不修改 unrelated 的 ACP 连接恢复、输入队列或模型 contract。

## 验证

1. 静态审查登记路径：显式工具和自动托管都传入 turn generation。
2. JSON/TOML 不涉及；运行 `git diff --check`、目标 Rust `rustfmt --check`、目标前端
   Prettier/ESLint 和 TypeScript no-emit（遵循仓库不默认运行桌面构建/测试的规则）。
3. 使用现有运行数据库只读核对：最新会话的 PPT/PDF 记录带 generation，当前回复查询只
   返回最新 generation，全量查询仍返回全部记录。
4. 静态回归矩阵：旧 `present_task_files`、自动交付、零成果、新回合、重复路径、旧
   NULL 记录、祖先/后代 lineage。
