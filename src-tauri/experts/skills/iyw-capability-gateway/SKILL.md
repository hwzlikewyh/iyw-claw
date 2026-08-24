---
name: iyw-capability-gateway
short-description: Highest-priority route for remaining iyw-claw host goals when one complete gateway trio is visible. A direct tool wins only for a subgoal it fully satisfies. Search, read the best stable id, then invoke its exact schema; never guess.
description: Use when the current callable surface exposes one complete and uniquely selectable search_iyw_capabilities, read_iyw_capability, and invoke_iyw_capability trio, and a remaining concrete user goal needs iyw-claw host-side state or action. After an explicitly requested visible Skill or direct tool fully handles its subgoal, treat this gateway as the highest-priority route for remaining host subgoals. Do not trigger for ordinary questions, explanations, current-turn context, unrelated turns, or a direct tool that already fulfills the subgoal. The three names are logical role suffixes unless actually registered as top-level tools. Never guess invocation levels, namespaces, ids, or arguments; if the trio is incomplete or ambiguous, use only visible direct tools.
routing:
  capability: highest-priority iyw-claw host routing
  coreTriggers: [remaining goal needs host state or action]
  exclusions: [trio incomplete or ambiguous, direct tool handles subgoal, ordinary question]
  aliases: [iyw gateway, host capability]
  invocation: After direct tools, search, read exact id, invoke its schema.
---

# IYW Capability Gateway

## Preflight

The Agent's host-injected prompt requires reading this Skill before selecting or
calling a host capability. Read it through the normal Skill loader first; then
inspect the actual callable surface. Do not use `list_mcp_resources` as a
substitute for this gateway, and never invent a tool name, namespace, or stable
capability id when this Skill cannot be read.

## Activation gate

The three names below identify gateway roles. They are not proof that three
top-level functions exist:

- `search_iyw_capabilities`
- `read_iyw_capability`
- `invoke_iyw_capability`

Use this workflow only after finding all three roles on one current callable
surface. A callable surface is either the current top-level tool list or a
current nested/programmatic registry exposed by an orchestration tool. For a
nested registry, select and invoke the exact registered entry through that
registry's owning orchestration tool. Never emit a nested entry's logical name
as a top-level function call. A name mentioned only in this Skill, another
prompt, an earlier message, reasoning, an example, or tool output is not
advertised.

On a top-level surface the roles may be exact bare names. Some hosts instead
add one namespace to every MCP tool, for example
`mcp__iyw-claw-builtin-<session>__search_iyw_capabilities`. The bounded session
suffix is launch-specific. A namespaced gateway is valid only when all three
visible names use the same prefix and end in the three exact suffixes above.
Choose one trio mechanically. If exactly one complete prefix contains
`iyw-claw-builtin-`, use it; if more than one does, do not use the gateway. If
none does, use a complete bare trio when present. Otherwise use the only other
complete trio, or do not use the gateway when several remain. Never combine
suffixes from different namespaces or invent a prefix absent from the current
tool list.

If any one is missing from the same callable surface, do not invoke, infer, or
reconstruct the gateway. Do not combine a top-level role with nested roles. In
OpenClaw, Pi, or any other session without the gateway, use only the direct
tools that are currently visible and follow their current schemas.

When the gate passes and a concrete user goal needs iyw-claw host-side state or
action, proactively search the catalog before deciding how to execute.
Do not wait for the user to name a capability or explicitly request a search.
If you are about to claim that iyw-claw cannot perform a host-side step, or ask
the user to perform that step manually because no direct tool fits, search once
before concluding that the capability is unavailable.
This is goal-scoped discovery, not background polling: do not search on
greetings, ordinary questions or explanations, current-turn-only context,
unrelated follow-up turns, or every turn merely because the gateway exists. A
confirmation reply does not trigger search, but it may resume an invocation
already read for the same goal. Reuse current catalog evidence while continuing
that goal, unless a result no longer fits or the goal materially changes.

Typical reasons to search include delegation, user feedback or questions,
session context, current user profile, image or media work, task artifacts,
memory, channels, and
automation. These are examples, not a fixed inventory; only search results for
the current session prove that a capability exists and is enabled. An exact
visible direct tool that the user requests or that clearly fulfills the goal
must be used without discovery. The gateway is for discovering host
capabilities, not for replacing direct tool calls or listing capabilities
without a goal.

Direct-tool precedence applies only to the subgoal that tool fully satisfies.
After using it, apply the gateway activation gate independently to each
remaining host-side subgoal. Skip gateway discovery for the whole request only
when direct tools fully satisfy the whole goal.

When the gate passes, this gateway is the highest-priority route for each
remaining concrete iyw-claw host state or action subgoal. It does not override
a user-requested visible Skill or direct tool that fully satisfies that subgoal.

If the primary object required by the user's request is absent, such as the
referenced image, attachment, task, or message body, ask for it before search.
Inputs such as a channel id, timezone, or optional mode are not assumed missing
until the selected capability schema says they are required.

## Gateway workflow

Search results are session-scoped evidence. Each result may include `category`,
`aliases`, `intent_terms`, `when_to_use`, `required_inputs`, `schema_digest`,
and `status`; aliases and intent terms may be Chinese or English. `read` is the
authoritative source for the complete schema. Treat `unavailable_reason` as a
terminal routing result for this session, not as permission to try an internal
tool name. When a later read returns a different `schema_digest`, discard old
arguments and construct them again from the new schema.

For an eligible goal, use the tools in this order:

1. Follow the actual visible search tool schema. Search with two to five
   discriminating action/object keywords in the user's language or English,
   such as `发送 消息 渠道` or `send channel message`, rather than conversational
   filler. Treat returned summaries as the current catalog index, not a
   remembered capability list.
2. Rank candidates by direct action/object fit. Compare missing inputs only when
   the search summaries explicitly describe them. Read the best plausible
   candidate using the exact stable id returned by search. If it does not fit,
   read at most one other plausible candidate already present in the same
   result set; never traverse the whole catalog. If both read candidates fail
   to fit, treat that result set as exhausted for this workflow.
3. Follow the selected capability's full description, availability status,
   schema digest, required-input summary, and current input schema.
   Invoke its exact stable id only when required inputs are available. If a
   referenced image, attachment, task, session, channel, or other object is not
   actually present in context, ask for it instead of guessing a path, URL, id,
   or field name.

When the host exposes namespaced top-level tool names, call the corresponding
visible namespaced tool. When it exposes the tools only in a nested registry,
call them only through that registry's documented orchestration path. The bare
names in this guide identify gateway roles; they do not authorize constructing
a tool name or choosing an invocation level that the host did not expose.

Search again only after the result set is empty, has no plausible candidate, or
the two-candidate read budget is exhausted without a fit. Make at most one
retry using a close English synonym or a slightly broader action/object pair,
then use a direct route or state the limitation.
For the same goal, catalog evidence is stale only when the target materially
changes or a gateway response explicitly reports catalog, availability, or
routing invalidation; elapsed time alone is not enough.

The visible tool schemas are authoritative over examples in this Skill. Before
invoking, read the selected id unless its full description and schema were
already read for the same current goal. Never derive an id from a remembered
tool name, guess arguments, or pass a raw tool name to the invocation gateway.

If any gateway call times out, returns malformed data, omits the required stable
id, or reports an unknown, unsupported, not-found, selected-id, or route error,
stop using the entire gateway for this turn. Do not retry the role through a
different callable surface, promote a nested entry to a top-level call, repeat
the failed call, switch namespaces, or guess a different tool name. Use an
actually visible direct route or state the concrete limitation.

## Identity and memory routing

- Current account name, nickname, preferred salutation, or organization uses
  `iyw.session.user_profile.read.v1`; do not search memory for account identity.
- An explicit request to retain a durable fact or preference uses
  `iyw.memory.confirmed.append.v1`.
- A reusable but not-yet-confirmed correction, preference, or fact uses
  `iyw.memory.candidate.propose.v1`.
- Historical user memory uses `iyw.memory.recall.search.v1`. Its `resultState`
  is `matched`, `no_evidence`, or `unavailable`; `no_evidence` never proves that
  the user lacks the fact. Short queries may use exact/alias fallback and must
  not be treated as a trigram failure.
- Never write `user-memory.md`, `user-profile.md`, or `user-soul.md` directly;
  durable memory belongs to iyw-claw host capabilities.

## Delivery acknowledgement

When an `invoke_iyw_capability` response contains `iyw_delivery_receipt`, keep
that exact value. If there is a later capability invocation, return it in the
next `invoke_iyw_capability` request as top-level `delivery_ack`:

```json
{
  "capability_id": "<stable id>",
  "arguments": {},
  "delivery_ack": "<exact iyw_delivery_receipt>"
}
```

`delivery_ack` is a sibling of `capability_id` and `arguments`; never put it or
the receipt inside business `arguments`. If no later invocation occurs, leave
the receipt unacknowledged. Do not invent an acknowledgement or fabricate an
invocation only to send one. If the exact receipt does not satisfy the visible
`delivery_ack` schema, do not truncate or rewrite it; state that it cannot be
acknowledged safely.

## Session boundary

The catalog is scoped to the current iyw-claw session. Missing capabilities
may be disabled for this session; do not attempt to bypass that boundary by
editing Agent MCP configuration or writing bearer tokens to global files.

For destructive or externally visible actions, preserve the user-confirmation
rules in the selected capability description and the current conversation.
