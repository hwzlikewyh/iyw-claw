---
name: agent-browser
description: Use the iyw-claw managed agent-browser first for every web page and public web-data task. Navigate, inspect, extract, interact, verify, and request user takeover only for human-only steps.
routing:
  capability: managed agent-browser
  coreTriggers: [browser, web page, public data, website]
  exclusions: [desktop app control]
  aliases: [agent browser, managed browser, 浏览器自动化, 网页数据]
  invocation: Use the managed browser first; read this Skill for advanced commands and recovery.
---

# Agent Browser

Use the iyw-claw managed `agent-browser` runtime as the first browser and web
data route. The host pins every operation to a managed tab, preserves the
user-visible profile and sign-in state, and verifies the bundled controller
before starting it. Do not install another browser, edit an Agent's MCP
configuration, or switch to an external browser while the managed route is
available.

## Browser-first policy

For every request that needs a web page or public web data:

1. Use the available managed browser capability surface first. If the live
   gateway is present, use the exact `iyw.browser.*` stable capability returned
   by `search -> read`; never guess a capability id or namespace.
2. Start with `browser_list_tabs` when a tab may already exist. Reuse the active
   tab unless the user explicitly asks for another tab.
3. Use `browser_open` for HTTP/HTTPS navigation, then `browser_snapshot` or
   `browser_read`/`browser_command` to inspect the page.
4. Use fresh snapshot references for `click`, `fill`, and other interactions.
   After navigation, route changes, popups, material DOM changes, or writes,
   take another snapshot before using a reference.
5. Verify the requested result using page text, URL, title, a stable element,
   a downloaded file, or another explicit success signal. A successful click
   alone is not proof that the business action completed.
6. Only after one state check shows that the managed runtime/session/daemon is
   unavailable, or the page cannot provide the requested data, may you use an
   actually installed alternative route. Read its current Skill first; never
   switch merely because a selector needs correction.

Prefer a reliable API or direct data source only when it is already available
and clearly satisfies the request. If it does not return data, is incomplete,
requires browser authentication, or the page is dynamically rendered, return
to the managed browser before reporting a missing result.

## Human takeover

Use `iyw.browser.user_action.request.v1` only for a step that must be performed
by a person: login requiring user credentials, MFA/OTP, CAPTCHA, device
approval, secure payment confirmation, drag-and-drop that cannot be automated,
or an explicit final human review. The host opens the managed tab in a visible
window and pauses Agent control until the user's meaningful input settles.

Do not ask the user to take over for an ordinary selector error, stale `@eN`
reference, slow page, missing wait, or a normal data extraction problem. First
take one fresh snapshot, correct the locator, or wait for a bounded condition.
Never put passwords, tokens, cookies, private keys, or one-time codes in a
reason or completion condition.

## Core workflow

```text
browser_list_tabs
  -> browser_open (reuse active tab by default)
  -> browser_snapshot (fresh @eN refs)
  -> browser_click / browser_fill / browser_press / browser_scroll / browser_command
  -> browser_snapshot again
  -> verify URL, title, text, element, download, or structured result
```

The direct managed tools use structured arguments. `browser_command` is the
managed advanced surface for the additional `agent-browser` commands listed
below; it still runs with the current tab's CDP endpoint and strict pinning.

## Navigation and inspection

Available operations include:

```text
open <url>                 back
forward                    reload
snapshot [-i] [-c] [-d N] [-s <selector>]
read [url]                 get text|html|value|attr|title|url|count|box|styles
is visible|enabled|checked <selector>
```

Use `snapshot -i` for interactive controls, `snapshot -c` for compact output,
and `snapshot -s` to scope a large page. `read` prefers text/Markdown and is
useful for public pages that do not need interaction. Use `get html` only when
the structure is needed; do not expose hidden credentials or full page dumps
in the user-facing response.

## Interactions

```text
click <ref-or-selector>       dblclick <ref-or-selector>
focus <ref-or-selector>       hover <ref-or-selector>
fill <ref-or-selector> <text> type <ref-or-selector> <text>
press <key>                   keydown <key>       keyup <key>
check <ref-or-selector>       uncheck <ref-or-selector>
select <ref-or-selector> <value>   scroll <up|down|left|right> [pixels]
scrollintoview <ref-or-selector>  drag <source> <target>
upload <ref-or-selector> <path>    download <ref-or-selector> <path>
```

Use `fill` when existing text must be cleared. Use `type` only when preserving
existing text is intentional. Uploads and downloads remain inside the
iyw-claw-managed directories; do not use arbitrary paths outside the current
task's approved workspace or managed download directory.

Semantic locators are available through `browser_command`:

```text
find role button click --name "Submit"
find text "Sign In" click
find label "Email" fill "user@example.com"
find first ".item" click
find nth 2 "a" text
```

Use a fresh snapshot or a unique semantic locator. Do not cycle through many
selectors after a failure.

## Waiting and verification

```text
wait <ref-or-selector>        wait <milliseconds>
wait --text "Success"         wait --url "/dashboard"
wait --load networkidle       wait --fn "window.ready"
```

Prefer a selector, text, URL, or load-state condition over arbitrary sleeps.
Waits are bounded by the host. After a material page change, snapshot again.

## Screenshots and PDF

```text
screenshot [path] [--full] [--annotate]
pdf <path>
```

Use screenshots for visual verification and `analyze_image` when the task needs
image understanding. Present a completed page or visual result with
`iyw.browser.window.present.v1`; close only the detached display window with
`iyw.browser.window.close.v1` when it is no longer needed.

## Tabs, frames, dialogs, and JavaScript

```text
browser_list_tabs            browser_open {new_tab:true}
browser_close_tab            browser_present / browser_close_window
frame <selector>             frame main
dialog accept [text]         dialog dismiss
eval <javascript>
```

Frame and dialog state is page-specific. Re-snapshot after switching frames or
after a dialog changes the page. JavaScript is for bounded page inspection or
interaction needed by the task; do not use it to exfiltrate cookies, tokens, or
hidden credentials.

## Browser settings and device emulation

```text
set viewport <width> <height>      set device <name>
set geo <latitude> <longitude>     set media dark|light
tap <ref-or-selector>              swipe <direction>
```

Network headers, HTTP basic credentials, and offline mode are restricted host
operations. Use them only when the live schema explicitly advertises the
operation and the user supplied the required non-secret configuration through
an approved path. Never print credentials.

## Network and state

The full `agent-browser` CLI also supports network interception, request
inspection, HAR capture, cookies, local/session storage, saved state, auth
profiles, Chrome profiles, and isolated sessions:

```text
network route|unroute|requests|har
cookies get|set|clear             storage local|session
state save|load                   session list|info
auth save|login|list|show|delete  profiles
```

These commands are subject to the managed host's allowlist and privacy policy.
Treat cookies, storage, auth profiles, state files, headers, and credentials as
secrets. Do not read, export, or report them unless the current host schema
explicitly authorizes the exact operation for the user's task.

## Debugging and advanced profiles

```text
console [--clear]                errors [--clear]
highlight <selector>             inspect
trace start|stop [path]          profiler start|stop [path]
a11y [url]                       vitals [url]
pushstate <url>                  removeinitscript <id>
```

The upstream runtime groups typed tools into `core`, `network`, `state`,
`debug`, `tabs`, `react`, `mobile`, and `all` profiles. iyw-claw keeps the
managed browser controller on one fixed runtime and exposes only the current
host-approved operations. Profile discovery is informative; it does not grant
permission to start a second browser or alter the managed runtime.

## Recovery and fallback

- `BROWSER_SNAPSHOT_STALE`: take one fresh snapshot and use one new reference.
- `BROWSER_INVALID_ARGUMENT`: fix the selector or argument from a fresh state.
- `BROWSER_TAB_GONE`: list tabs and reuse or create the exact missing tab.
- `BROWSER_OPERATION_TIMEOUT`: inspect state once; do not repeat blindly.
- `BROWSER_CONTROL_CHANGED`: handle the visible obstruction, then refresh state.
- `BROWSER_RUNTIME_UNAVAILABLE`: inspect managed state once. Only then read an
  installed `opencli-browser` Skill and run its documented doctor/fallback.

An error is not evidence that a click succeeded or failed to have an effect.
For write operations with `effectMayHaveOccurred`, inspect the page before
retrying to avoid duplicate submissions.

## Managed runtime boundary

The host owns the bundled `agent-browser` version, executable integrity,
profile, CDP endpoint, session socket, download path, screenshot directory,
shutdown, and recovery. Do not run `npm install -g`, `agent-browser install`,
`agent-browser upgrade`, `agent-browser mcp`, `agent-browser dashboard`, or
plugin installation from an Agent session. Do not edit global Agent MCP config.
The upstream `record` command creates a fresh browser context, so it is not
exposed through the fixed-tab managed command surface; use screenshot, PDF,
trace, or profiler output instead until the host owns recording context swaps.
If the managed browser is missing or unsupported, report the concrete host
prerequisite and follow the documented fallback policy.

## Business Skills

Domain Skills may wrap this Skill with page-specific selectors, field schemas,
preflight checks, and post-submit verification. For example,
`iyw-copyright-registration` uses `agent-browser` for the IYW copyright portal.
Read the domain Skill first when the user names a specific business workflow,
then use this Skill's browser-first and human-takeover rules underneath it.
