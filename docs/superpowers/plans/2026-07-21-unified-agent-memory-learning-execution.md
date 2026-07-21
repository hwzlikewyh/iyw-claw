# Unified Agent Memory and Conservative Learning Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use
> superpowers:subagent-driven-development (recommended) or
> superpowers:executing-plans to implement this plan task-by-task. Steps use
> checkbox (`- [ ]`) syntax for tracking.

**Goal:** Store desktop memory under the current user's `~/.iyw-claw` root,
give all eleven Agents one shared Memory/Profile/Soul context, and let the nine
safe MCP Agents append confirmed memory or submit conservative candidates.

**Architecture:** The Rust host remains the sole owner of policy, Markdown,
candidate JSON, migration, and transactions. Launch-time ACP integration turns
storage, policy, adapter transport, and companion health into three independent
capabilities; MCP tools carry authenticated requests back to the host. Tauri,
HTTP, backup/restore, and the settings UI consume the same service contracts.

**Tech Stack:** Rust 2021, Tokio, SeaORM/SQLite, Axum, Tauri 2, serde, Next.js
16 static export, React 19, strict TypeScript, Vitest, and Testing Library.

## Global Constraints

- Desktop resolution is `IYW_CLAW_USER_MEMORY_DIR`, then OS home joined with
  `.iyw-claw`. It never reads `IYW_CLAW_HOME` or `IYW_CLAW_DATA_DIR`.
- Server resolution is `IYW_CLAW_USER_MEMORY_DIR`, then `IYW_CLAW_HOME`,
  then the effective persistent data root.
- Failure to resolve a desktop home disables only memory; it never falls back
  to the current directory or installation directory.
- Prompt content remains the three canonical Markdown documents. Agents may
  append only to `user-memory.md`; Profile and Soul remain user-edited.
- Candidate state is `.user-memory-learning.json`: schema version 1, 1,000
  normalized characters per candidate, 500 records, and 10 retained source
  details. Observation counts may continue after details reach the cap.
- Observation counts move `tentative -> emerging -> pending_confirmation`
  but never confirm automatically. Terminal states are `confirmed`,
  `rejected`, and `superseded`.
- Provenance stores Agent type, hash-derived opaque source id, turn nonce,
  signal, and timestamp. Raw tokens, prompts, and transcripts are not persisted.
- `append_user_memory` retains Agent-visible input `{ content }`.
  `propose_user_memory` accepts only `{ content, signal }` with signal
  `correction`, `preference`, or `fact`.
- MCP features `memory` and `memory-proposal` are independent.
- Claude Code, Codex, OpenCode, Gemini, Cline, Hermes, CodeBuddy, Kimi Code,
  and Grok may expose both MCP tools. Pi and OpenClaw remain read-only.
- Read context, confirmed append, and candidate proposal each expose
  `available`, a stable reason code, and bounded degraded reason codes.
- Live sessions freeze their actual vector before becoming ready. Settings
  shows a projected vector for a new conversation.
- Multi-resource changes use a durable `prepared`/`committed` journal with
  previous and next generations. Prepared rolls back; committed rolls forward;
  mismatches fail closed and retain the journal.
- Invalid candidate JSON remains unchanged and disables proposals only.
  Confirmed Markdown append still works and skips reconciliation.
- Migration copies only missing valid legacy Markdown, never mutates a source,
  and saves a versioned per-file receipt after every pass.
- Backup includes candidate state below `user-memory/` and excludes receipt,
  lock, and journal. A Phase 1 archive restores empty candidate state.
- No heartbeat, output-marker parsing, transcript mining, direct Agent file
  editing, time-based demotion/deletion, or automatic Profile/Soul update.
- Tests created or changed for this implementation are temporary verification
  artifacts. Delete new test files and restore changed existing tests after
  capturing GREEN evidence; do not include them in commits unless the user
  separately authorizes a permanent regression test.
- All new behavior follows RED/GREEN/REFACTOR. A RED command must fail for the
  expected missing behavior before implementation begins.
- Keep new functions <= 50 lines, new files <= 300 lines, nesting <= 3,
  positional parameters <= 3, and cyclomatic complexity <= 10. Large existing
  ACP/frontend files receive wiring only.

---

### Task 1: Canonical Root Resolution and Unavailable Service

**Files:**

- Create: `src-tauri/tests/user_memory_paths.rs`
- Modify: `src-tauri/src/paths.rs`
- Modify: `src-tauri/src/desktop_bootstrap.rs`
- Modify: `src-tauri/src/user_memory/types.rs`
- Modify: `src-tauri/src/user_memory/service.rs`
- Modify: `src-tauri/src/user_memory/store.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/bin_targets/iyw_claw_server.rs`
- Modify: `src-tauri/src/db/mod.rs`
- Modify: `src-tauri/src/commands/backup/restore.rs`

**Interfaces:**

```rust
pub const USER_MEMORY_DIR_ENV: &str = "IYW_CLAW_USER_MEMORY_DIR";

pub enum UserMemoryRootSource {
    Override,
    DesktopHome,
    ServerHome,
    ServerData,
}

pub struct ResolvedUserMemoryRoot {
    pub path: PathBuf,
    pub source: UserMemoryRootSource,
}

pub fn resolve_desktop_user_memory_root(
    explicit: Option<&OsStr>,
    user_home: Option<&Path>,
) -> Result<ResolvedUserMemoryRoot, UserMemoryPathError>;

pub fn resolve_server_user_memory_root(
    explicit: Option<&OsStr>,
    legacy_home: Option<&OsStr>,
    data_root: &Path,
) -> Result<ResolvedUserMemoryRoot, UserMemoryPathError>;
```

- Existing tests retain `UserMemoryService::new(db, path)`. Production uses
  `UserMemoryService::from_resolution(db, result)`.
- `UserMemoryService::root()` returns
  `Result<&Path, AppCommandError>`; every caller propagates unavailability.
- `DesktopBootstrap` captures pre-bootstrap `IYW_CLAW_HOME` and install root
  only as migration inputs, never as the new desktop root.
- Restore receives the startup-resolved root and never re-reads path env vars.

- [x] **Step 1: Write resolver and unavailable-service tests**

Add table-driven tests named
`desktop_override_wins_and_is_absolutized`,
`desktop_home_ignores_legacy_home_and_data`,
`desktop_without_override_or_home_is_unavailable`,
`server_uses_override_home_then_data`,
`unavailable_service_never_creates_files_in_cwd`, and
`restore_uses_startup_resolved_root`. Pure resolver tests pass values directly
and do not mutate process-global environment variables.

- [x] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_paths
```

Expected: compilation fails because typed resolvers and unavailable service
interfaces do not exist.

- [x] **Step 3: Implement the minimal typed resolution**

Apply the exact priority rules, ignore empty overrides, absolutize once, store
source metadata, and map missing desktop home to stable reason
`user_memory_root_unavailable`. Route desktop/server construction, settings,
backup, and restore through the stored result.

- [x] **Step 4: Run GREEN and regressions**

```powershell
cargo test --features test-utils --test user_memory_paths
cargo test --features test-utils --test user_memory --test backup_user_memory
```

- [x] **Step 5: Commit**

```powershell
git add src-tauri/src/paths.rs src-tauri/src/desktop_bootstrap.rs src-tauri/src/user_memory src-tauri/src/lib.rs src-tauri/src/bin_targets/iyw_claw_server.rs src-tauri/src/db/mod.rs src-tauri/src/commands/backup/restore.rs
git commit -m "fix(memory): 固定用户记忆存储根目录"
```

### Task 2: Deterministic Legacy Markdown Migration

**Files:**

- Create: `src-tauri/src/user_memory/structured_file.rs`
- Create: `src-tauri/src/user_memory/migration.rs`
- Create: `src-tauri/tests/user_memory_migration.rs`
- Modify: `src-tauri/src/user_memory/mod.rs`
- Modify: `src-tauri/src/user_memory/types.rs`
- Modify: `src-tauri/src/user_memory/service.rs`
- Modify: `src-tauri/src/user_memory/fs.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**

- Add `UserMemoryMigrationSource`, `UserMemoryMigrationFileResult`,
  `UserMemoryMigrationReceipt`, and `UserMemoryMigrationReport`.
- Receipt is `.user-memory-migration.json` with `schemaVersion: 1`.
- `UserMemoryService::migrate_legacy_documents(sources)` runs under process
  and file locks before `snapshot_locked` creates empty documents.
- `structured_file` owns no-follow regular-file reads, bounded UTF-8/JSON
  reads, user-only creation where supported, atomic replace, file fsync, and
  directory fsync. Candidate and journal tasks reuse these primitives.

- [x] **Step 1: Write migration tests**

Use temp roots to cover first-valid precedence, missing-only copies, canonical
no-overwrite, duplicate-root skipping, conflict warnings, symlink/non-regular/
over-64-KiB/invalid-UTF-8 rejection, per-file partial failure, terminal receipt
outcomes, retryable I/O outcomes, deletion without resurrection, and no
candidate-state synthesis.

- [x] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_migration
```

Expected: compilation fails because migration types and methods are absent.

- [x] **Step 3: Implement structured files and migration**

Keep file decisions independent; preserve all sources. Write the receipt after
each pass even when some outcomes remain retryable. Expose report data for
settings diagnostics.

- [x] **Step 4: Run GREEN and storage regressions**

```powershell
cargo test --features test-utils --test user_memory_migration
cargo test --features test-utils --test user_memory
```

- [x] **Step 5: Commit**

```powershell
git add src-tauri/src/user_memory src-tauri/src/lib.rs
git commit -m "feat(memory): 迁移旧版用户记忆文档"
```

### Task 3: Candidate State, Normalization, and Lifecycle

**Files:**

- Create: `src-tauri/src/user_memory/candidate_types.rs`
- Create: `src-tauri/src/user_memory/candidate_store.rs`
- Create: `src-tauri/src/user_memory/candidate_lifecycle.rs`
- Create: `src-tauri/tests/user_memory_candidates.rs`
- Modify: `src-tauri/src/user_memory/mod.rs`
- Modify: `src-tauri/src/user_memory/helpers.rs`
- Modify: `src-tauri/src/user_memory/service.rs`

**Interfaces:**

```rust
pub enum UserMemoryCandidateSignal { Correction, Preference, Fact }
pub enum UserMemoryCandidateStatus {
    Tentative,
    Emerging,
    PendingConfirmation,
    Confirmed,
    Rejected,
    Superseded,
}

pub struct AgentMemoryProposal {
    pub content: String,
    pub signal: UserMemoryCandidateSignal,
}

pub struct CandidateObservationSource {
    pub agent_type: AgentType,
    pub opaque_source_id: String,
    pub turn_nonce: u64,
}

pub enum UserMemoryCandidateResolution {
    Confirm { edited_content: Option<String> },
    Reject,
    SupersedeByCandidate { candidate_id: String },
    SupersedeByMemoryEntry { entry_id: String },
}
```

- State path is exactly `.user-memory-learning.json` with schema version 1.
- Revision hashes the validated serialized generation.
- Deduplication is a case-insensitive digest of normalized content plus signal.
  Observation identity is candidate digest plus opaque source id plus nonce.
- Service methods are `propose_agent_memory_authorized`,
  `list_candidates`, `resolve_candidate`, and `delete_candidate`.
- Only terminal records can be deleted. A superseded record requires exactly
  one candidate or memory-entry target.

- [ ] **Step 1: Write candidate store and lifecycle tests**

Cover normalization, control/secret rejection, same-turn idempotency,
distinct-turn counts, the 10-detail cap, 500-record cap, every transition,
terminal duplicates, no automatic confirmation, stale revision, supersede
validation, terminal-only deletion, symlink rejection, and invalid JSON
preservation.

- [ ] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_candidates
```

Expected: compilation fails because candidate types and methods are missing.

- [ ] **Step 3: Implement the bounded store and explicit state machine**

Use `structured_file`. A missing file is a valid empty state. Invalid JSON
returns repairable reason `user_memory_candidate_invalid` without rewriting
the file. One proposal writes only the candidate file atomically.

- [ ] **Step 4: Run GREEN and secret-filter regressions**

```powershell
cargo test --features test-utils --test user_memory_candidates
cargo test --features test-utils --test user_memory
```

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/user_memory
git commit -m "feat(memory): 新增保守记忆候选生命周期"
```

### Task 4: Durable Transactions and Append Reconciliation

**Files:**

- Create: `src-tauri/src/user_memory/transaction.rs`
- Create: `src-tauri/tests/user_memory_transactions.rs`
- Modify: `src-tauri/src/user_memory/journal.rs`
- Modify: `src-tauri/src/user_memory/store.rs`
- Modify: `src-tauri/src/user_memory/service.rs`
- Modify: `src-tauri/src/user_memory/candidate_lifecycle.rs`

**Interfaces:**

```rust
pub enum TransactionPhase { Prepared, Committed }

pub enum ResourceGeneration<T> {
    Absent,
    Present { etag: String, value: T },
}

pub struct UserMemoryGeneration {
    pub policy: Option<UserMemoryPolicy>,
    pub documents: BTreeMap<UserMemoryDocumentId, ResourceGeneration<String>>,
    pub candidate_state: Option<ResourceGeneration<UserMemoryLearningState>>,
}

pub struct UserMemoryTransactionJournal {
    pub schema_version: u32,
    pub transaction_id: Uuid,
    pub phase: TransactionPhase,
    pub previous: UserMemoryGeneration,
    pub next: UserMemoryGeneration,
}
```

- The executor takes one validated previous/next generation and writes in
  deterministic document, candidate, then policy order.
- `append_agent_memory_inner` assumes locks are already held and returns the
  deterministic entry id and next Markdown. Public append acquires locks once.
- Confirmed append reconciles an exact active candidate in the same transaction.
  Invalid candidate JSON still permits a Markdown-only append and diagnostic.
- Settings confirmation calls the same append primitive and persists Markdown
  plus candidate status atomically.

- [ ] **Step 1: Write crash-point and reconciliation tests**

Construct journals directly and cover absent, prepared, committed, malformed,
previous/next mismatch, absent candidate generation, repeated recovery, failure
after each resource write, confirmation rollback, direct reconciliation,
duplicate reconciliation, and invalid candidate JSON plus successful Markdown.

- [ ] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_transactions
```

Expected: compilation fails because phase-aware transaction types are absent.

- [ ] **Step 3: Implement explicit phase recovery and lock-aware append**

Write/fsync prepared; atomically replace next resources; persist policy; replace
and fsync committed; remove journal; sync root. Prepared recovery restores every
previous generation. Committed recovery reapplies every next generation.
Unexpected content fails closed and preserves the journal.

- [ ] **Step 4: Run GREEN and concurrency regressions**

```powershell
cargo test --features test-utils --test user_memory_transactions
cargo test --features test-utils --test user_memory
```

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/user_memory
git commit -m "refactor(memory): 强化跨资源事务恢复"
```

### Task 5: Candidate API, Settings Snapshot, Backup, and Restore

**Files:**

- Create: `src-tauri/tests/user_memory_api.rs`
- Modify: `src-tauri/src/user_memory/types.rs`
- Modify: `src-tauri/src/user_memory/service.rs`
- Modify: `src-tauri/src/commands/user_memory.rs`
- Modify: `src-tauri/src/web/handlers/user_memory.rs`
- Modify: `src-tauri/src/web/router.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands/backup/mod.rs`
- Modify: `src-tauri/src/commands/backup/user_memory.rs`
- Modify: `src-tauri/src/commands/backup/restore.rs`
- Modify: `src-tauri/tests/backup_user_memory.rs`

**Interfaces:**

- Add shared core/Tauri/HTTP operations
  `list_user_memory_candidates`, `resolve_user_memory_candidate`, and
  `delete_user_memory_candidate`.
- List accepts optional status, `offset: u32`, and bounded `limit: u32`; it
  returns summaries, total count, and state revision.
- Settings snapshot adds resolved root/source, availability diagnostic,
  migration report, candidate diagnostic/counts, projected capabilities, and
  companion health without renaming Phase 1 fields.
- Backup whitelist becomes the three Markdown names plus
  `.user-memory-learning.json`. Receipt, lock, and journal stay excluded.
- Restore validates candidate state before the pending marker. An older archive
  stages a valid empty schema-v1 state, clearing old live candidates.

- [ ] **Step 1: Write API and archive contract tests**

Cover pagination/filtering, every resolution, stale revision, forged fields,
terminal deletion, core/HTTP parity, candidate backup inclusion, internal-file
exclusion, Phase 1 restore to empty state, invalid archive rejection, and
canonical-root-only restore.

- [ ] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_api --test backup_user_memory
```

Expected: compilation or assertions fail because API operations and candidate
archive handling are absent.

- [ ] **Step 3: Implement additive APIs and archive handling**

All handlers delegate to core functions. Hold the memory lock while taking a
backup generation and while staging validated restore state. Use additive serde
fields/defaults where backward-compatible request decoding requires them.

- [ ] **Step 4: Run GREEN and runtime compile checks**

```powershell
cargo test --features test-utils --test user_memory_api --test backup_user_memory
cargo check
cargo check --no-default-features --features server-runtime --bin iyw-claw-server
```

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/user_memory src-tauri/src/commands/user_memory.rs src-tauri/src/web src-tauri/src/lib.rs src-tauri/src/commands/backup
git commit -m "feat(memory): 提供候选管理与备份接口"
```

### Task 6: Authenticated MCP Proposal and Per-Turn Provenance

**Files:**

- Create: `src-tauri/src/acp/memory_turn.rs`
- Create: `src-tauri/tests/user_memory_mcp.rs`
- Modify: `src-tauri/src/acp/mod.rs`
- Modify: `src-tauri/src/acp/manager.rs`
- Modify: `src-tauri/src/acp/connection.rs`
- Modify: `src-tauri/src/acp/delegation/types.rs`
- Modify: `src-tauri/src/acp/delegation/transport.rs`
- Modify: `src-tauri/src/acp/delegation/listener.rs`
- Modify: `src-tauri/src/acp/delegation/companion.rs`
- Modify: `src-tauri/src/acp/delegation/tool_schema.json`
- Modify: `src-tauri/src/bin_targets/iyw_claw_mcp.rs`

**Interfaces:**

- `MemoryTurnTracker` owns an atomic monotonic nonce and active flag.
  `begin_accepted_turn()` returns a nonce, `complete_turn()` clears active
  state, and `active_nonce()` returns `Option<u64>`.
- `TokenEntry` stores authenticated Agent type, immutable launch
  capabilities, `Arc<MemoryTurnTracker>`, and a hash-derived opaque source
  id. Raw token remains map-only and never enters candidate state.
- Broker message `memory_proposal` carries token, content, and signal.
  Agent identity, lifecycle fields, source, path, count, and nonce are host-owned.
- `CompanionFeatures` adds `memory_proposal: bool`. Feature
  `memory-proposal` exposes only `propose_user_memory`.
- Proposal outside an active accepted turn is rejected. Completion,
  cancellation, disconnect, and terminal error clear active state.

- [ ] **Step 1: Write tracker, wire, companion, and listener tests**

Prove an accepted prompt increments before forwarding; rejected empty prompts
do not begin a turn; every terminal path clears state; repeated same-turn
proposal is idempotent; later turns count; forged identity/status/path/nonce
cannot cross the schema; invalid/write-disabled/inactive tokens reject; each
memory feature exposes exactly one tool; Pi/OpenClaw never gain proposal access.

- [ ] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_mcp
cargo test --features test-utils acp::delegation::
```

Expected: compilation fails because proposal wire types, feature, and tracker
are absent.

- [ ] **Step 3: Implement the authenticated proposal flow**

Derive the opaque source id from a domain-separated SHA-256 hash of launch token
and connection id. Validate tracker state in the host listener. Forward only
the two Agent-visible fields from the companion. The result reports new versus
duplicate, current status/count, and whether confirmation is recommended; it
never claims the candidate is confirmed.

- [ ] **Step 4: Run GREEN and append regressions**

```powershell
cargo test --features test-utils --test user_memory_mcp
cargo test --features test-utils acp::delegation::
cargo test --no-default-features --features mcp-runtime --bin iyw-claw-mcp
```

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/acp src-tauri/src/bin_targets/iyw_claw_mcp.rs
git commit -m "feat(memory): 接入候选记忆提议工具"
```

### Task 7: Capability Vector, Companion Health, and Launch Finalization

**Files:**

- Create: `src-tauri/src/user_memory/capabilities.rs`
- Create: `src-tauri/src/acp/companion_health.rs`
- Create: `src-tauri/tests/user_memory_capabilities.rs`
- Modify: `src-tauri/src/user_memory/mod.rs`
- Modify: `src-tauri/src/user_memory/types.rs`
- Modify: `src-tauri/src/user_memory/context.rs`
- Modify: `src-tauri/src/acp/mod.rs`
- Modify: `src-tauri/src/acp/connection.rs`
- Modify: `src-tauri/src/acp/manager.rs`
- Modify: `src-tauri/src/acp/session_state.rs`
- Modify: `src/lib/types.ts`

**Interfaces:**

```rust
pub struct UserMemoryCapabilityResult {
    pub available: bool,
    pub reason: UserMemoryCapabilityReason,
    pub degraded_reasons: Vec<UserMemoryDegradedReason>,
}

pub struct UserMemoryCapabilities {
    pub read_context: UserMemoryCapabilityResult,
    pub confirmed_append: UserMemoryCapabilityResult,
    pub candidate_proposal: UserMemoryCapabilityResult,
}

pub enum CompanionHealthStatus {
    Ready,
    Missing,
    Incompatible,
    ProbeFailed,
    Timeout,
}
```

- Health includes stable reason, expected/detected versions, selected path, and
  advertised tools. Blocking process work runs outside the async executor with
  a bounded timeout.
- Composition applies policy/origin, then each resource, adapter transport,
  health, and manifest. The three capabilities never share one boolean.
- Launch freezes capabilities, context, and fingerprint after initialize and
  companion probe/injection but before `Connected` and prompt acceptance.
- `LiveSessionSnapshot` and TypeScript mirror add immutable
  `user_memory_capabilities`.
- Maintenance guidance names only frozen available tools. Candidate state,
  migration, diagnostics, and provenance never enter prompt context.

- [ ] **Step 1: Write composition, health, launch, and context tests**

Use all eleven Agents in a table. Cover policy off, delegation off, probe
origin, partial readable documents, read-only Memory, invalid/read-only
candidate state, companion missing/incompatible/malformed/hung, each one-tool
manifest, both tools, readiness ordering, live serialization, Pi/OpenClaw
guidance, and exclusion of candidate/diagnostic content.

- [ ] **Step 2: Run RED**

```powershell
cargo test --features test-utils --test user_memory_capabilities
```

Expected: compilation fails because capability/health and live snapshot fields
are absent.

- [ ] **Step 3: Implement composition and bounded health probing**

Put new logic in the two new modules; keep large ACP files to lifecycle wiring.
Omit an unreadable document while retaining degraded read context when another
enabled document is readable.

- [ ] **Step 4: Run GREEN and context/session regressions**

```powershell
cargo test --features test-utils --test user_memory_capabilities --test user_memory_context_policy --test user_memory_injection
cargo test --features test-utils acp::session_state::
```

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/user_memory src-tauri/src/acp src/lib/types.ts
git commit -m "feat(memory): 暴露会话记忆能力与伴生健康"
```

### Task 8: Sidecar Manifest and Packaging Validation

**Files:**

- Modify: `src-tauri/src/acp/delegation/companion.rs`
- Modify: `src-tauri/src/bin_targets/iyw_claw_mcp.rs`
- Modify: `src-tauri/scripts/prepare-sidecars.mjs`
- Create: `src-tauri/scripts/prepare-sidecars.test.mjs`
- Modify: `.github/workflows/release-tauri.yml`

**Interfaces:**

- `binary_capabilities()` advertises exact version and complete tool
  manifest including both memory tools.
- Native preparation executes the copied binary with `--capabilities` and
  validates JSON, version, and exact tool set under a timeout.
- Cross-target preparation checks naming, inclusion, and source-derived expected
  manifest, and explicitly labels the result non-runtime.
- Runtime launch remains authoritative and degrades on mismatch.

- [ ] **Step 1: Locate packaging files and write manifest regressions**

The test fails when either memory tool is absent, version is stale, JSON is
malformed, or native probe times out/exits non-zero. A cross-target validation
must never claim successful executable probing.

- [ ] **Step 2: Run RED**

```powershell
node --test src-tauri/scripts/prepare-sidecars.test.mjs
cargo test --no-default-features --features mcp-runtime --bin iyw-claw-mcp
```

Expected: the new complete-manifest assertion fails before implementation.

- [ ] **Step 3: Implement structured complete-manifest validation**

Parse JSON structurally, use an explicit timeout, compare exact sets, avoid
human-log parsing, and add no network dependency.

- [ ] **Step 4: Run GREEN and a real native probe**

```powershell
cargo build --no-default-features --features mcp-runtime --bin iyw-claw-mcp
src-tauri\target\debug\iyw-claw-mcp.exe --capabilities
node --test src-tauri/scripts/prepare-sidecars.test.mjs
pnpm tauri:prepare-sidecars
```

Assert the JSON contains both memory tools and native sidecar preparation
reports runtime validation.

- [ ] **Step 5: Commit**

```powershell
git add src-tauri/src/acp/delegation/companion.rs src-tauri/src/bin_targets/iyw_claw_mcp.rs src-tauri/scripts/prepare-sidecars.mjs .github/workflows/release-tauri.yml
git commit -m "fix(memory): 校验完整伴生工具清单"
```

### Task 9: Settings Capability, Migration, and Candidate Experience

**Files:**

- Create: `src/lib/user-memory-learning.ts`
- Create: `src/components/settings/user-memory-capability-panel.tsx`
- Create: `src/components/settings/user-memory-migration-alerts.tsx`
- Create: `src/components/settings/user-memory-candidate-panel.tsx`
- Modify: `src/lib/api.ts`
- Modify: `src/lib/user-memory-documents.ts`
- Modify: `src/components/settings/user-memory-settings.tsx`
- Modify: `src/components/settings/user-memory-settings.test.tsx`
- Modify: `src/i18n/messages/en.json`
- Modify: `src/i18n/messages/zh-CN.json`

**Interfaces:**

- TypeScript mirrors Rust candidate, capability, health, migration, paging, and
  resolution DTOs exactly.
- API functions are `listUserMemoryCandidates`,
  `resolveUserMemoryCandidate`, and `deleteUserMemoryCandidate`.
- Capability panel labels projection as applying to a new conversation and
  separates read, confirmed append, and proposal for all eleven Agents.
- Filters cover tentative, emerging, pending, and terminal candidates. Commands
  support confirm, edit-and-confirm, reject, supersede, and terminal delete with
  revision guards.
- Root/source and migration warnings come from the backend. The page includes
  the provider-transmission/native-history disclosure from the design.
- Use existing shadcn controls and Lucide icons. No nested cards, decorative
  gradients/orbs, feature-instruction prose, or dynamic route.

- [ ] **Step 1: Extend frontend behavior tests first**

Cover root/source, unavailable state, projected badges, Pi/OpenClaw reasons,
degraded reads, per-file migration warning, filters, every resolution request,
terminal deletion, paging, loading/error/empty states, stale-revision draft
preservation, and parity of English/Chinese locale keys. Test real visible
behavior rather than mock component existence.

- [ ] **Step 2: Run RED**

```powershell
pnpm test -- src/components/settings/user-memory-settings.test.tsx
```

Expected: compilation or assertions fail because types, API, and controls are
absent.

- [ ] **Step 3: Implement focused types, components, and API wiring**

Keep `user-memory-settings.tsx` as an orchestrator. Use stable control
dimensions and wrap long paths/reasons. Profile and Soul remain editor-only;
candidate controls render only for the Memory tab.

- [ ] **Step 4: Run GREEN, focused lint, and static build**

```powershell
pnpm test -- src/components/settings/user-memory-settings.test.tsx src/lib/settings-navigation.test.ts
pnpm eslint src/components/settings/user-memory-settings.tsx src/components/settings/user-memory-capability-panel.tsx src/components/settings/user-memory-migration-alerts.tsx src/components/settings/user-memory-candidate-panel.tsx src/lib/user-memory-learning.ts src/lib/api.ts src/lib/types.ts
pnpm build
```

- [ ] **Step 5: Commit**

```powershell
git add src/components/settings src/lib/api.ts src/lib/types.ts src/lib/user-memory-documents.ts src/lib/user-memory-learning.ts src/i18n/messages/en.json src/i18n/messages/zh-CN.json
git commit -m "feat(memory): 增加候选记忆管理界面"
```

### Task 10: End-to-End Integration, Review, and Delivery Gate

**Files:**

- Modify only files needed to correct failures found by the commands below
- Delete ad-hoc smoke files, scratch binaries/fixtures, generated review
  packages, and temporary test scaffolding outside permanent harnesses
- Delete every new test file from Tasks 1-9 and restore any existing test file
  changed only for verification before staging production changes

**Interfaces:**

- The delivery surface is the approved design's 18 acceptance criteria.
- Branch base is `051ceef`. The original checkout's unrelated dirty files are
  never copied, reverted, staged, or committed.
- Phase 2 is committed locally but is not pushed unless the user explicitly
  requests a Phase 2 push after verification.

- [ ] **Step 1: Format and inspect the complete diff**

Run formatters without concealing changes, then verify:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
pnpm prettier --check "src/**/*.{ts,tsx}" "src/i18n/messages/{en,zh-CN}.json"
git diff --check 051ceef..HEAD
git status --short
```

- [ ] **Step 2: Run full frontend verification**

```powershell
pnpm eslint .
pnpm test
pnpm build
```

- [ ] **Step 3: Run full desktop Rust verification**

```powershell
Set-Location src-tauri
cargo check
cargo test --features test-utils
cargo clippy --all-targets --features test-utils -- -D warnings
```

- [ ] **Step 4: Run server and MCP verification**

```powershell
cargo check --no-default-features --features server-runtime --bin iyw-claw-server
cargo test --no-default-features --features server-runtime --bin iyw-claw-server --lib
cargo clippy --no-default-features --features server-runtime --bin iyw-claw-server --lib -- -D warnings
cargo check --no-default-features --features mcp-runtime --bin iyw-claw-mcp
cargo test --no-default-features --features mcp-runtime --bin iyw-claw-mcp
cargo clippy --no-default-features --features mcp-runtime --bin iyw-claw-mcp -- -D warnings
```

- [ ] **Step 5: Run real capability, root, migration, and restore smokes**

Execute the native MCP binary's `--capabilities`. Use temporary roots and
explicit startup parameters to prove desktop independence, server priority,
migration idempotency, candidate-corruption degradation, Phase 1 restore to
empty candidates, and Phase 2 round-trip. Capture evidence, then remove every
ad-hoc smoke artifact.

- [ ] **Step 6: Review all acceptance criteria and the branch diff**

Generate one review package for `051ceef..HEAD`. Check each acceptance
criterion against code and evidence. Fix every Critical or Important issue and
re-run covering tests. Report residual Minor issues instead of discarding them.

- [ ] **Step 7: Verify temporary artifact cleanup**

```powershell
git status --short
rg --files | rg "memory.*(smoke|scratch|tmp)|review-package|task-.*-(brief|report)"
```

Expected: only intentional source, documentation, and permanent tests are
tracked/untracked. Build output and prepared sidecars remain ignored.

- [ ] **Step 8: Commit tracked integration fixes when present**

```powershell
git add -u
git commit -m "test(memory): 完成统一记忆集成验证"
```

Inspect `git diff --cached --name-only` before committing; add any new
permanent regression file by its exact path only. Skip this commit if review
produces no integration changes. Report exact test commands, counts, exit
codes, branch, HEAD, and whether push was performed.
