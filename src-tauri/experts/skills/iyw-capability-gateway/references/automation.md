# Scheduled Automation

Load this reference for creating, listing, updating, disabling, or removing
iyw-claw scheduled tasks. The live catalog and current schemas own the exact
Agent enum, field names, limits, and availability.

## Operation Map

| User intent | Capability tool | Required evidence |
| --- | --- | --- |
| Find selectable projects | `list_scheduled_task_projects` | Safe project ID/name list |
| Inspect tasks | `list_scheduled_tasks` | Exact task ID or filtered task list |
| Create a task | `create_scheduled_task` | Name, prompt, five-field cron |
| Change a task | `update_scheduled_task` | Fresh task ID and current details |
| Remove a task | `delete_scheduled_task` | Exact task ID and terminal result |

## Decide Before Calling

Do not create an automation from a vague request. Establish the task's intent,
frequency, timezone, target project, executor Agent, prompt, and enabled state.
Ask one necessary user question when a missing choice materially changes what
will run; do not ask for confirmation merely to create an obvious task when the
schema says the operation executes immediately.

If a project is named, list task projects first. Select an exact `project_id`
from that result, or use a unique project name/path only when the current schema
allows it. Never send both `project` and `project_id`. Omitting both creates or
uses the dedicated persistent folder described by the schema; do not imply that
it is the user's normal workspace without checking the result.

## Create

`create_scheduled_task` requires a non-empty display `name`, a non-empty
execution `prompt`, and a five-field cron expression in this order:

```text
minute hour day-of-month month day-of-week
```

Set an IANA `timezone` explicitly when the user means local time; the schema
defaults to UTC. Choose `agent_type` only from the current returned enum; the
current Agent is the default when omitted. `enabled` controls whether scheduling
starts immediately and defaults to true. Keep prompts self-contained, safe to
run repeatedly, and free of credentials, tokens, cookies, or untrusted shell
fragments.

The operation executes immediately according to the current schema. Verify the
returned task ID, cron, timezone, project association, executor, and enabled
state. Do not claim the task has run yet merely because it was created; creation
and first execution are different results.

## List and Inspect

Use `list_scheduled_tasks` for a read-only global task view. Omit filters to
inspect all active tasks, or pass the exact task ID. Project filters accept the
schema's project form but are mutually exclusive; enabled and Agent filters are
optional. Treat a zero-result list as “no matching task was returned,” not proof
that a task never existed in retained history.

When changing or deleting, identify the exact integer task ID from the current
list. Do not select by fuzzy name, stale session memory, or a previously seen
ID without a fresh read.

## Update

`update_scheduled_task` is a patch operation. Read the task first and send only
the intended fields inside `patch`; unspecified fields are preserved. Project
changes still cannot contain both project selectors. Re-check the returned
record after the patch and report the fields that actually changed. A schema
rejection means no valid update was established; do not retry with guessed field
names or a full replacement payload.

## Delete and Disable Semantics

`delete_scheduled_task` takes one exact integer task ID. The current host
disables and soft-deletes the task while retaining run history. It executes
immediately without a separate confirmation prompt in the current schema; do
not claim a confirmation is pending unless the live read says otherwise. Verify
the terminal result and distinguish disabled/deleted from failed or unavailable.

If the user only wants to pause future runs, prefer the update patch with
`enabled: false` when that is the current schema's intended operation. Do not
delete a task to pause it when preserving the task record is requested.

## Failure and Safety Rules

Stop when the project list, task ID, Agent enum, cron, or timezone is ambiguous.
Never embed secrets or private paths in a scheduled prompt. Do not infer
successful execution from task creation, an enabled flag, or a stale list. For
timeouts or effect-unknown results, read the exact task ID once before deciding
whether the state is known; do not create a duplicate task.
