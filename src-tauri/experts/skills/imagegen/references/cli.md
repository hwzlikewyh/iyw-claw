# IYW Image CLI Reference

Use `scripts/image_gen.py` for normal generation and editing through the IYW
Fusion API. Live calls require an iyw-claw login or `IYW_TOKEN`; `--dry-run`
requires neither credentials nor network access.

## Setup

```powershell
$skillDir = Join-Path $env:USERPROFILE ".iyw-claw\skills\imagegen"
$imageCli = Join-Path $skillDir "scripts\image_gen.py"
```

Run with an isolated uv environment:

```powershell
uv run --with openai --with pillow python $imageCli --help
```

## Generate

```powershell
uv run --with openai --with pillow python $imageCli generate `
  --prompt "Editorial product photo of a ceramic mug" `
  --size 1536x1024 `
  --quality medium `
  --out "output\imagegen\mug.png"
```

Use `--prompt-file` instead of `--prompt` for long prompts. Use `--n` only for
variants of the same prompt and `--out-dir` when generating more than one file.

## Edit

```powershell
uv run --with openai --with pillow python $imageCli edit `
  --image "input\product.png" `
  --prompt "Replace only the background with a clean white studio; preserve the product" `
  --out "output\imagegen\product-white.png"
```

Repeat `--image` for multiple references. Use `--mask` only when the model and
request require a mask. Do not set `--input-fidelity` with `gpt-image-2`.

## Batch

Create UTF-8 JSONL with one object per distinct asset:

```json
{"prompt":"A red ceramic mug","out":"red-mug.png"}
{"prompt":"A blue ceramic mug","out":"blue-mug.png"}
```

```powershell
uv run --with openai --with pillow python $imageCli generate-batch `
  --input "tmp\imagegen\jobs.jsonl" `
  --out-dir "output\imagegen" `
  --concurrency 3
```

Do not retry a batch automatically when its completion state is uncertain.

## Common Flags

- `--model`: defaults to `gpt-image-2`.
- `--size`: `auto` or a supported `WIDTHxHEIGHT`.
- `--quality`: `low`, `medium`, `high`, or `auto`.
- `--output-format`: `png`, `jpeg`, or `webp`.
- `--force`: overwrite an existing output only when the user authorized it.
- `--dry-run`: validate and print the non-secret request shape without a call.

## Authentication and Network

The script reads `IYW_TOKEN` or
`~/.iyw-claw/iyw-account-token.json`. Never pass credentials in arguments.
It sends requests to `https://gateway.iyw.cn/iyw-fusion-api/v1` by default.
Read [codex-network.md](codex-network.md) only for sandbox/network failures.

## Display

After every successful command, resolve each output to an absolute path and
call `show_image`. Do not return only `Wrote ...` text or a Markdown link.

## Transparency

For a simple opaque subject, generate a flat key-color background and run:

```powershell
uv run --with pillow python "$skillDir\scripts\remove_chroma_key.py" `
  --input "output\imagegen\source.png" `
  --out "output\imagegen\transparent.png" `
  --auto-key border --soft-matte --despill
```

Use `gpt-image-1.5 --background transparent --output-format png` when true
native transparency is required. `gpt-image-2` rejects transparent background.
