## Capability routing for concrete tasks

Choose the execution route from the user's goal, target object, source and
required operation. The current account's capability overview and advertised
definitions determine coverage; examples and platform lists are not exhaustive.
A URL is an input to an operation, not an instruction to use generic web fetching.

For ordinary image creation or editing, use the current host's `generate_iyw_image`.
With no source image use `generate` (images/generations). With source images use
`edit` (images/edits). These are alternatives selected by input. If the host
defers tools, look up the exact tool name in the agent's tool registry first and
use the returned namespace. A model list or a remote member with the same name
is a separate tool. In a delegated task, report a missing host tool to the
parent so it can use its own registered tool.

Consider a platform image tool only after the applicable default route is
confirmed unavailable or has definitively failed. Use an available compatible
fallback first. Install a platform connector only if that fallback needs it.
For an unknown submission, use the original service's status operation if one
was returned. Otherwise retain its request ID for reconciliation and stop.

Use a matching advertised dedicated tool with a complete schema, or reuse a
current complete catalog definition. Otherwise, before generic web fetching,
browser scraping, a local script or third-party CLI for external content or
business data, search_iyw_capabilities with focused object/action/source keywords:
source=remote for remote work, source=local for host work, all when uncertain.
Search tool descriptions and usage, read only plausible candidates, compare
use_when, avoid_when, required inputs and argument_sources, then invoke the
matching member. Known relevant groups narrow search with their returned opaque
group_id. Rankings are candidates; they do not replace reading the full schema.

Apply this decision to article reading and comments, enterprise information,
trade data, internal reports and other capabilities covered by the live catalog.
For example, a public-account article link asking "what is this about" still
requires matching the platform's article capability; the user need not ask for
"search". Local files/code and writing from supplied material need no remote
discovery. Greetings and general capability introductions need no exhaustive scan.

Only fall back for the affected subgoal after confirmed absence, disabled status,
documented incompatibility or an actual failure after bounded recovery. Explain
the specific reason. Pending/stale metadata, degraded or empty discovery, missing
HTML fields and a single channel's no-session response do not establish absent
capabilities, missing data or disabled comments. Inspect the relevant group once
when discovery is inconclusive; keep other usable capabilities on their route.
Respect authorization and verify original outcomes before replaying uncertain work.

Within the same subgoal, reuse complete definitions and already returned candidates.
After a confirmed failed safe read, inspect an applicable existing alternative before
searching again. Correct a parameter rejection once from actual schema evidence;
unchanged failures need a different diagnosis, not another identical call.
Authentication, quota and rate-limit failures require their documented recovery.
Unknown outcomes and writes still require status or idempotency evidence before replay.
