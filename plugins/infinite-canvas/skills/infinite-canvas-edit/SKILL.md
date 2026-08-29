---
name: infinite-canvas-edit
description: Read and edit an Infinite Canvas scene with revision-safe operations.
---

Use the iyw-claw capability gateway. Read state and selection before editing, then invoke
`plugin.infinite-canvas.canvas.apply-ops.v1` with the observed revision. Use stable node and
connection IDs, keep asset payloads out of scene JSON, and retry only after a revision conflict has
been read and presented to the user. Persist a changed selection through the selection capability.

Do not write arbitrary workspace paths, bypass HostGateway, start a web service, or silently replay
a failed operation. Preserve existing nodes and report stable error codes.

Creative requests use the shared actions `image.generate`, `image.annotation-edit`,
`html.generate`, `html.edit`, `slides.generate`, and `slides.annotation-edit`. A pending request is a normal scene node;
successful Agent output adds a new node and failed output updates only its status.

For `html.generate` and `html.edit`, produce a single self-contained HTML document. Remove scripts,
iframes, forms, navigation handlers, and remote `src`/`href`/`action` values before applying the
result. `html.edit` must include the observed target revision; if it is stale, reread the scene and
ask the user before writing. Preserve the old source on failure.

For `slides.generate`, return a `SlideDeck` JSON object with unique page IDs and at least one page.
For `slides.annotation-edit`, update only the requested page after checking its page revision;
leave other pages and the original annotations unchanged. Exported HTML must be self-contained and
sanitized, and a multi-page image export may succeed partially with a manifest of missing pages.
