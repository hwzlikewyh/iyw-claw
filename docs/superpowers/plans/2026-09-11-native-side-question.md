# 原生顺便问接入实施计划

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** 在独立临时侧栏接入远山原生 side_question 与星河原生临时侧线程，主任务不停止、不重绑定、不入主消息队列。

**Architecture:** 输入框在普通发送前识别 `/btw`；通过一个受会话身份约束的 ACP 扩展请求访问原生执行器。远山使用当前 Query 的原生控制方法；星河内部工作器执行上游侧会话的 fork、历史边界注入和 turn/start。宿主和 UI 不复制主对话上下文，不调用替代模型 API。

**Tech Stack:** React/TypeScript、Rust/Tokio、ACP、受管 Node.js、官方 Agent SDK 与锁定的 App Server。

## Global Constraints

- 用户要求从刚获取的远程主分支建立功能分支。
- 实施基线：origin/main `1935f6925cc1b03b75d17d33ff666d8562d50ea7`。
- 分支：`feat/native-side-question-main-20260911`；使用独立 worktree。
- 不覆盖原工作区、不复制其未提交文件、不推送、不合回主分支；允许将远程主线同步进本功能分支。
- 不运行或新增测试；不编译、打包或启动桌面端；执行静态调用链审查与 `git diff --check`。
- 不修改不可变受管 npm 安装目录；应用只提供自己的薄协议引导层。
- 远山适配兼容已核验的 ACP 0.73.0 / SDK 0.3.257；未公开方法缺失必须明确报不可用。
- 星河沿主分支已经使用的内部工作器接入，外部 ACP 未实现同一扩展时不得伪造支持或退回主消息。
- 问答不落入主消息历史；侧栏最多保留 20 条，关闭侧栏不取消，关闭会话页取消；取消必须匹配侧问请求 ID。
- 每个连接最多一个侧问执行；超时清理仅针对侧线程。
- 远山无工具；星河原生上下文与侧会话边界保留，工具可用性不得超越现有受控运行时的权限。
- 不记录问题、答案、模型凭据或完整协议载荷。

## Task 1: 独立 ACP 请求与宿主 API

**Files:**
- Create: `src-tauri/src/acp/side_question.rs`
- Modify: `src-tauri/src/acp/{mod.rs,connection.rs,manager.rs}`
- Modify: `src-tauri/src/commands/acp.rs`
- Modify: `src-tauri/src/web/{router.rs,handlers/acp.rs}`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
```text
ACP _iyw/side_question:
  {sessionId, action:"capabilities"|"ask"|"cancel", requestId?, question?}
  -> {supported?, mode?, response?, cancelled?, synthetic?}
Host acp_side_question:
  {connectionId, request:{sessionId, action, requestId?, question?}} -> same result
```

- [x] 定义固定 DTO、问题/ID 大小限制，拒绝无效 action 和跨会话请求。
- [x] 在空闲和主提示进行中的连接循环处理 SideQuestion command，使用独立异步任务回复，不持有主 prompt lock。
- [x] 桌面/Web 使用同一 manager 方法；请求接收与实际回答明确区分。
- [x] 连接/会话结束后释放侧请求；失败不执行普通发送或重试。

## Task 2: 远山原生 Query 桥接

**Files:**
- Create: `src-tauri/src/acp/side_question/claude-bootstrap.mjs`
- Modify: `src-tauri/src/acp/side_question.rs`
- Modify: `src-tauri/src/acp/connection.rs`

**Interfaces:** `_iyw/side_question` 调用所属 session.query.askSideQuestion(question, {signal})；cancel 使用独立 AbortController。

- [x] 引导层从已解析的受管安装树加载 SDK 与 runAcp，不启动第二个会话。
- [x] 对同安装树 AgentApp.connect 做一次性、有恢复保证的扩展注册；保留上游全部标准方法与取消处理。
- [x] 保留入口的 settings env 合并、stdout 协议纯净、dispose 和信号退出处理。
- [x] 仅以应用内容寻址的引导文件启动，不修改或复制第三方安装包。
- [x] 检测方法存在、禁止重复 request ID、限制并发、超时取消；始终保留失败原因且日志不含问题。

## Task 3: 星河原生侧线程

**Files:**
- Create: `harness/codex/src/acp_agent/side_question.rs`
- Create: `harness/codex/src/acp_agent/side_question_request.rs`
- Modify: `harness/codex/src/acp_agent.rs`
- Modify: `harness/codex/src/upstream_backend.rs`

**Interfaces:** 在 bridge command 入口截取 `_iyw/side_question`；新临时线程不替换主 session_id/pending_prompt。只接收当前侧线程的完成事件并返回独立响应。

- [x] 验证父会话绑定；从原生 thread/fork 获取上下文，设置 ephemeral/excludeTurns。
- [x] 注入上游侧会话边界与明确的工具/子代理约束，继承主模型，禁止恢复父任务。
- [x] 在主事件处理之前分流侧线程事件，既不吞主事件也不写主消息状态。
- [x] 独立启动、收集结果、取消、turn/interrupt 和 thread/unsubscribe；任何阶段失败均尝试清理。
- [x] 不把不受管线程的权限请求错误转发成主权限；未获受控授权的请求拒绝。

## Task 4: 独立临时侧栏与输入分流

**Files:**
- Create: `src/lib/side-question.ts`
- Create: `src/hooks/use-side-question.ts`
- Create: `src/components/chat/side-question-panel.tsx`
- Modify: `src/components/chat/{message-input.tsx,chat-input.tsx,conversation-shell.tsx}`
- Modify: `src/components/conversations/conversation-detail-panel.tsx`
- Modify: `src/lib/api.ts`
- Modify: `src/i18n/messages/*.json`

**Interfaces:** 两类智能体显示入口，capability 探测决定是否允许提问；`onSideQuestion(question)` 独立于 `onSend/onEnqueue`。

- [x] `/btw` 和 `/btw <question>` 在发送入队之前分流；非纯文字附件给出明确提示，不丢草稿。
- [x] 可发现的 slash 入口与侧栏按钮；只看当前绑定会话，跨会话迟到响应无效。
- [x] 20 条临时历史、展开/收起、单条取消、失败保留、复制到主草稿且不自动发送。
- [x] 桌面和 Web 共用 API；能力不支持时明确提示，不改发主会话。

## Task 5: 静态审查与交付

- [x] 审查主忙碌/空闲、重复点击、取消早于启动、连接关闭、跨页响应、超时清理、原生接口变化。
- [x] 只读子代理审查新分支 diff，修复阻断问题。
- [x] `git diff --check`，检查国际化键、命令注册和两种 transport 一致性。
- [x] 仅暂存本任务文件，提交功能分支；保留分支，不自动推送/合并。
- [x] 如实报告没有运行编译、测试或真实模型端到端验证；外部星河 ACP 能力限制单独说明。

## 交付核验记录（2026-09-11）

- 分支从实际 fetch 后的 `origin/main` 创建；实施前 HEAD 与远程跟踪分支均为 `1935f6925cc1b03b75d17d33ff666d8562d50ea7`。旧工作区及其未跟踪文件未参与实现。
- TypeScript 只读语义/类型分析：读取工作区源码，依赖解析映射到原工作区已有 node_modules；849 个非测试根文件，`noEmit=true`、`incremental=false`，最终 0 diagnostics。首次发现侧问状态字面量拓宽问题后修复，再检查通过；没有写编译产物或安装依赖。
- 12 个变更 Rust 文件通过 rustfmt 只读解析；仅格式化新文件，不执行 Cargo 编译、检查或测试。
- 应用自有 Node 引导脚本通过 `node --check`；未执行脚本启动 Agent。已核对同安装树模块导出、版本与原入口启动流程。
- 中英文 `SideQuestion` 20 个键及占位符一致；桌面命令、Web 路由、前端参数与超时窗口沿调用链核对。
- `git diff --check` 通过；无新增或修改测试文件、夹具、快照或构建输出。
- 后端只读审查确认：interrupt 失败不会因 unsubscribe 成功而解除隔离；迟到 fork/start 回收只影响子线程；侧线程不能依靠 fork 血缘混入主线程权限审批。
- UI 只读审查确认：欢迎页与普通页共用侧问入口；scope 变化清除旧注入；队列编辑期间禁用追加，并在编辑器消费处二次设防；历史队列 `/btw` 不会发给主任务；发送按钮不误触主取消。

### 适配边界与未运行验证

- 远山调用当前原生 Query 的 `askSideQuestion`，对应原生控制请求；没有重建前端消息上下文，也没有调用替代模型 API。该方法未出现在当前公开声明中，本桥接锁定已核验的受管 ACP 0.73.0（SDK 0.3.257）。版本不同会保留原入口并明确不支持扩展；不会修改第三方受管包。
- 星河沿内部工作器使用原生 `thread/fork(ephemeral)`、`thread/inject_items`、`turn/start`；主线程绑定保持不变。此轻量侧栏额外限制工具并检查只读 sandbox，不能泛称上游原生侧会话本身硬禁全部工具。
- 星河继承的是原生 fork 当时可用的父历史快照，不承诺包含正在输出的每个 token。外部星河 ACP 若未实现本扩展，将明确显示不可用，不回退主提示或通用模型接口。
- 侧问暂存最近 20 条，收起面板不停止；会话页卸载尝试独立取消。清理结果不确定时隔离该侧问通道并提示重连，不能将超时或已接收取消误报为清理完成。
- 没有运行测试、Cargo/桌面编译、打包、启动应用或真实模型请求。原生 SDK 响应、真实取消时序、UI/rAF 交互及桌面/Web 端到端仍需在允许的集成/发布环境验收。星河运行库必须与本分支应用一并构建交付，现有安装不会仅因改源码自动获得能力。
- 保留本地功能分支及 worktree；不推送、不合并，不清理用户的其他分支或工作区。
### 收尾主线同步

- 功能提交：`5bf86c15`（`feat(side-question): 接入双端原生顺便问侧栏`）。
- 收尾发现远程主线已前进，再次实际 fetch 成功，确认 `origin/main = c1498b81e6a4f8f0f66f3e9651498be073a9cdae`，新增主线变更仅涉及两个消息展示文件，与本功能修改文件不重叠。
- 通过 `baa3e6b1` 将该远程主线合入功能分支（不是把功能分支合回 main），没有冲突，没有改写历史；`git merge-base --is-ancestor origin/main HEAD` 成功。
- 合入前后已审查的侧问实现文件完全一致；同步后再次只读 TypeScript 分析，849 个非测试根文件仍为 0 diagnostics。最终功能差异的 `git diff --check` 通过。
- 未推送任何远程分支；保留独立 worktree。