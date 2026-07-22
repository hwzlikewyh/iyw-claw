---
name: lixiao-workflows
description: Authenticate to Lixiao (励销) and call its captured enterprise search, company detail, permission, contact, phone, unlock, product, exhibition, intellectual-property, and recruitment APIs through a bundled Python CLI. Use when Codex needs to log in to 励销, persist a reusable local session, inspect available captured operations, search enterprise data, or automate the workflows documented by the Lixiao browser traffic.
---

# Lixiao Workflows

Use the bundled standard-library CLI. Keep this Skill separate from the IYW image and commerce Skills.

## Entry Point

Set the CLI path from the Skill directory:

```powershell
$cli = "lixiao-workflows\scripts\lixiao.py"
uv run --no-project python $cli api list
```

Use `python $cli` when a working Python 3.10+ executable is already available. Expect JSON on stdout and a nonzero exit code for errors.

## Security Rules

- Persist credentials only in `C:\Users\iyw\.iyw-claw\credentials.json` by default.
- Save only application tokens, SSO access tokens, business API tokens, and required cookies.
- Never save the phone number, password, captcha proof, or request payload used for password login.
- Enter application and business tokens through hidden input with `auth set-app-token` and `auth set-business-token`. Do not place them in command arguments.
- Never print saved tokens, cookies, passwords, app secrets, or authorization headers. The CLI redacts these fields, including in `--dry-run` output.
- Override storage only for isolated testing with `--config-dir` or `LIXIAO_CONFIG_DIR`.

## Login Workflow

1. Inspect non-secret session state:

```powershell
uv run --no-project python $cli auth status
```

2. Configure the application token once through hidden input:

```powershell
uv run --no-project python $cli auth set-app-token
```

3. Prefer QR login. Create the QR code, open the returned `data.url`, then poll using the returned `data.code`:

```powershell
uv run --no-project python $cli auth qr-start
uv run --no-project python $cli auth qr-wait --code <code> --wait-seconds 120
```

4. Use password login only when a valid Geetest proof is already available. Run `auth captcha`, complete Geetest externally, then pass the proof fields. Enter the password only at the hidden prompt:

```powershell
uv run --no-project python $cli auth captcha
uv run --no-project python $cli auth password --phone <phone> --challenge <challenge> --validate <validate> --seccode <seccode>
```

5. Configure the business authorization token through hidden input if business calls report `authentication_required`:

```powershell
uv run --no-project python $cli auth set-business-token
```

The captured traffic does not contain the exchange request that converts the SSO access token into the separate business API token. Do not guess that exchange or reuse the SSO token as business authorization.

## Call APIs

Read [references/commands.md](references/commands.md) when selecting an operation or constructing a search body.

Use required fields as named flags. Override or add query parameters with repeatable `--query KEY=VALUE`:

```powershell
uv run --no-project python $cli api company-card --id <company-id>
uv run --no-project python $cli api company-products --id <company-id> --query page=1
```

Pass POST bodies as inline JSON, `@file.json`, or `-` for stdin:

```powershell
uv run --no-project python $cli api scene-search --body @search.json
Get-Content request.json | uv run --no-project python $cli api phone-call-list --body -
```

Use `--dry-run` before a new call to inspect the URL and redacted request shape without sending it:

```powershell
uv run --no-project python $cli --dry-run api company-card --id <company-id>
```

## Result Handling

- Treat `ok: true` as successful CLI execution and consume `data` as the upstream response.
- Treat `ok: false` as an error and inspect `error.code`, `error.message`, and `error.retryable`.
- Do not infer permission from empty contact fields. Check `permission-info` and feature packages first.
- Do not retry authentication or permission errors automatically. Retry only errors marked `retryable: true`.
