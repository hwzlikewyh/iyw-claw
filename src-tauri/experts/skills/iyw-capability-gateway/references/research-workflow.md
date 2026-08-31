# Deep Research Workflow

Use this workflow when a user asks for research, comparison, investigation,
current events, market intelligence, or a cited report.

## Contents

- [1. Define the brief](#1-define-the-brief)
- [2. Plan queries](#2-plan-queries)
- [3. Collect sources](#3-collect-sources)
- [4. Deep-read and verify](#4-deep-read-and-verify)
- [5. Synthesize](#5-synthesize)
- [6. Deliver](#6-deliver)

## 1. Define the brief

Identify whether the user needs learning, a decision, a written deliverable,
or current monitoring. Ask at most the minimum question needed for a materially
different outcome; otherwise use reasonable defaults and state the assumed
recency, geography, audience, and depth.

Do not confuse research with writing-only transformation. Research requires
external evidence; a supplied document can be summarized without web search.

## 2. Plan queries

Break the topic into 3–5 answerable sub-questions. For each, prepare 2–3 query
variants covering terminology, counterarguments, geography/date, and the user's
decision criteria. For current events, include a news/current-date variant.

Use the gateway's live catalog to find available search, browser, code, video,
or platform capabilities. The top-level MCP still has only the gateway trio.
Read the returned schema before every distinct capability family.

## 3. Collect sources

Search each sub-question, combine multiple sources, and deduplicate by canonical
URL. Maintain a source ledger with:

```text
source_id | title | url | publisher/type | publication date | accessed date
sub-question | claims supported | limitations | access status
```

Prefer primary/official documentation, standards, academic work, filings,
datasets, and reputable reporting. Use forums/social posts for lived experience
or discussion signals, not as unqualified proof. Keep search snippets as leads,
not evidence. Aim for breadth first, then select 3–5 strongest sources for deep
reading; the correct count depends on scope, not a fabricated quota.

Use managed browser first for pages and public web data. Start with existing
tabs, navigate with a discovered `iyw.browser.*` capability, take a fresh
snapshot/read after navigation or DOM changes, and verify URL/title/text. For a
dynamic or authenticated page, use the managed profile; request human action
only for login/MFA/CAPTCHA/payment or another human-only step.

## 4. Deep-read and verify

Read the full relevant sections of selected sources, not only result snippets.
For each material claim, record the exact supporting source and whether it is
direct evidence, an estimate, a source's opinion, or an inference. Cross-check
important numbers and dates with an independent source. If sources disagree,
show both positions and explain the likely reason (definition, date, sample,
method, or geography).

Mark a single-source claim as single-source or unverified. If a sub-question has
no credible evidence, write “insufficient data found” and describe what was
checked. Never fill a gap with plausible-sounding detail.

## 5. Synthesize

Use this structure unless the user requests another format:

```markdown
# [Topic]: Research Report
*Generated: [date] | Sources: [N] | Confidence: [High/Medium/Low]*

## Executive Summary
## 1. [Theme]
## 2. [Theme]
## 3. [Theme]
## Key Takeaways
## Sources
## Methodology
## Limitations and Conflicts
```

Put citations next to claims, not in an untraceable source dump. Every material
assertion must have a source or an explicit label such as inference,
single-source, or insufficient data. Keep conclusions proportional to the
evidence and separate facts, interpretations, and recommendations.

## 6. Deliver

For a short answer, provide the report in chat with citations. For a long report,
write a Markdown or JSON file in the task-approved workspace/output location,
then register the final file through `iyw.artifacts.present.v1`. Do not register
source files, caches, logs, temporary fetches, or internal notes. A URL or preview
alone is not proof of Artifact registration.

If the report is HTML or Markdown and embeds images, prefer the validated
`iyw-image-workflows` upload path for new/local images and write only the
verified public HTTPS URL returned after the upload/check. Do not embed
presigned URLs, temporary signed query URLs, or local absolute paths. Respect
privacy and local-only requirements; if hosting is unavailable, use a valid
workspace-relative fallback and state the limitation.

Do not automatically save the research topic or conclusions to user memory.
Only use memory MCP when the user explicitly asks to remember a durable personal
preference/fact or when the user confirms a reusable preference discovered in the
conversation.

## Recovery

- Empty/weak search: revise one query using a close synonym, then stop if still empty.
- Stale browser reference: take one fresh snapshot and retry the same action once.
- Dynamic/auth boundary: use managed browser or the human-action capability.
- Rate limit/backend failure: follow the platform-specific retry chain in
  `internet-routing.md`; never cycle guessed commands.
- Unverified result: report the gap instead of claiming completion.
