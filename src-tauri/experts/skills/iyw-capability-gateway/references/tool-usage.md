# Gateway Tool Usage

This is the short operational card for the live iyw-claw gateway. Load a more
detailed reference before acting when the task matches one:

| Task | Required reference |
| --- | --- |
| Known IYW website API request | [iyw-http.md](iyw-http.md) |
| Session/profile/history, interaction, or plugin capability | [capability-families.md](capability-families.md) |
| Final files/directories/URLs, current-reply delivery, HTML/Markdown image hosting | [artifact-delivery.md](artifact-delivery.md) |
| Channels, targets, message history/sending, credentials, QR authorization, diagnostics | [channel-operations.md](channel-operations.md) |
| Scheduled-task projects, cron, create/update/pause/delete | [automation.md](automation.md) |
| Delegate, wait for, collect, or cancel an independent Agent task | [delegation.md](delegation.md) |
| Browser, public web, screenshots, audio, transcription, image understanding | [browser-and-media.md](browser-and-media.md) |
| Memory, self-learning, corrections, candidates, harvest, index, document maintenance | [memory-and-learning.md](memory-and-learning.md) |
| Research or platform evidence | [research-workflow.md](research-workflow.md) and, when web access is needed, [internet-routing.md](internet-routing.md) |

You must read the full tool description and input schema before first use.
Reuse a previous read in this conversation without another read. Direct tools
include these instructions in their definitions. Capabilities
behind the gateway require an explicit `read_iyw_capability`; each direct memory
operation also requires that read using its advertised capability mapping.

## Shortest Valid Call

Use a direct tool when it covers the current subgoal, even when the user did not
name it. This avoids unnecessary search/read/invoke calls. Reuse already-read
capability instructions in the same session; continue checking current business
state, returned revisions, and availability when the task requires them.

| Work | Input shape |
| --- | --- |
| Fusion image models | Call `list_iyw_image_models` with `{}`; choose a returned model supporting generation or editing as needed |
| Image generation/editing | Text-to-image: `generate` (`images/generations`); single-image changes: `variation`; multi-image fusion: `mix`; four-panel or same-series extension from one reference: `extend`. Explicit `edit` (`images/edits`) requires source images. `generate`, `auto` without images, and `edit` need an exact model ID from `list_iyw_image_models`; no prior platform failure is required. Default timeout: platform 600s, Fusion 300s; override with `wait.timeoutSeconds`, including above 600. Set `delivery.registerArtifact=false` for intermediate assets. No generation capability ID exists |
| Document knowledge | `search_iyw_knowledge`: `query`, optional known filters; `folderId` is an integer and `fileId` is a string |
| Known IYW website API | `fetch_iyw_url`: HTTPS `iyw.cn` and all subdomains; GET/POST/PUT/PATCH/DELETE/HEAD/OPTIONS; JSON/form/text bodies and ordinary header overrides; current login token supplied by the host; fixed output envelope |
| Memory recall | Read the mapped capability once, then `manage_iyw_memory` with `operation` and `parameters`; policy preflight is automatic |
| Other host capabilities | Search/read once, then `invoke_iyw_capability` with `capability_id` and an `arguments` object |
| Questions | `ask_user_question` with `questions`; group related questions in one call |
| Final files | `present_task_files` with one `files` array for the ready deliverables |
| Interactive HTML | `show_interactive_html` with `title` and `html`; wait only when its response blocks the next step |

Business fields go directly inside the specified JSON object. Do not stringify
it or add an extra `arguments`/`parameters` layer. Preserve the schema's spelling,
ID types, enums, and mutually exclusive fields. Omit unknown optional fields.
Existing returned IDs and prior schema reads can be reused; never fabricate IDs
or cache a business-state answer as if it were a fresh execution.

Tool identities and capability IDs are different fields. Copy a capability ID
exactly from current search results or the advertised memory mapping; do not
construct one from a tool name or add a version suffix. Reading a directly
advertised tool's definition is not a `read_iyw_capability` invocation.

When `capability_not_found` returns a direct-tool hint, inspect that tool's actual
advertised definition and stop guessed-ID attempts. The hint is guidance, not an
automatic execution or permission to resubmit earlier work. Its `not_started`
status applies only to that lookup. A backend rejection, timeout, or uncertain
creation must retain its original error/task identity; do not turn it into
discovery or repeat the operation under another name.

## Five-Step Sequence

1. Inspect the actual callable surface and choose one complete trio when the
   current host sub-goal requires gateway discovery. The roles are
   `search_iyw_capabilities`, `read_iyw_capability`, and
   `invoke_iyw_capability`; prefer the unique visible `iyw-claw-builtin-*`
   trio. If a role is missing or multiple trios are ambiguous, stop this
   gateway attempt and return to the owning direct tool, domain Skill, or
   read-only browser workflow.
2. Search with 2-5 discriminating action/object terms in Chinese or English,
   such as `读取 网页`, `会议 音频 转写`, `提交 成果`, or `send channel message`.
   Do not search greetings, trivial self-contained requests, current-turn-only
   context, or merely to enumerate tools.
3. Treat results as the current catalog index. Compare the returned summary,
   aliases, `when_to_use`, status, required inputs, and schema digest. Read the
   best matching stable ID and its full description/schema; read at most one
   same-result alternative.
4. Invoke only an available ID returned by that search. Supply arguments exactly
   as the current read schema requires, using its examples and declared fields.
   Omit unnecessary optional fields; never borrow parameters from another tool.
   Ask for a missing primary object; never guess IDs, paths, URLs, field names,
   or permissions.
5. Verify the result state and business effect. Distinguish success from
   queued, preview, blocked, canceled, failed, unavailable, and effect-unknown.

When progress needs a concrete user-owned input, preference or decision, call
an advertised `ask_user_question` directly. Use concise options or free text for
requirements, scope, missing information and short feedback; wait for the answer.
Only fall back to search/read/invoke when the direct question tool is unavailable.
Do not ask routine progress confirmations or questions you can resolve yourself.

When seeing, manipulating or experimenting improves understanding or feedback,
proactively call `show_interactive_html`. Design the page and JSON result freely;
examples are not restrictions. It loads automatically, returns immediately by
default, and waits for `iyw.submit(data)` only with `wait_for_response: true`.
See [interaction-tools.md](interaction-tools.md). A direct interaction tool does
not need capability search/read/invoke or installation of a page framework.

Reading before first use is mandatory Agent behavior. The host does not record
reads or reject calls based on read history. Reuse full instructions already
read in this conversation, including on later turns; do not issue another read
for every call. Search summaries alone do not replace the full instructions.

A `capability_schema_mismatch` with `execution_status=not_started` permits one
correction using the schema already read and the error's field hints. Retry the
same intended operation once with corrected arguments. A text-only error must also
explicitly report schema rejection and `execution_status=not_started`. A parameter
error does not require another read; read only if not previously read. Use the
error's field path, allowed properties, enum choices, bounds, and hint; never replay unchanged arguments or silently
drop intended behavior. Stop this recovery if the corrected call fails.

An empty result, unknown ID, malformed output, timeout, unavailable capability,
schema rejection without that evidence, or two non-matching reads ends the
current gateway attempt. It does not by itself end the user's task. Permission
and effect-unknown errors do not permit this correction. One search retry is
allowed only after an exhausted result set and only with a close synonym. Do
not switch namespaces, promote nested tools, or cycle guessed names; hand off
to another already-authorized route instead. Follow the owning reference for
other bounded recoveries; do not chain recoveries after a failed correction.

## Memory Card

For `manage_iyw_memory`, first read the selected operation's full instructions
using the capability ID advertised in `operation.description`. Then pass its
schema fields under `parameters`. This direct tool executes policy preflight
automatically; reading instructions does not execute the policy. The following
explicit policy sequence applies when invoking memory through the gateway.

When memory is relevant, load `memory-and-learning.md`. Before the first direct
memory operation in each accepted turn, invoke `read_memory_policy` exactly as
advertised. If the host returns `memory_policy_required`, perform that
preflight and retry the intended operation with fresh schema evidence. For
substantive work, do one bounded `memory_recall` unless the request is clearly
self-contained. Interpret `matched`, `no_evidence`, and `unavailable` literally;
timeout is not evidence of an empty history.

Use current document reads only when the actual `memory`, `profile`, or `soul`
document is needed. Read the smallest set, preserve revisions/eTags, and use
transactional update/correction capabilities rather than shell edits. Candidate
resolve/delete operations require a fresh stable ID and revision; rescan and
index rebuild require `execute: false` preview first.

## Browser and Audio Card

Load `browser-and-media.md` before browser or media work. For browser tasks use
list tabs -> reuse/open -> fresh snapshot/read -> one intended action -> fresh
snapshot -> verify. Snapshot references expire after page changes. Use the
dedicated tools before `browser_command`; read `agent-browser` first for an
advanced command. Request human action only for login, MFA, CAPTCHA, device
approval, secure payment, or explicit human review.

Choose flash transcription for ordinary short audio (immediate result, up to
100 MiB/2 hours). Choose durable async transcription for meetings, multiple
speakers, channel separation, long/oversized, or resumable work (up to
512 MiB/5 hours); query a non-terminal result by its returned `job_id`.

## Delivery, Receipts, and Safety

Register every final user-facing file, directory, or public URL with
`present_task_files` before completion. Do not register source, configuration,
tests, migrations, build output, caches, logs, temporary files, or internal work
unless explicitly requested.

If an invocation returns `iyw_delivery_receipt`, preserve it exactly and send it
as top-level `delivery_ack` on the next real invocation. Never put it inside
business arguments or fabricate a call just to acknowledge it.

Never expose tokens, credentials, cookies, private gateway envelopes, raw
provider IDs, or arbitrary host paths. The host owns authorization, locking,
idempotency, cancellation, confirmation, persistence, and schema validation.
