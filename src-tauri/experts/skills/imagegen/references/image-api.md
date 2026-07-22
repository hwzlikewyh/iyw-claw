# IYW Fusion Image API Parameters

These parameters are exposed by the bundled `scripts/image_gen.py` client. The
client uses the OpenAI-compatible Image API at the IYW Fusion endpoint.

## Models

| Model | Quality | Input fidelity | Size | Use |
|---|---|---|---|---|
| `gpt-image-2` | `low`, `medium`, `high`, `auto` | Always high; omit the flag | `auto` or validated flexible sizes | Default generation and editing |
| `gpt-image-1.5` | `low`, `medium`, `high`, `auto` | `low`, `high` | `1024x1024`, `1024x1536`, `1536x1024`, `auto` | Native transparent output and compatibility |
| `gpt-image-1` | `low`, `medium`, `high`, `auto` | `low`, `high` | Legacy fixed sizes | Compatibility only |
| `gpt-image-1-mini` | `low`, `medium`, `high`, `auto` | `low`, `high` | Legacy fixed sizes | Lower-cost compatibility |

## gpt-image-2 Size Rules

- `auto`, or explicit `WIDTHxHEIGHT`.
- Maximum edge: 3840 pixels.
- Both edges must be multiples of 16.
- Long-to-short edge ratio must not exceed 3:1.
- Total pixels: 655,360 through 8,294,400.

Common values: `1024x1024`, `1536x1024`, `1024x1536`, `2048x2048`,
`2048x1152`, `3840x2160`, and `2160x3840`.

## Generate

Supported request fields include `model`, `prompt`, `n`, `size`, `quality`,
`background`, `output_format`, `output_compression`, and moderation controls
accepted by the selected model.

## Edit

Edit requests add one or more image files, optional mask, and optional
`input_fidelity`. Repeat invariants in the prompt. Omit `input_fidelity` for
`gpt-image-2`.

## Transparency

`gpt-image-2` does not support `background=transparent`. Use a flat chroma-key
background plus `scripts/remove_chroma_key.py` for clean opaque subjects. Use
`gpt-image-1.5` with `background=transparent` and PNG/WebP when native alpha is
required for hair, glass, smoke, translucent materials, reflections, or soft
shadows.

## Response Handling

The CLI decodes `b64_json` into the requested output files. A process exit code
of zero is necessary but not sufficient: confirm every expected file exists and
is a supported image before calling `show_image`.

Do not expose response metadata that identifies internal providers, channels,
models selected by the service, token values, or request headers.
