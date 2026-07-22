---
name: imagegen
description: Use when an Agent needs GPT Image-specific generation parameters or image editing through the IYW Fusion API; route ordinary text-to-image requests through iyw-image-workflows first.
---

# Image Generation

Generate and edit images with the bundled `scripts/image_gen.py` CLI through
the IYW Fusion API. Do not use or wait for a built-in `image_gen` tool.

## Routing

Treat `iyw-image-workflows` as the primary router for image requests:

- Use its verified commerce workflow for upload/review, product variation,
  series extension, multi-image fusion, commerce upscale, and task queries.
- Use its verified `fission-generate` command first for ordinary text-to-image
  requests.
- Use this skill's CLI for image editing or when the user explicitly requests
  GPT Image-specific generation parameters.
- Never guess an IYW endpoint or commerce payload.

## Authentication

The CLI resolves the IYW access token in this order:

1. `IYW_TOKEN`
2. `~/.iyw-claw/iyw-account-token.json` field `access_token`

Normal use relies on the account file created by iyw-claw login. Do not ask the
user to paste a token, pass a token on the command line, print it, or include it
in a prompt. The CLI sends the same token as the OpenAI-compatible SDK
`api_key` and the custom `token` request header.

The default API base is:

```text
https://gateway.iyw.cn/iyw-fusion-api/v1
```

Only use `IYW_FUSION_API_BASE_URL` when the user explicitly selects another
trusted environment.

## Entry Point

Resolve the installed skill from the current user's iyw-claw directory:

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\imagegen"
$imageCli = Join-Path $skillDir "scripts\image_gen.py"
```

Prefer the `uv` path supplied in the iyw-claw runtime context. Run the CLI with
an isolated dependency environment:

```powershell
uv run --with openai --with pillow python $imageCli generate `
  --prompt "A clean product photograph of a ceramic mug" `
  --out "output\imagegen\ceramic-mug.png"
```

Commands:

- `generate`: create one image or variants of one prompt.
- `edit`: edit one or more local images while preserving stated invariants.
- `generate-batch`: generate distinct prompts from JSONL.

Read [references/cli.md](references/cli.md) for flags and
[references/image-api.md](references/image-api.md) for model constraints.

## Workflow

1. Determine whether the request is generation, editing, or a verified IYW
   commerce operation.
2. Convert the user's request into a concise production prompt. Preserve exact
   requested text and edit invariants. Use
   [references/prompting.md](references/prompting.md) when needed.
3. Run `--dry-run` first for a new batch or unfamiliar explicit parameter set.
   Dry-run does not require a token or access the network.
4. Run the live CLI. Do not automatically retry a request that may incur cost.
5. Treat only a zero exit code plus an existing output file as success.
6. Call `show_image` once for every final image, in requested/server order:

```json
{
  "source": "C:\\absolute\\path\\output.png",
  "caption": "生成结果",
  "name": "output.png"
}
```

`show_image` also accepts a final HTTPS URL. Use it instead of returning only a
Markdown link so iyw-claw renders the image as a native conversation image.

## Output Rules

- Save preview-only outputs under `output/imagegen/`.
- Save project assets inside the workspace before referring to them in code.
- Do not overwrite an existing asset unless the user explicitly requests it.
- Use `n` only for variants of one prompt; use `generate-batch` for distinct
  assets.
- Validate output existence, format, dimensions, and visible content before
  declaring success.
- For edits, repeat what must remain unchanged in every request.

## Transparency

`gpt-image-2` does not accept `background=transparent`. For simple opaque
subjects, generate against a flat chroma-key background and run
`scripts/remove_chroma_key.py`. Use `gpt-image-1.5` with transparent PNG/WebP
only when the user requests true native transparency or chroma-key extraction
cannot preserve complex edges.

## Failure Handling

- Missing token: ask the user to sign in to iyw-claw; do not ask for the token.
- Missing Python dependency: use `uv run --with openai --with pillow`.
- Network or API error: report the non-secret error and keep any valid prior
  output.
- Missing `show_image`: return the saved absolute path or final HTTPS URL and
  state that inline rendering was unavailable; never claim it was displayed.
