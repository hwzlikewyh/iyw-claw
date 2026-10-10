# Internet Routing (Agent Reach Integration)

This reference absorbs the useful behavior of the local `agent-reach` Skill
without importing its fixed CLI paths, external workspace assumptions, or
credentials. It is a routing guide; report writing and synthesis belong to
`research-workflow.md`.

## Contents

- [Triggers and preflight](#triggers-and-preflight)
- [Remote social priority](#remote-social-priority)
- [Platform routing](#platform-routing)
- [Fallback and evidence](#fallback-and-evidence)
- [Privacy and workspace](#privacy-and-workspace)

## Triggers and preflight

Use this route when the task requires retrieving online information from a URL
or one of these categories: web/RSS, GitHub/code, X/Twitter,
小红书, Bilibili, V2EX, Reddit, LinkedIn/jobs, YouTube, 小宇宙/podcast, finance,
or public discussions. Mentioning a platform or URL in supplied content does not
by itself require online retrieval or a browser. Local analysis and writing from
provided material can proceed directly.

## Remote social priority

Social lookup and all-web research with social sources use the signed-in remote
social catalog before local Agent Reach, OpenCLI, platform CLIs or browser
scraping. This includes 小红书、抖音、微信视频号 and merchant/hospital news,
activity, promotions and group-buying. A generic direct-tool or domain-Skill
preference does not override this rule.

1. Reuse a current matching remote definition, or locate the platform group with
   `search_iyw_capabilities(source=remote)` and its exact name, such as “小红书”,
   “抖音” or “微信”. Use focused action keywords only when further discovery is
   needed. Broad “抖音 搜索” candidates can be music, challenges or Demo tools;
   read the platform group to select an actual content search member. This finds
   tools, not posts; do not paste the complete research brief into discovery.
2. Read the exact returned opaque `capability_id` of the relevant group/member.
   A social parent group contains platform groups; read the relevant child group
   to obtain callable members. Invoke only a member's returned `capability_id`
   with schema-matching business arguments. Never pass raw `social-channels`
   or `tikhub-*` IDs to the host wrapper or infer opaque IDs from names.
3. Search actual content using the hospital/merchant name, city and topic as
   business arguments. Follow genuine content IDs, tokens and cursors for
   detail reads. 微信视频号 is WeChat Channels, not the user's messaging channel
   or only a public-account article. Verify the selected member's actual scope.
4. Supplement official sites, professional and local communities with suitable
   search/content readers. Evaluate coverage separately for every requested
   source family; unsupported local forums do not make remote social tools
   unavailable.

Fallback requires confirmed missing/disabled capability, documented mismatch,
or an actual remote failure after bounded recovery. Explain the affected
platform and reason before its authorized local route; continue other usable
platforms remotely. Pending/stale metadata, degraded semantic search and an
empty discovery result do not prove absence: try one focused synonym or browse
the relevant group without the semantic index. Zero matching posts is a content
result, not evidence of a missing tool. Business unauthorized/401/403 or 402
is failure even with HTTP 200; respect authentication, payment and rate limits.
Check uncertain outcomes before any replay. When no host gateway is connected,
use an actually advertised remote MCP trio with its own current schemas/IDs;
if no remote route is available, state that before configured local fallback.

### Local fallback preflight

When selecting an unknown backend or diagnosing its availability, discover and invoke
`iyw.internet.agent_reach.status.v1` through the current gateway when available.
It returns observed channel health and active backends from the managed Agent
Reach installation; it does not install tools or import credentials. Otherwise,
run the currently installed
`agent-reach doctor --json` only if that executable is actually available and
the user asked for that external route. Prefer the current iyw gateway catalog
and use an already known suitable direct route without repeating health discovery.
Run this local health check only after choosing a justified local fallback;
it is not a prerequisite for remote social discovery or execution.
Public content uses search/read tools first; browser use follows the conditions
in `browser-and-media.md`. Never claim a doctor result from memory or infer an
`active_backend` that was not observed.

Announce the active route briefly when it matters (for example, “使用统一浏览器
路由读取公开页面”); do not expose cookies, headers, keys, or internal
transport details.

## Platform routing

| User intent | Preferred current route | Important behavior |
| --- | --- | --- |
| General web/search | Available search capability, then content reader; browser only for required interaction or unreadable dynamic content | Match search depth to the task; snippets are leads only. |
| GitHub/repository/code/Issue/PR | Discovered GitHub/code capability or managed browser | Pin owner/repo/number/branch; verify the returned URL and state. |
| 抖音/Douyin | Remote social platform group/member first; authorized local route only after evidenced fallback | Search/list content then read real returned video/user IDs; retain dates and offer validity. |
| 微信视频号/WeChat Channels | Remote WeChat members whose schema explicitly covers Channels first | Distinguish video search/detail from public-account articles and host messaging channels. |
| X/Twitter | Remote social member first; authorized local route after evidenced fallback | Search may be unstable; use one documented retry then a stable feed/user/article route. |
| 小红书/XHS | Remote social search/list/detail first; authorized local route after evidenced fallback | Follow the selected schema: App may accept note_id/share_text; Web V3 requires corresponding note_id and xsec_token. Never invent tokens. |
| Bilibili | Remote social member first; authorized local route after evidenced fallback | Do not use YouTube `yt-dlp` logic for Bilibili; use a supported video/search/subtitle route. |
| V2EX | Public API/browser route if currently advertised | Preserve topic/node identifiers and distinguish replies from the topic body. |
| Reddit | Remote social member first; authorized local route after evidenced fallback | Respect the selected remote endpoint's authentication; do not infer local cookies are required by a remote API. |
| LinkedIn/jobs | Remote social member first; authorized local route after evidenced fallback | Verify returned profile/job evidence and the actual endpoint's authentication boundary. |
| YouTube | Remote social video/subtitle member first; justified local fallback | Prefer subtitles; if absent, use the host's supported audio transcription route. |
| 小宇宙/podcast | Discovered podcast/transcription route or managed browser | Keep transcript provenance and label machine transcription uncertainty. |
| RSS/news/finance | Discovered feed/search route or managed browser | Record feed URL, item date, and access time; do not treat stale items as current. |

The source Skill's backend examples (Exa, OpenCLI, `bili`, `rdt`, `gh`, Jina,
`yt-dlp`, `feedparser`) are optional implementation hints only. Use them only
when the current environment advertises the exact command/tool and the current
Skill permits it. Do not install packages, configure cookies, or switch browsers
just because a preferred backend is absent.

## Fallback and evidence

Apply the remote priority and fallback conditions above before this local
backend recovery chain. Never switch the whole research task to local tools
because one requested platform is unsupported or fails.

Use one bounded recovery chain per platform: refresh state/doctor, retry the same
route once where documented, then choose one verified alternative. Stop on an
unknown command, missing capability, authentication boundary, rate limit, or
malformed result rather than guessing namespaces or cycling selectors.

For every source, retain URL/title/date/publisher and the claim it supports.
Deduplicate canonical URLs across platforms. Mark paywalls, login-only pages,
truncated content, translations, search snippets, and user-generated discussion
as limitations. Platform restrictions are evidence gaps, not permission to
fabricate content.

Do not perform posting, commenting, liking, following, repository writes, or
other social/developer write operations unless a separate user-approved Skill
and current capability explicitly cover that write. This gateway route is
read/research oriented.

## Privacy and workspace

Keep temporary fetches, transcripts, screenshots, and raw search output outside
the repository when possible (use the host-approved temporary directory). Never
place cookies, auth profiles, API keys, or tokens in workspace files, prompts,
logs, reports, or Artifacts. Do not return a private browser storage dump.

After a large external research task, an installed `agent-reach check-update`
may be run only when that command is actually available; report a new version as
an optional note and never interrupt the task to update it.
