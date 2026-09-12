# Browser and Media Operations

Load this reference for web pages, public data, website interaction, screenshots,
audio, transcription, or image understanding. The unified `browser` tool checks
both the user's connected Chrome/OpenCLI and the iyw-claw managed browser. It
prefers OpenCLI for existing Chrome sign-in state and switches only for a
classified human-only action. Use the live catalog for exact stable IDs and
schemas.

The built-in browser setting is enforced by the host. When off, all operations,
presentation, and user action stay in external Chrome/OpenCLI. Never re-enable
or enter the managed browser to recover an external failure. An old managed tab
requires a new external open and snapshot; do not replay its references.
`OPENCLI_USER_ACTION_REQUIRED` means the human step is still pending, even when
the external tab was successfully presented. Verify fresh state after the user
finishes. A timeout with `effectMayHaveOccurred=true` must not be blindly retried.

## Unified Browser Workflow

Use this sequence for ordinary navigation and interaction:

1. **List**: call `browser` with `action=list_tabs` to inspect both providers.
   Reuse an existing opaque tab id whenever possible.
2. **Open**: call `browser` with `action=open` for an HTTP/HTTPS URL. Set
   `new_tab: true` only
   when another tab is explicitly needed. Pass an exact tab ID when navigating
   a non-active tab. `about:blank` is the only non-HTTP URL allowed.
3. **Inspect**: call `browser` with `action=snapshot` before an
   action. It returns short-lived `@eN` references for interactive elements.
   Use `browser` with `action=read` when agent-readable page text is needed; `outline` is
   useful for headings and `filter` narrows large pages. Use `raw` only when
   the response body itself is required.
4. **Act**: call `browser` with the matching action using the exact tab ID. Use an
   `@eN` reference from the latest snapshot or a precise CSS selector. Never
   reuse a reference after navigation, route changes, popups, material DOM
   updates, or a write action.
5. **Refresh and verify**: after any material page change, call another
   `browser(action=snapshot)` before the next action. Verify URL, title, text, element state,
   downloaded file, or the business result. A successful click alone is not
   evidence that the requested action completed.
6. **Capture or present**: use screenshot for a local managed-browser image;
   use `analyze_image` when visual understanding is needed. Use browser present
   for a completed UI, local service, HTML preview, or visual result the user
   should see. Close the detached display window when no longer needed; close a
   tab only when it is no longer needed by the user or another Agent.

### Browser tool choices

| Need | Tool behavior |
| --- | --- |
| Find/reuse a page | `browser(action=list_tabs)`; preserve opaque tab IDs |
| Navigate | `browser(action=open)`; active tab by default, new tab only when needed |
| Inspect controls | `browser(action=snapshot)`; interactive by default, references expire |
| Extract text | `browser(action=read)`; outline/filter before reading a large page |
| Click/type/key/scroll/wait | `browser(action=...)`; refresh snapshot after change |
| Screenshot | `browser(action=screenshot)` |
| Advanced browser operation | `browser(action=advanced)` |
| Show a page to the user | `browser(action=present)` |
| Ask the user to operate | `browser(action=request_user_action)` |
| Clean up | `browser(action=close_window|close_tab)` |

### Wait arguments

After discovering and reading `iyw.browser.unified.v1`, use `wait` as an
`action` value. For a bounded delay on the current task tab:

```json
{
  "capability_id": "iyw.browser.unified.v1",
  "arguments": { "action": "wait", "milliseconds": 1000 }
}
```

Prefer `action=wait` with an observed `selector` and bounded `timeout_ms` when
waiting for an element. Pass the exact returned `tab_id` to target a specific
tab. Opening a page and explicitly waiting are separate calls on that same tab;
do not add a `wait` property to `action=open`. `timeout_ms` limits execution time
and is not a delay. Follow the current schema's field names and examples.

## Advanced Browser Commands

Use `browser_command` only when a dedicated tool cannot express the operation.
Read the installed `agent-browser` Skill first. The host pins the command to one
managed tab, appends its fixed CDP endpoint, and never invokes a shell. Commands
may cover extraction, semantic locators, keyboard/mouse/form actions,
upload/download, waits, PDF, frames/dialogs, JavaScript, accessibility,
performance, React, network inspection, and debugging. Lifecycle operations
such as opening/closing tabs or windows, profile management, installation,
plugins, dashboard, or chat are rejected; use dedicated host tools instead.

Treat cookies, storage, state, headers, clipboard data, credentials, and page
scripts as sensitive. Pass every command argument separately: no shell quoting,
pipes, redirects, command chaining, or guessed command names.

## Recovery, Fallback, and Human Action

- A gateway schema rejection with `execution_status=not_started` follows the
  one-correction rule in `tool-usage.md`: use the schema already read, preserve the
  intended operation, and retry once. It is not a browser/provider failure and
  does not authorize a provider switch. Stop if the corrected call fails.
- For a stale reference or locator failure, take one fresh snapshot and retry
  the same intended action once with one new reference or revised locator. Do
  not cycle selectors.
- For OpenCLI bridge, Chrome, extension, daemon, CDP, network, timeout,
  selector, or unknown failure, return the structured failure and stop the
  current browser/provider attempt. Do not switch providers from this Skill;
  the caller may choose another already-authorized route, but must not treat
  this failure alone as proof that the business task is impossible.
- Switch only when the built-in browser is enabled and OpenCLI reports login, MFA, CAPTCHA, device approval,
  security confirmation, human review, or another explicit user-action
  requirement. Keep that task pinned to the managed provider afterward.
- Request browser user action only for credentials held by the user, MFA,
  CAPTCHA, device approval, secure payment confirmation, an unavailable
  managed operation, or explicit human review. Do not request it for ordinary
  navigation, a stale selector, a wait, or an operation covered by a dedicated
  tool. Never put passwords, one-time codes, cookies, or tokens in the reason or
  completion conditions.
- Completion conditions use stable evidence such as `urlContains`,
  `titleContains`, `textContains`, `selector`, or `downloadCompleted`. All
  supplied conditions are required. A timeout or closed window is not proof of
  completion; inspect fresh state afterward.

Browser failure is route-local. When a domain Skill's direct data source is
incomplete or the page is dynamically rendered, normal UI navigation remains a
valid read-only discovery path. Only reuse a request after the page actually
triggered it and its fields, session context, and business result are verified.

## Public Web and Research

For a URL, public page, platform, current event, comparison, or cited research,
also load `research-workflow.md` or `internet-routing.md` as applicable. Search
snippets are leads, not evidence. Keep canonical URL/title/date/publisher and
the claim supported; deep-read selected pages and mark login, paywall,
truncation, translation, user-generated, and single-source limitations. Use the
unified browser capability for dynamic or authenticated pages and verify the
final business result in a fresh snapshot/read.

## Audio Recognition and Transcription

Choose the route by size, duration, durability, and speaker requirements:

| Audio need | Route | Behavior |
| --- | --- | --- |
| Ordinary short audio, immediate text, no diarization or resume requirement | `transcribe_audio_flash` | Synchronous result; up to 100 MiB and 2 hours |
| Meeting, multiple speakers, channel separation, oversized/long audio, or resumable work | `transcribe_audio` | Durable asynchronous job; up to 512 MiB and 5 hours; save returned `job_id` |
| An async job is not terminal | `query_audio_transcription` | Query by the exact decimal `job_id`; repeat only according to returned status |

Provide exactly one source for either create route:

- `path`: a readable file path relative to the current workspace. Prefer this
  for large local files; do not guess an absolute path.
- `url`: one HTTPS audio URL downloaded by the host.
- `data`: Base64 or a `data:<mime>;base64,...` URI. Raw Base64 additionally
  requires a safe `fileName` and `mimeType`; the schema limits the data string
  to 24 MiB, so use path or URL for larger audio.

`language` is an optional BCP-47 tag and defaults to `zh-CN`. The options
`punctuation`, `wordTimestamps`, `speakerDiarization`, and `channelSplit` are
schema-controlled. Flash supports WAV/MP3/OGG directly and the host may convert
M4A or another supported container to WAV when the upstream requires it. Do not
claim speaker labels, channel separation, or durable recovery when those options
were not requested or the flash route was used.

For async transcription, treat the first response as a job acknowledgment when
it is non-terminal, not as the final transcript. Query with the returned ID and
report terminal success, failure, or unavailable state exactly. For flash,
verify that the returned transcript is complete before summarizing or using it.

## Images

Use `analyze_image` to understand or judge an existing image and `show_image` to
display an existing or generated image. Do not use either to generate or edit
images. Use `generate_iyw_image` for all image production/editing, including
IYW product/material/commerce and ordinary raster creation. Attach SVG, BMP,
ICO, and other unsupported model-image formats as ordinary files rather than
forcing an image-analysis route.

Prefer `generate` (`images/generations`) for text-to-image, `variation` for
single-image changes, `mix` for multi-image fusion, and `extend` for four-panel
grids or same-series extension from one base image. `auto` follows these defaults;
without images it uses `generate`. A grid request without a reference also uses
`generate`. Explicit `edit` (`images/edits`) requires source images. Neither Fusion
operation requires a prior platform attempt or failure. `fission` and specialized
platform operations remain available when selected. A timeout, transport error
or running task is not confirmed failure; query its task ID before any retry.

Before `type=generate`, `auto` without images, or explicit `type=edit`, call
`list_iyw_image_models` with `{}`. Choose a returned model for the user's task
with `image_generation` or `image_editing` enabled, respectively, then pass its
exact `id` in `parameters.model`. Reuse the catalog for the same task or batch.
Specialized IYW operations such as `variation`, `extend`, and `mix` do not need
this Fusion model lookup.

Default timeouts are 600 seconds for platform HTTP requests and task polling,
and 300 seconds for Fusion generation/editing. The agent may override either with
`wait.timeoutSeconds`, including values above 600; each batch item can set its own
wait. Prefer the defaults or longer for slow image tasks. `0` explicitly submits
platform tasks without polling; Fusion retains its default timeout.

After `generate_iyw_image`, choose verification from the user's task. Ordinary
generation/editing can deliver successful results directly using returned status,
URLs, and delivery metadata. Use visual analysis when the task includes quality
review, comparison, or visual acceptance; verify placement/rendering in a requested
page or composed deliverable. A detailed generation prompt alone is not a review
request. Keep checks focused and do not regenerate beyond the requested scope.

For an HTML or Markdown deliverable that embeds newly generated or local
images, use the public HTTPS URL returned by `generate_iyw_image`. Already
verified public HTTPS image URLs may be reused. Never embed a presigned PUT URL,
a temporary signed query URL, or a local absolute path. Skip external upload for
private/sensitive images or an explicit local-only request. If TOS is not
available, do not invent a URL; use a workspace-relative path only when it is a
valid fallback and report the limitation before registering the final artifact.
