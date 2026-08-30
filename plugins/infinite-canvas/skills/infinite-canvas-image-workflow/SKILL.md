---
name: infinite-canvas-image-workflow
description: Import generated image assets into an Infinite Canvas scene.
---

Use the iyw-claw capability gateway and the `write-asset` capability for generated files. Upload
128 KiB-or-smaller chunks with the expected byte count and SHA-256, then apply a node operation that
references `asset://<sha256>`. Keep the prompt and pending node when generation or billing fails;
retry only when the user explicitly asks. Never put base64 image data in a scene or call an external
model endpoint from this Skill.

For a failed request, keep the original `requestId` node with `status: "error"`. An explicit retry
creates a new request ID and a new pending node; it must not replay billing or replace an existing
successful image.

For `image.annotation-edit`, read the target image node and its associated annotation layer, use
the supplied flattened `inputAssetSha256`, and write the edited result as a new image node beside
the original. Keep the original image and annotations unchanged.

For `image.generate`, create the pending request node first, then return one result envelope per
generated file. Each successful file must be uploaded through `write-asset` and applied as a new
`media` node with `{ asset: { sha256, bytes, mimeType, path } }`; never guess dimensions before
reading the uploaded image metadata. Place multiple results to the right of the selected bounds,
preserve aspect ratio, and optionally wrap them in a `group` node with `primaryImageId`.

On partial success, keep successful image nodes and mark the request node `status: "partial"` with
the safe failure code. On total failure, keep the prompt and mark it `status: "error"`. A retry is
always a new request ID and never reuses a failed billing or upload operation.
