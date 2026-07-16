# Agent Display Image Tool Design

## Goal

Add an always-available `show_image` tool to the built-in `iyw-claw-mcp`
companion so MCP-capable agents can render an image directly in the
conversation. The tool supports local files, HTTP/HTTPS URLs, Data URIs, and
raw Base64 data.

## User Experience

An agent calls:

```json
{
  "source": "C:\\workspace\\output\\chart.png",
  "caption": "Monthly revenue"
}
```

The conversation renders a dedicated "Displayed image" card using the existing
image preview and download behavior. A source row appears below the image when
the source can be reopened:

- Local file: show the file name/path. On a local desktop, clicking calls the
  platform opener so the system default application opens the file. It never
  opens an iyw-claw file tab or internal preview.
- HTTP/HTTPS URL: show the URL. Clicking opens the system browser on desktop or
  a new browser tab on web.
- Data URI or raw Base64: render the image without a source link.
- Remote desktop/web viewing a server-side local path: show the path as plain
  text. Do not open the file on the server or pretend the client can open it.

The tool is included by default in every `iyw-claw-mcp` process injected into
an agent that supports MCP. It has no settings toggle.

## Tool Contract

Tool name: `show_image`.

Input fields:

- `source` (required string): absolute or relative local path, `file://` URI,
  HTTP/HTTPS URL, Data URI, or raw Base64.
- `mime_type` (optional string): required for raw Base64; ignored only when the
  source provides a trustworthy and validated image type.
- `caption` (optional string): user-facing description, capped at 2,000
  characters.
- `name` (optional string): display/download file name when the source has no
  useful name, capped at 255 characters.

Relative paths resolve against the MCP companion process working directory.
The result uses standard MCP image content:

```json
{
  "content": [
    {
      "type": "text",
      "text": "{\"type\":\"iyw_claw_display_image\",...}"
    },
    {
      "type": "image",
      "data": "<base64>",
      "mimeType": "image/png"
    }
  ],
  "isError": false
}
```

The compact, namespaced JSON text item is a durable metadata envelope. ACP
hosts already preserve text tool output and image content in live events and
session transcripts. The frontend uses the envelope to recover the caption,
display name, source kind, and external-open target without relying on
non-standard MCP fields that an agent host might drop.

## Source Loading And Validation

The companion resolves one source per call:

1. `http://` or `https://`: download with a 15-second timeout.
2. `data:`: parse the media type and Base64 section.
3. `file://`: convert to an OS path and read it.
4. Plain string with `mime_type`: first try an existing path; otherwise treat
   it as raw Base64.
5. Other plain strings: treat as a local path.

Limits and validation:

- Maximum decoded/downloaded size: 10 MiB.
- Supported media types are `image/png`, `image/jpeg`, `image/gif`,
  `image/webp`, `image/bmp`, `image/avif`, and `image/svg+xml`.
- Reject malformed Base64, non-success HTTP responses, empty files, unsupported
  URI schemes, and data whose raster magic bytes or SVG text header contradict
  its media type.
- HTTP responses are streamed and stopped once the size limit is exceeded.
- Errors return an MCP tool result with `isError: true` and concise actionable
  text. Image bytes are never included in error messages or logs.

## MCP Companion Changes

Add an `images` member to `CompanionFeatures`. Unlike optional product features,
the connection layer always includes `images` when it injects the built-in MCP
companion. Existing delegation/feedback/ask/session flags remain independent.

`show_image` is a companion-local asynchronous tool. It does not use the
delegation broker because loading an image requires no main-process state. The
call participates in MCP cancellation and returns only after source loading,
validation, and Base64 encoding complete.

## Conversation Rendering

The ACP connection already extracts `ContentBlock::Image` values from tool-call
content into `ToolCallInfo.images`. The live reducer currently routes every
image-bearing tool call through the image-generation card. Extend that path to
recognize the metadata envelope:

- Envelope present: emit a frontend `display_image` block with image, caption,
  source metadata, display name, and tool status.
- Envelope absent: retain current `image_generation` behavior exactly.

For historical messages, extend the image-bearing `tool_result` adapter to
parse the same envelope from `output_preview` and emit the same displayed-image
part. This keeps live streaming, reconnect snapshots, and transcript reloads
visually consistent.

Refactor the current generated-image card into a shared image card with a
`generated` or `displayed` presentation. Generated images keep their existing
title and revised prompt; displayed images use localized "Displayed image"
copy, the optional caption, and the external source row.

External navigation uses existing platform wrappers:

- HTTP/HTTPS: `openUrl`.
- Local path on local desktop: `openPath`.
- Local path in web/remote mode: no click action.

## Internationalization

Add display-image title, source-link label, unavailable-local-link hint, load
failure, size-limit, and invalid-image strings to all ten locale files. Do not
surface raw English MCP errors in UI-owned labels.

## Verification

The repository must continue to contain no standalone test files except
`src-tauri/scripts/release-contracts.test.mjs`.

- Add Rust inline tests in `companion.rs` for feature gating, path/Data URI/raw
  Base64 parsing, metadata response shape, size rejection, and error results.
- Use a temporary frontend test during TDD for metadata parsing and external
  target classification, then delete it before delivery.
- Run targeted ESLint/Prettier, `pnpm build`, desktop library check, server/MCP
  check and clippy, release contracts, and the standalone-test-file scan.
- Smoke-test the MCP JSON-RPC `tools/list` and `tools/call` response with a tiny
  PNG fixture generated in a temporary directory.
- Verify the rendered card in local desktop/browser screenshots at desktop and
  narrow viewport sizes, including the external-link row and no overlap.

## Non-Goals

- Image editing or generation.
- Video, audio, PDF, or Office document rendering.
- Multiple images in one tool call.
- Persisting a second copy of image bytes outside the agent transcript.
- Opening server-side local files from a remote browser.
