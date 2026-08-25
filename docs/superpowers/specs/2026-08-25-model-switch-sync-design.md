# Model Switch And Catalog Synchronization

## Goal

Keep the model selector truthful across the Fusion catalog, Agent-side model
catalog, ACP session state, and the model used by the next turn. A model change
must never be shown as applied before the Agent acknowledges it.

## Design

1. Filtered `/v1/models?sdk_id=...` responses are merged into the Rust model
   catalog without replacing the complete catalog. This lets a newly exposed
   Agent-compatible model reach the next Agent spawn while preserving models
   learned from the complete catalog.
2. `session/set_config_option` receives a one-shot acknowledgement through the
   ACP command channel. The command resolves only after the Agent response has
   been parsed; Agent errors are returned to the caller and still emitted as a
   recoverable session error.
3. The chat panel does not send configuration changes while a turn is running.
   If an idle connection does not advertise the selected model, it performs one
   guarded same-session reconnect. The fresh Agent process reads the updated
   native model projection and re-advertises selectors. If the model is still
   unavailable, the selection rolls back to the Agent-reported current value.
4. Existing queue, prompt, viewer, and snapshot flows remain unchanged. Model
   preferences are persisted only as the desired value; failed recovery removes
   the unsupported value and restores the confirmed live value.

## Invariants

- A complete catalog fetch replaces the complete Rust catalog; a filtered fetch
  only adds or refreshes entries and never deletes unrelated entries.
- The frontend never treats command-channel enqueue as model-switch success.
- No model reconnect is attempted during `prompting`, for viewers, or for
  delegation children.
- Same-session reconnect preserves the external session ID and conversation
  history.
- Existing user-controlled model choice is preserved when new catalog entries
  arrive; catalog refresh never auto-switches the active model.

## Validation

- Static review of the frontend -> transport -> ACP command -> Agent response
  chain for both idle and prompting states.
- Focused TypeScript lint/type validation and Rust formatting/check where
  available. The repository's guidance does not permit adding or running test
  suites by default, so runtime switching remains a validation gap.
