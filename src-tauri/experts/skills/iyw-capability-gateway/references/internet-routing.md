# Internet Routing (Agent Reach Integration)

This reference absorbs the useful behavior of the local `agent-reach` Skill
without importing its fixed CLI paths, external workspace assumptions, or
credentials. It is a routing guide; report writing and synthesis belong to
`research-workflow.md`.

## Contents

- [Triggers and preflight](#triggers-and-preflight)
- [Platform routing](#platform-routing)
- [Fallback and evidence](#fallback-and-evidence)
- [Privacy and workspace](#privacy-and-workspace)

## Triggers and preflight

Use this route when the user asks to search/research/look up anything online,
mentions a URL, or names one of these categories: web/RSS, GitHub/code, X/Twitter,
小红书, Bilibili, V2EX, Reddit, LinkedIn/jobs, YouTube, 小宇宙/podcast, finance,
or public discussions.

Before a multi-backend platform operation, run the currently installed
`agent-reach doctor --json` only if that executable is actually available and
the user asked for that external route. Prefer the current iyw gateway catalog
and unified browser capability. Never claim a doctor result from memory or infer an
`active_backend` that was not observed.

Announce the active route briefly when it matters (for example, “使用统一浏览器
路由读取公开页面”); do not expose cookies, headers, keys, or internal
transport details.

## Platform routing

| User intent | Preferred current route | Important behavior |
| --- | --- | --- |
| General web/search | Gateway search capability, then managed browser read | Search multiple variants; snippets are leads only. |
| GitHub/repository/code/Issue/PR | Discovered GitHub/code capability or managed browser | Pin owner/repo/number/branch; verify the returned URL and state. |
| X/Twitter | Managed browser or discovered platform capability | Search may be unstable; use one documented retry then a stable feed/user/article route. |
| 小红书/XHS | Managed browser or discovered platform capability | Search/feed first; read using the complete returned URL/token, never a bare note id. |
| Bilibili | Bilibili-capable route or managed browser | Do not use YouTube `yt-dlp` logic for Bilibili; use a supported video/search/subtitle route. |
| V2EX | Public API/browser route if currently advertised | Preserve topic/node identifiers and distinguish replies from the topic body. |
| Reddit | Managed browser or discovered logged-in route | Login-backed; do not invent anonymous API access. |
| LinkedIn/jobs | Managed browser or discovered logged-in route | Treat profile/job pages as authentication-bound and verify visible evidence. |
| YouTube | Discovered video/subtitle/audio route or managed browser | Prefer subtitles; if absent, use the host's supported audio transcription route. |
| 小宇宙/podcast | Discovered podcast/transcription route or managed browser | Keep transcript provenance and label machine transcription uncertainty. |
| RSS/news/finance | Discovered feed/search route or managed browser | Record feed URL, item date, and access time; do not treat stale items as current. |

The source Skill's backend examples (Exa, OpenCLI, `bili`, `rdt`, `gh`, Jina,
`yt-dlp`, `feedparser`) are optional implementation hints only. Use them only
when the current environment advertises the exact command/tool and the current
Skill permits it. Do not install packages, configure cookies, or switch browsers
just because a preferred backend is absent.

## Fallback and evidence

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
