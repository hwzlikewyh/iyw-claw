# 记忆工具可靠性 Implementation Plan

> **For agentic workers:** Use `executing-plans` to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 让记忆工具只在当前会话真实可调用时暴露，并为 MCP 失效提供可诊断的宿主托底。

**Architecture:** 保留 `UserMemoryService` 为唯一事实源。将 companion readiness 扩展为会话级协议/工具集合能力，Skill 只消费当前会话真实工具列表；失败时指向现有宿主 Memory 操作，不新增数据库或隐藏文本通道。

**Tech Stack:** Rust 2021、Tokio、Serde、SeaORM/SQLite、Markdown Skill 文档、现有 ACP/MCP UDS bridge。

## Global Constraints

- 不新增第二套用户记忆数据库或直接编辑用户记忆文件。
- 不改变 M0-M4 分层、候选审核和敏感信息边界。
- 不接受裸记忆工具名，不把 package version 当作唯一 wire compatibility 条件。
- 新增行为先写回归测试并观察失败，再写生产代码。
- 不执行仓库单元/集成测试命令，除非用户明确要求；本次使用定向静态检查和编译级验证。

---

### Task 1: 固化 companion 能力校验

**Files:**
- Modify: `src-tauri/src/acp/delegation/transport.rs`
- Modify: `src-tauri/src/acp/delegation/listener.rs`
- Modify: `src-tauri/src/acp/companion_manifest.rs`
- Test: existing Rust unit tests in the same modules

**Interfaces:**
- `BrokerCompanionReadyRequest` carries protocol version and bounded tool names.
- `TokenRegistry::record_companion_ready` accepts only a compatible protocol and stores the exact tool set.

- [x] Add temporary checks for unsupported protocol, compatible package-version skew, and a missing required tool.
- [x] Confirm the package-version-skew check fails before the production change.
- [x] Implement bounded protocol/tool validation without changing token authentication.
- [x] Run the focused checks, then remove the temporary test code.

### Task 2: Make memory capability session-authoritative

**Files:**
- Modify: `src-tauri/src/user_memory/capabilities.rs`
- Modify: `src-tauri/src/user_memory/capability_types.rs`
- Modify: `src-tauri/src/acp/connection.rs`
- Test: `src-tauri/src/user_memory/tests.rs` and ACP module tests

**Interfaces:**
- `UserMemoryRuntimeEnvironment` supplies the verified per-session companion report.
- `compose_user_memory_capabilities` remains the single capability projection function.

- [x] Audit the existing per-session runtime projection and readiness timeout path.
- [x] Enforce every launch-authorized memory tool against the token's actual `tools/list`.
- [x] Carry the verified protocol/tool report into launch finalization diagnostics.
- [x] Preserve existing host-bridge, missing-tool, and incompatible-companion capability reasons.

### Task 3: Strengthen Skill and generated context instructions

**Files:**
- Modify: `src-tauri/experts/skills/self-improving/SKILL.md`
- Modify: `src-tauri/experts/skills/self-improving/operations.md`
- Modify: `src-tauri/src/user_memory/context.rs`
- Test: Skill validation and `src-tauri/src/user_memory/tests.rs`

**Interfaces:**
- Context rendering receives only capability-approved tools and emits exact route guidance.
- Skill keeps `append_user_memory` and `propose_user_memory` semantics unchanged.

- [x] Define unique full-name matching, ambiguity refusal, and no guessed bare-name calls.
- [x] Update the Skill and generated context with native/MCP routes and host fallback.
- [x] Run the bundled Skill validator and consistency checks.

### Task 4: Add structured failure and retry observability

**Files:**
- Modify: `src-tauri/src/acp/delegation/companion.rs`
- Modify: `src-tauri/src/acp/delegation/listener.rs`
- Modify: `src-tauri/src/acp/delegation/transport.rs`
- Test: companion dispatch and listener memory tests

**Interfaces:**
- Memory tool results expose stable `code`, `retryable`, and `durableChanged` fields.
- Retry replays the identical authenticated broker message at most once.

- [x] Add a temporary check for structured legacy-error normalization.
- [x] Implement stable error rendering and one identical-request retry.
- [x] Confirm append content IDs and proposal source/turn keys make retries idempotent.
- [x] Add route/result/error logs without token or full content.

### Task 5: Expose the host-memory fallback

**Files:**
- Modify: `src-tauri/src/acp/delegation/companion.rs`
- Modify: `src-tauri/src/acp/delegation/listener.rs`
- Modify: `src-tauri/experts/skills/self-improving/SKILL.md`
- Test: focused companion rendering checks

**Interfaces:**
- MCP errors carry `code`, `retryable`, `durableChanged`, and `fallback`.
- `fallback=host_memory_action` points to the existing per-message Memory button.

- [x] Add and run a temporary check for legacy error rendering, then remove it.
- [x] Normalize failure fields and user-visible fallback guidance.
- [x] Verify the fallback remains user-triggered and uses `append_user_memory_direct`.

### Task 6: Final static and build verification

**Files:**
- Review all changed files and existing memory docs.

- [x] Run targeted Rust formatting, whitespace checks, and Skill validation.
- [x] Run default-library and `mcp-runtime` compile checks.
- [x] Inspect the final route invariants, logs, temporary-test removal, diff, and worktree status.
