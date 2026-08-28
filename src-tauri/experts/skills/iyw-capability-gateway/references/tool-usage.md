# Gateway Tool Usage

Use this as the operational card for current iyw-claw capabilities. The live
catalog and `read_iyw_capability` schema always override this reference.

## Gateway sequence

1. Inspect the actual callable surface and choose one complete gateway trio.
2. Search with 2–5 action/object terms.
3. Read the best returned stable ID; read at most one same-result alternative.
4. Invoke that exact ID with arguments matching the current schema.
5. Verify the result and continue the user's goal.

An empty result, unknown ID, malformed response, timeout, unavailable status, or
schema rejection ends gateway use for the turn. Do not switch namespaces, guess
arguments, or retry a failed route under another name. Preserve any returned
`iyw_delivery_receipt` as top-level `delivery_ack` on the next real invocation.

## Memory cards

| Capability | Search terms | Input preparation | Success/failure handling |
| --- | --- | --- | --- |
| `iyw.memory.recall.search.v1` | `recall memory history` / `检索历史记忆` | Short query; optional limit 1–8. | `matched` is usable evidence; `no_evidence` is not false; `unavailable` is a limitation. |
| `iyw.memory.documents.read.v1` | `read current memory document` / `读取当前记忆文档` | `documents` array of the smallest unique `memory/profile/soul` set. | Use returned content/revisions; do not read unrelated documents. |
| `iyw.memory.confirmed.append.v1` | `remember confirmed preference` / `记住确认偏好` | One concise durable user-stated `content`, max 1000 chars. | It is append-only and may report already recorded; never store secrets or task state. |
| `iyw.memory.candidate.propose.v1` | `propose memory correction` / `候选记忆纠正` | `content` plus `signal: correction|preference|fact`. | Use status/count/recommendation; proposal is not confirmed memory. |
| `iyw.memory.candidates.list.v1` | `list memory candidates` / `列出候选记忆` | Optional status, offset, limit 1–100. Page using `total` and retain `revision`. | Re-read after revision conflict; never guess candidate IDs. |
| `iyw.memory.candidate.resolve.v1` | `resolve memory candidate` / `处理候选记忆` | Exact `candidateId`, `expectedRevision`, and one resolution. | Confirm/reject/supersede uses host reference normalization; terminal candidates cannot resolve again. |
| `iyw.memory.candidate.delete.v1` | `delete terminal memory candidate` / `删除终结候选` | Exact terminal `candidateId` and current revision. | Only terminal candidates are deletable; conflict requires a fresh list. |
| `iyw.memory.harvest.status.v1` | `memory harvest status` / `记忆队列状态` | No arguments. | Read counts and failure timestamps; do not infer that queued work completed. |
| `iyw.memory.harvest.rescan.v1` | `rescan memory harvest` / `重扫记忆收获` | `execute:false` preview first; `true` only on explicit request. | Retains terminal records; report preview vs executed distinctly. |
| `iyw.memory.candidate.index.rebuild.v1` | `rebuild memory index` / `重建记忆索引` | `execute:false` preview first; `true` only on explicit repair request. | Idempotent host rebuild; report affected count and revision. |
| `iyw.memory.settings.read.v1` | `memory settings health` / `记忆健康设置` | No arguments. | Safe summary omits paths, credentials, and raw document contents. |
| `iyw.memory.documents.update.v1` | `edit memory documents` / `编辑记忆文档` | Read first; send `expectedRevision` and exact per-document `content/enabled/expectedEtag` patches. | On conflict, stop and re-read. No arbitrary paths, shell writes, or stale replay. |
| `iyw.memory.documents.correct.v1` | `correct memory entry` / `修正记忆条目` | Read the document, then send `document`, exact `oldContent/newContent`, and `expectedEtag`. | Host updates the entry and candidate references in one transaction; conflicts require a fresh read. |

## Browser and research cards

For web work, read `agent-browser` first. Discover `iyw.browser.tabs.list.v1`
before opening, reuse the current tab, open the requested HTTPS URL, then use a
fresh snapshot/read. After route changes, popups, writes, or dynamic updates,
take another snapshot before clicking/filling. Verify URL/title/text/download;
a successful click is not proof. Use `iyw.browser.user_action.request.v1` only
for human-only login/MFA/CAPTCHA/payment steps. Use `browser_present` for a
completed user-facing page, not routine browsing.

For deep research, read `research-workflow.md` and use a source ledger. Search
each sub-question with multiple variants, canonicalize URLs, deep-read the
strongest pages, cross-check claims, label uncertainty, and deliver long
reports through `iyw.artifacts.present.v1`.

## Other capability families

Use the same search/read/invoke discipline for:

- `iyw.artifacts.present.v1`: register only final user-facing files/directories/
  public URLs in the current conversation Artifacts; never register caches,
  source, logs, tests, or temporary work.
- `iyw.interaction.*`: ask only for a required unresolved decision; use feedback
  checks at sensible checkpoints during long work.
- `iyw.delegation.*`: delegate only bounded, independently verifiable work and
  inspect status by the returned task ID.
- `iyw.channels.*`: resolve the exact configured channel and target before
  sending; never put credentials in ordinary arguments.
- `iyw.audio.*`: use flash for ordinary short immediate audio within its current
  limits; use durable async transcription plus query for long, multi-speaker,
  channel-separated, oversized, or resumable work.
- `iyw.image.*`: analyze existing images for understanding and use the selected
  image Skill for generation/editing; do not guess image API payloads.
- `iyw.automation.*`: inspect existing projects/tasks first; destructive changes
  require the exact target and confirmation rules from the read schema.

## Result discipline

Separate `matched`, `no_evidence`, `unavailable`, preview, accepted/rejected,
and effect-unknown states in the response. Keep user-facing updates concise,
but state the concrete limitation when a capability did not complete. Do not
claim an external write, memory change, report delivery, or browser business
result without its returned evidence.
