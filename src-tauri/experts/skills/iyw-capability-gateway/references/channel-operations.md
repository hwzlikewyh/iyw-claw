# Channel Operations

Load this reference for configured message channels, contacts/targets, local
message history, sending, channel credentials, QR authorization, connection
diagnostics, or global channel settings. This is a host-side workflow. A domain
Skill such as `wecom-unified` may own platform-specific business operations;
use the gateway for iyw-claw channel state and delivery, and use the live
catalog/schema for the exact current capability ID.

## Choose the Operation

| User intent | Capability tool | Important boundary |
| --- | --- | --- |
| List configured channels | `list_message_channels` | Safe projection; no credentials or raw provider IDs |
| Create or patch a channel | `save_message_channel` | Requires idempotency key; write-only credentials |
| Check/set/replace/delete/authorize credential | `manage_channel_credential` | Exact channel ID; credential values are write-only |
| Connect/disconnect/diagnose | `operate_message_channel` | Normalized readiness is not proof of business recovery |
| Find sendable conversations | `list_channel_targets` | Use returned opaque `target_id` only |
| Read local channel log | `list_channel_messages` | Local log only, bounded and redacted |
| Send text/rich/files | `send_channel_messages` | Target must be resolved; batch uses idempotency key |
| Read or patch global channel settings | `manage_channel_settings` | Webhook and router secrets are write-only |
| Remove a channel | `delete_message_channel` | Exact channel and explicit host confirmation |

## Read and Resolve First

1. Search for `list message channels` and read the returned capability. Apply
   only safe filters that the current schema advertises, such as channel ID,
   type, enabled state, or runtime status.
2. Select the exact channel ID from the result. Do not infer an ID from a name,
   an external platform ID, or a previous session.
3. For sending or target-specific history, search/read `list channel targets`
   and preserve the returned opaque target ID. Only configured defaults and
   conversations that have interacted with iyw-claw are eligible targets.
4. For local history, use `list_channel_messages` with bounded time, direction,
   status, limit, and cursor filters. It does not fetch remote platform history;
   say so when the user asked for messages that may not be in the local log.

Do not send a message until both the channel and target are exact. If no target
is returned, report that the host has no safe send target; never manufacture a
provider conversation ID or silently send to a guessed default.

## Sending Messages

Use `send_channel_messages` with a caller-generated non-empty `request_id` for
the complete batch. Each item needs `channel_id` and `target_id` from the host;
the item can contain text or structured `rich` content, but not both. Files may
be readable absolute or working-directory-relative paths when the schema allows
them. Keep the message body bounded and do not put credentials, tokens, cookies,
provider headers, or private gateway details in text or rich fields.

Verify the per-item result. Unsupported attachments, partial batch rejection,
provider failure, queued delivery, or effect-unknown are not a successful send.
If a request is retried after an uncertain transport result, reuse the same
`request_id` only when the current result/schema says it is the idempotent retry
for that batch; do not create a second request ID that could duplicate a send.

## Channel Configuration

Use `save_message_channel` to create or patch a channel. Creating requires the
current channel type; patch only intended fields. Provider identifiers and
credentials supplied during bootstrap are write-only and must never be echoed,
included in audit text, or returned as user-facing evidence. The operation
executes immediately according to its current schema; verify the resulting
normalized channel projection.

Use `manage_channel_settings` for global routing and reporting settings. Read
with `operation: get` before changing unfamiliar values. Patch only the intended
fields, preserve unrelated settings, and treat webhook URLs, natural-router
keys, and similar credentials as write-only. A successful patch is not proof
that an external provider is reachable; use channel operation diagnostics when
connectivity matters.

## Credentials and QR Authorization

`manage_channel_credential` operates on one exact channel. Read `status` when
diagnosing, and use `set` or `replace` only with a credential the user supplied
or an approved setup flow. Credential values are write-only; never read them
back, print them, store them in ordinary arguments, or put them in a reason or
completion condition.

QR or device authorization is a two-stage flow: start authorization with the
host, preserve the returned opaque authorization ID, then check that exact ID.
Do not claim the channel is authorized because a QR was created; verify the
terminal authorization result. Credential deletion blocks for explicit user
confirmation. If authorization or deletion is canceled, report cancellation
rather than retrying with a guessed ID.

## Connect and Diagnose

Use `operate_message_channel` with an exact channel ID and caller-generated
`request_id` for `connect`, `disconnect`, `quick_check`, or `full_loop`. Read
the current channel first so the operation is not applied to a stale or wrong
record. Results intentionally contain normalized readiness and safe errors, not
raw provider data. `connected` or a successful quick check alone does not prove
that inbound events, target resolution, or outbound delivery works; use the
business-specific verification available in the result or a safe message-log
check.

## Deletion and Failure Handling

`delete_message_channel` requires an exact channel ID and request ID and waits
for explicit confirmation/cancellation in iyw-claw. Do not bypass that prompt,
repeat it under another ID, or claim deletion before the terminal result. The
host controls local related-record cleanup and retention.

For any operation, distinguish `available`, `disabled`, `queued`, `sent`,
`failed`, `canceled`, `rejected`, and effect-unknown states as returned. An
unknown channel, target, authorization ID, or request result is a concrete
limitation. Stop on schema rejection or unavailable capability; do not switch
namespaces or expose a raw provider fallback.
