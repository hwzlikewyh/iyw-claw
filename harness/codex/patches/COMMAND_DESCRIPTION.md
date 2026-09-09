# Command Purpose Descriptions

The bundled worker accepts an optional `description` on `exec_command`.
The agent should supply a short purpose in the user's language. The field is
display metadata; it does not change command text, sandbox policy, approvals,
process ownership, timeouts, or tool output.

## Data Flow

1. `codex-core` advertises and parses the optional argument.
2. The existing turn `ExtensionData` stores normalized descriptions by call ID.
   Whitespace is collapsed, control characters removed, and text limited to
   120 Unicode characters. Async completion uses the original turn and call ID.
3. Canonical command start and completion items carry `description`, defaulting
   missing descriptions to `None` and omitting them when serializing. Deprecated
   execution events retain their upstream format. Existing records remain readable.
4. App Server maps the field into `ThreadItem::CommandExecution`; the ACP
   harness preserves it in `rawInput`. The existing frontend description
   priority displays it without changing tool identity or execution status.
5. Described completion items are retained in legacy as well as paginated
   rollouts. The local history parser keeps direct-call arguments and projects
   described nested Code Mode commands using their actual call IDs. Repeated
   lifecycle records do not create duplicate command cards.

Commands without descriptions retain the previous display and history behavior.
Manually entered shell commands and synthetic approval items have no inferred
description. Approval prompts continue to show the actual command.

## Upstream Maintenance

Baseline: OpenAI Codex `rust-v0.153.4`, commit
`3d2ee51ca2d5db578f328aa75e20aa22c0197c9a`.

The existing `codex-core` and `codex-rollout` overrides are extended. Four
additional production source copies are required because Rust structs and enum
constructors must agree across the dependency graph:

- `codex-protocol`: optional description on the canonical command item.
- `codex-app-server-protocol`: optional description on the public command item
  and the canonical conversion; legacy and synthetic items use `None`.
- `codex-app-server`: synthetic command item constructors use `None`; a canonical
  start with a description updates an earlier approval card using the same ID.
- `codex-thread-store`: commands migrated from deprecated events use `None`.

`command-description.patch` isolates the runtime source changes from the
production-source copies and the pre-existing Windows patches. Apply it to the
refreshed local overrides with `git apply --check` before an upgrade. It is a
review/reapplication artifact; the build already consumes the patched sources.

Their standalone manifests retain upstream production dependency versions,
features, and platform conditions. Workspace path dependencies resolve to the
same pinned commit. Upstream test modules, fixtures and executables are omitted.
The Apache license is included with each copied crate. Worker and harness
Cargo patch tables select the same four overrides. The precomputed protocol
exports also include the optional field in command item JSON and TypeScript.

When adopting a new Codex release:

1. Compare the argument schema and handler, turn `ExtensionData`, command item
   types, legacy conversions, App Server item builders, and rollout policy.
2. Reapply the description changes to the refreshed production sources. Keep
   the existing Windows process patches when refreshing core/rollout sources.
3. Update the pinned dependencies and worker lockfile together. Confirm Cargo
   resolves exactly one copy of each protocol crate to the local override.
4. Check the worker library and application parser compile. Review direct and
   Code Mode calls, concurrent IDs, delayed completion, failure, history replay,
   omitted/blank descriptions, and records produced before this change.
5. If upstream adds a corresponding field, adopt its semantics and retire the
   redundant patch while retaining compatibility with saved `description`.

## Verification

Completed on 2026-09-09:

- `cargo check --manifest-path harness/xinghe-worker/Cargo.toml --lib --offline --locked --jobs 4`
- `cargo check --manifest-path src-tauri/Cargo.toml --lib --no-default-features --features server-runtime --offline --jobs 4`
- Rust syntax inspection of the additional production source copies.
- Static review of the argument, canonical events, delayed completion, ACP
  projection, existing frontend title selection, and local history parser.

After the user's switching-verification request, an isolated executable loaded
the actual history-normalization and item-projection modules and verified:

- A/B/A history replay with the same call ID in separate sessions.
- Fresh snapshot replay, success/failure state, and duplicate suppression.
- Interleaved completion of parallel commands without mixing descriptions.
- Old and blank-description records, next-turn fallback, and an existing
  command card receiving its purpose after approval.

These targeted checks passed. Unrelated ACP content/subagent handlers and
message-container types were stubbed in the isolated executable; this is not
full application coverage. The temporary executable sources and build output
were removed. Session routing/snapshot identity guards and shared turn state
during model selection were reviewed statically.

No desktop click-through/end-to-end verification or installer build was
performed. Compilation emitted existing unused-code and future-compatibility
warnings. Successful compilation and module checks do not replace desktop
acceptance testing.
