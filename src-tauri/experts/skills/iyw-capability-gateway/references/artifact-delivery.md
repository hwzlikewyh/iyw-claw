# Artifact Delivery

Load this reference whenever a task creates a final file, directory, or public
URL. Artifact delivery is a required completion sub-goal, not an optional
presentation step. Call the directly advertised `present_task_files` using its
current input schema; no capability search/read/invoke is needed. Only fall back
to the live catalog when the direct tool is unavailable. The current schema
overrides this guide.

## What to Register

This places saved deliverables in the conversation's Artifacts area. Use
`show_interactive_html` for a freely designed page that opens inside the chat;
displaying an interactive page does not register a saved file or URL as an artifact.

Register only the completed deliverables needed to satisfy the user's task
through `present_task_files` before the final response. Infer the deliverables
from the task; no separate explicit request or approval for each file is needed:

- A working-directory-relative file or directory.
- An absolute file or directory path when the user explicitly needs that path.
- An HTTP or HTTPS URL that is part of the final result.

The capability accepts at most 100 item references. Relative paths resolve from
the current Agent working directory and are confined to that workspace after
canonicalization. Existing files and directories must be readable; missing,
inaccessible, unsupported, device, symlink-escape, credential-bearing, or
non-HTTP URL sources are rejected. A URL artifact must use `http` or `https`
without embedded username or password. Preserve the original reference and
inspect accepted/rejected entries in the returned result.

Submit image URLs directly without a separate shell, browser, or download tool
call just to prepare delivery. The host downloads image URLs into the managed
turn directory and registers the downloaded file. Images with identical bytes
share one artifact path in the same turn, even when supplied through different
URLs or local file names. Submit one reference per image; do not deliver both a
URL and its local copy. Use the accepted path returned by the tool, which may
differ from the submitted URL. Download or storage failures are rejected entries.
Image downloads use the host's existing 20 MiB limit and public-network checks;
non-image web pages remain URL artifacts.

For a PowerPoint containing images, register the finished PPT/PPTX. Do not
separately register its embedded images, reference materials, drafts, scripts,
or temporary exports. Include companion files only when the final deliverable
requires those external files to work. Apply the same rule to reports, PDFs,
webpages, archives and other composed deliverables.

Files do not become deliverables merely because they were created, modified,
downloaded or used during the task. Source code, configuration, tests, build
output, logs, caches and internal working files belong in Artifacts only when
they themselves are the task's final deliverable. A dirty Git diff is not an
artifact list.

For images generated as intermediate material, set
`generate_iyw_image` input `delivery.registerArtifact` to `false`; its default is
`true`. Embed the needed images in the completed deliverable, then register
that deliverable. Keep image registration enabled when the images themselves
are the final result. In a batch, the top-level `delivery` applies to every item;
separate intermediate assets from final-image deliveries when they differ.

## Query and Manage

Use the same `present_task_files` tool with an `action`:

- `present`: register `files`; omitted `action` preserves this behavior.
- `list`: paginate and search, with optional `search`, `message_id`, `page` and
  `page_size` (default 50, maximum 100).
- `get`: read an existing `artifact_id` and its current availability.
- `update`: provide `artifact_id` and `display_name`, `source`, or both. The
  source is a replacement final file, directory or HTTP/HTTPS URL. The original
  ID, conversation, reply and creation time remain attached to the record.
- `delete`: remove the current conversation's record by `artifact_id`. It does
  not delete original files, managed copies or remote resources. A repeated
  deletion returns `deleted: false`.

`list` and `get` accept `scope: "current"` (default, this conversation) or
`scope: "all"` (all conversations in the current workspace). The host determines
the workspace; do not invent conversation IDs or folder IDs. Updates and
deletions remain restricted to the current conversation. Obtain real IDs from
registration or query results. Management actions are not new deliveries.

```json
{"action":"list","scope":"all","search":"report","page":1,"page_size":20}
```

```json
{"action":"update","artifact_id":123,"display_name":"Final report","source":"output/report.pdf"}
```

## Current Conversation and Reply Scope

Registration belongs to the current conversation Artifacts and is linked to the
assistant turn generation. It does not write directly to the workspace-wide
“All Artifacts” aggregate. The UI can attribute registered items to the current
assistant reply only when the gateway result contains an accepted registration;
an explicit failure or zero accepted items must not be replaced by guessed input
paths. A new conversation may use a temporary runtime identity before its
persisted database ID exists; wait for the host to resolve the persisted
conversation before assuming the current artifact view is populated.

When the user asks for only this reply's new成果, use explicit registrations
from this reply and the current-reply scope. Do not infer ownership from session
totals, timestamps, dirty-file statistics, or files merely mentioned in a
prompt. Reuse the host's artifact IDs and current conversation identity rather
than inventing a virtual ID.

## HTML and Markdown with Images

Make image hosting part of the document's portability before registration:

1. Reuse an already verified public HTTPS image URL when one exists.
2. For newly generated or local images, call `generate_iyw_image` with
   `delivery.registerArtifact: false` when they are document materials. The
   gateway uploads non-URL input to TOS and returns a public HTTPS result URL
   without calling `checkImage`.
3. Write that verified public URL into HTML `<img src>` or Markdown image link.
4. Reopen/read the document or inspect its source to verify every image
   reference is intentional and has no local absolute path or temporary token.
5. Register the final HTML/Markdown and its required companion assets together.

Never embed a presigned PUT URL, a URL with temporary signature query
parameters, `file://`, a local absolute path, or a guessed CDN/TOS URL. Do not
upload private or sensitive images, or violate a user request to keep assets
local/offline. If TOS upload is unavailable or fails, do not fabricate a URL;
use a workspace-relative path only when the recipient can access the
same artifact directory, and state the portability limitation.

For image production, call `generate_iyw_image`; it owns input preparation,
generation/editing, result waiting, display, and public URL delivery. Keep
knowledge retrieval separate through `search_iyw_knowledge`. `analyze_image`
and `show_image` do not upload or generate images.

Choose image verification from the user's requested outcome. Ordinary generation
or editing delivers successful images directly from returned status, URLs, and
delivery metadata. Inspect quality for a requested review, comparison, or visual
acceptance task; verify placement/rendering when integrating images into a requested
page or composed deliverable. A detailed generation prompt alone does not require
a separate quality review. Keep task-status and artifact-registration checks, and
do not automatically regenerate beyond the requested scope.

## Preview and Browser Presentation

The artifact UI supports the host's existing file, directory, Markdown, HTML,
PDF, Office, image, and text preview paths. HTML at or above the host's 20 MiB
preview limit is not renderable; still register the file if it is the requested
deliverable and tell the user that preview is unavailable. Do not silently
truncate or rewrite a large HTML file to make preview pass.

For a completed HTML page, local service, or visual report that the user should
inspect, also load `browser-and-media.md` and proactively use the browser
presentation capability after verifying the intended URL/page. Presentation is
not registration: perform both when both are relevant. Close only the detached
display window when finished; do not close a shared managed tab the user still
needs.

## Result Verification

The registration result separates accepted and rejected items. Completion is
valid only when every required final item is accepted, or when any rejected item
is intentionally omitted and the limitation is reported. Treat zero accepted,
partial rejection, persistence failure, missing artifact directory, and
effect-unknown as incomplete delivery. Do not claim an artifact exists merely
because the path was valid locally or the tool returned HTTP success.

If delivery is unavailable, keep the final response honest: name the concrete
missing capability or rejected item, give the valid local result if useful, and
do not retry with guessed paths or register internal fallbacks. Preserve any
`iyw_delivery_receipt` for the next real gateway invocation.
