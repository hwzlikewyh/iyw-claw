# Gateway Tool Usage

This is the short operational card for the live iyw-claw gateway. Load a more
detailed reference before acting when the task matches one:

| Task | Required reference |
| --- | --- |
| Session/profile/history, interaction, or plugin capability | [capability-families.md](capability-families.md) |
| Final files/directories/URLs, current-reply delivery, HTML/Markdown image hosting | [artifact-delivery.md](artifact-delivery.md) |
| Channels, targets, message history/sending, credentials, QR authorization, diagnostics | [channel-operations.md](channel-operations.md) |
| Scheduled-task projects, cron, create/update/pause/delete | [automation.md](automation.md) |
| Delegate, wait for, collect, or cancel an independent Agent task | [delegation.md](delegation.md) |
| Browser, public web, screenshots, audio, transcription, image understanding | [browser-and-media.md](browser-and-media.md) |
| Memory, self-learning, corrections, candidates, harvest, index, document maintenance | [memory-and-learning.md](memory-and-learning.md) |
| Research or platform evidence | [research-workflow.md](research-workflow.md) and, when web access is needed, [internet-routing.md](internet-routing.md) |

## Five-Step Sequence

1. Inspect the actual callable surface and choose one complete trio. The roles
   are `search_iyw_capabilities`, `read_iyw_capability`, and
   `invoke_iyw_capability`; prefer the unique visible
   `iyw-claw-builtin-*` trio. If a role is missing or multiple trios are
   ambiguous, stop using the gateway for this turn.
2. Search with 2-5 discriminating action/object terms in Chinese or English,
   such as `读取 网页`, `会议 音频 转写`, `提交 成果`, or `send channel message`.
   Do not search greetings, trivial self-contained requests, current-turn-only
   context, or merely to enumerate tools.
3. Treat results as the current catalog index. Compare the returned summary,
   aliases, `when_to_use`, status, required inputs, and schema digest. Read the
   best matching stable ID; read at most one same-result alternative.
4. Invoke only an available ID returned by that search. Supply arguments exactly
   as the current read schema requires. Ask for a missing primary object; never
   guess IDs, paths, URLs, field names, or permissions.
5. Verify the result state and business effect. Distinguish success from
   queued, preview, blocked, canceled, failed, unavailable, and effect-unknown.

If you do not know how to proceed safely, or a required input, acceptance
criterion, scope boundary, or user-owned choice is unclear or has multiple
reasonable interpretations, pause the task and proactively discover
`ask_user_question` through the same trio. Ask one concise multiple-choice
question (or one call with a few related questions), wait for the answer, and
continue with the selected requirements. Never guess through ambiguity. Do not
use this capability for secrets, ordinary progress confirmation, or selector
failures; if it is not advertised, ask the necessary question plainly in chat
and report the concrete gateway limitation.

An empty result, unknown ID, malformed output, timeout, unavailable capability,
schema rejection, or two non-matching reads ends gateway use for the turn. One
search retry is allowed only after an exhausted result set and only with a close
synonym. Do not switch namespaces, promote nested tools, or cycle guessed names.

## Memory Card

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
