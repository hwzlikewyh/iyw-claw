# Artifact Delivery

Load this reference whenever a task creates a final file, directory, or public
URL. Artifact delivery is a required completion sub-goal, not an optional
presentation step. The current conversation's live gateway catalog and the
`present_task_files` read schema override this guide.

## What to Register

Register every final user-facing item together through `present_task_files`
before the final response whenever possible:

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

Do not register source files, configuration, tests, migrations, build output,
caches, logs, temporary files, scratch notes, private gateway data, or internal
working files unless the user explicitly requests that exact item. A dirty Git
diff is not an artifact list. Register the actual final deliverable selected by
the current task, not every file touched during implementation.

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
2. For newly generated or local images, load `iyw-image-workflows` and use its
   validated `upload` path when the user permits external storage. Use the URL
   returned only after TOS upload and image check succeed.
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

If the image is an IYW product/material/knowledge or commerce asset, the domain
Skill owns upload, image checking, and business workflow. If it is free raster
creation/editing or an explicit GPT Image request, use `imagegen` for generation
and then handle any user-approved hosting through its returned verified URL or
the appropriate IYW upload path. `analyze_image` and `show_image` do not upload
or generate images.

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
