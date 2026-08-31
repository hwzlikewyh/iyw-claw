# Capability Families

Load this reference whenever the task needs an iyw-claw host action, current
state, delivery, interaction, or a dynamic plugin capability. The names below
are routing anchors, not permission to call them directly. Always search the
current catalog, read the returned stable ID, and obey its current schema.

## Quick Family Map

| User intent | Search terms | Capability family | Main result to verify |
| --- | --- | --- | --- |
| “我是谁”“查当前会话” | `user profile`, `session info` | Identity/session | Safe profile or exact session metadata |
| “以前做过什么” | `search task history` | Session history | Bounded task projection, not a full transcript |
| “把文件交付给我” | `present final files` | [Artifacts](artifact-delivery.md) | Current conversation artifact receipt |
| “问我一个必要选择” | `ask user question` | Interaction | Submitted answer or dismissal |
| “检查用户有没有改要求” | `check user feedback` | Interaction | New steering messages, or empty snapshot |
| “分给另一个 Agent” | `delegate task` | [Delegation](delegation.md) | `task_id`, then terminal task result |
| “发消息/查消息” | `channel target`, `send message` | [Channels](channel-operations.md) | Target resolution and per-item send status |
| “每天运行/修改计划” | `scheduled task` | [Automation](automation.md) | Project/task ID and resulting enabled state |
| “插件提供了什么能力” | `plugin capability` | Dynamic plugin | Live availability and plugin result |

## Identity, Sessions, and History

### Current profile

Use the current user-profile capability for display name, nickname, preferred
salutation, or organization. It accepts no arguments and intentionally excludes
account IDs, phone numbers, points, avatar URLs, tokens, and credentials. Do not
search memory to answer “what is my name?”

### Referenced session

When the user provides an `iyw-claw://session/<number>` reference, extract only
that numeric conversation ID and use the session-info capability. It can return
title, Agent, status, workspace, branch, model, message count, and token stats;
request recent messages only when needed. A `found: false` result means the
referenced session no longer exists and is still a valid read result.

### Historical task lookup

Use session-history search for “what tasks were completed before” or to locate a
previous task by keywords. It returns bounded intent/result/decision/failure/
pending summaries and a conversation ID. It is not memory recall and does not
replace reading a referenced session. Use memory recall for prior decisions,
preferences, and reusable Agent experience.

## Artifacts and User-Facing Delivery

If the task produces a final file, directory, or public HTTP/HTTPS URL, register
all final items with the artifact capability before the final response. Use
working-directory-relative or absolute paths exactly as produced. Register only
user-facing deliverables: never source files, configuration, tests, migrations,
build output, caches, logs, temporary files, or internal notes unless explicitly
requested. A Markdown preview or ordinary URL in chat is not proof of
registration; verify the returned artifact result.
For HTML/Markdown image hosting, preview limits, current-reply attribution,
and partial registration handling, load [artifact-delivery.md](artifact-delivery.md).

## Interaction Checkpoints

- Use feedback checks before implementation, before a significant architecture
  choice, after a meaningful subtask, and when pausing on a long task. An empty
  result means no new steering was observed; continue without inventing input.
- When a required input, acceptance criterion, scope boundary, or user-owned
  choice is unclear, has multiple reasonable interpretations, or cannot be
  safely inferred, proactively search/read/invoke the `ask_user_question`
  capability before acting. Do not guess through ambiguity. Ask one concise
  multiple-choice question, or one call containing a few directly related
  questions, then wait for the answer and continue with the chosen requirements.
- The question schema accepts 1-4 questions per call and 2-4 options per
  question. Set `multiSelect` only when more than one option can be selected
  independently; otherwise use single choice. The call blocks until the user
  submits or dismisses the card, so do not start side effects while waiting.
- Keep question options concrete and mutually understandable. Do not use the
  question capability for passwords, tokens, cookies, credentials, ordinary
  progress confirmation, or a selector failure that has a documented recovery.
  If the capability is not advertised, ask the necessary question plainly in
  chat and report the gateway limitation instead of inventing a tool or schema.

## Delegation

For delegated work, load [delegation.md](delegation.md). The short rule is to
send a complete cold-start prompt, preserve returned `task_id` values, wait for
terminal status, and cancel only an identified task whose result is no longer
wanted.

## Message Channels

For channel discovery, target resolution, local message history, sending,
credentials, QR authorization, connection diagnostics, deletion, and settings,
load [channel-operations.md](channel-operations.md). Never send before resolving
the opaque target through the host.

## Scheduled Automation

For project selection, cron/timezone input, task CRUD, Agent selection, and
soft-delete behavior, load [automation.md](automation.md). Read the current task
before updates or deletes and never send both project selectors.

## Dynamic Plugin Capabilities

Plugin capabilities may appear in the live catalog alongside built-in entries.
Use only an available result returned by the current search and read its full
description, plugin status, required inputs, and schema. Host authorization,
workspace scope, Agent type, permission revision, cancellation, and plugin
availability are enforced at invocation. An unavailable plugin result is a
terminal routing limitation for this turn; do not call an internal plugin name,
retry under another namespace, or fabricate a fallback schema.

## Completion Discipline

For every family, distinguish a successful effect from a queued, preview,
blocked, canceled, failed, unavailable, or effect-unknown result. Preserve
stable IDs, cursors, revisions, and receipts exactly as returned. Do not expose
the gateway's private session envelope or use host paths and provider IDs as
user-facing evidence.
