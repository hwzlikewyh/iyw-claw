# Scheduled Task Artifacts and Awake Design

## Goal

Show artifacts produced by each scheduled-task run in both the run detail
timeline and the global run-history table. Keep a desktop host awake while at
least one enabled scheduled automation exists, so the in-process scheduler can
fire while the app is running.

## Approaches

Artifact placement options were per-run inline lists, a separate task-level
artifact section, or only the latest run's artifacts. Per-run lists are selected
because they preserve the link between output and execution in both existing
history views. Each list uses that run's `conversation_id` and the existing task
artifact API and preview.

For power management, adding a cross-platform dependency was considered against
using the operating-system mechanisms already available to the desktop. The
native mechanisms are selected to avoid changing dependencies or server
behavior: Windows uses `SetThreadExecutionState`, macOS uses `caffeinate`, and
Linux uses `systemd-inhibit`. Unsupported or unavailable mechanisms are logged
without preventing the automation engine from running.

## Architecture and behavior

- Add one reusable run-artifact view. It loads artifacts for a run's conversation
  and displays names and type icons inline. Selecting an item opens the existing
  task-artifact preview. It presents loading, retryable error, and empty states.
- Place the view below the summary in the run timeline and below the task name in
  each global history row. Runs without conversations have no artifact request.
- Add a desktop-only keep-awake controller to the automation engine driver. At
  startup, after task changes, and every five seconds, query whether any
  non-deleted, enabled automation uses the schedule trigger. Acquire an inhibitor
  when true and release it when
  false. Polling also reconciles changes made by another process. Controller
  teardown releases its native resource; OS process/thread cleanup also releases
  it on application exit. macOS monitors the client PID; Linux runs `cat` under
  `systemd-inhibit`, with the client holding its stdin pipe open. The pipe closes
  on normal exit or crash, ending the command and releasing the inhibitor.
- The server runtime does not acquire a host power inhibitor. Manual-only tasks
  do not inhibit sleep.

## Errors and resources

Artifact lookup failures remain local to the associated run row and can be
retried; they do not change run status. Native inhibitor startup or refresh
failures are logged and retried during the next reconciliation. Failure to
acquire an inhibitor does not abort scheduling. Child processes are terminated
and reaped when their guard is dropped.

## Verification

Run TypeScript type checking and ESLint on the changed frontend modules. Run a
desktop Rust check and a server-feature Rust check to verify platform gating.
Per repository guidance, do not run unit, integration, or end-to-end tests unless
the user explicitly requests them. Statically inspect the artifact lookup,
run-row rendering, enabled-schedule query, inhibitor transitions, and teardown.
