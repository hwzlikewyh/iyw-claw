---
name: lixiao-workflows
description: Authenticate to Lixiao (励销) and call its captured enterprise search, company detail, permission, contact, phone, unlock, product, exhibition, intellectual-property, and recruitment APIs through a bundled Python CLI. Use when Codex needs to log in to 励销, persist a reusable local session, inspect available captured operations, search enterprise data, or automate the workflows documented by the Lixiao browser traffic.
routing:
  capability: Lixiao enterprise data and captured API workflows
  coreTriggers: [login to Lixiao or query its enterprise data]
  exclusions: [generic company research outside Lixiao]
  aliases: [励销, Lixiao enterprise search]
  invocation: Read SKILL.md and use only the bundled CLI and captured operations.
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

- Persist credentials as plaintext only in the current user's `~/.iyw-claw/credentials.json` by
  default so the same behavior works on Windows, Linux, and macOS. Resolve `~` from that user's home
  directory; never hard-code a Windows username or home path.
- Save application tokens, SSO access tokens, business API tokens, required cookies, and the login phone + password so later runs can re-authenticate without user input.
- Reuse the IYW Claw account token from `~/.iyw-claw/iyw-account-token.json`; do not copy it into the Lixiao credential file during the normal login flow.
- Never save the Geetest captcha proof (challenge/validate/seccode); it is single-use.
- The filtered command channel may pass the collected account and password directly to `auth login --phone ... --password ...`; do not print, echo, or reuse those values outside that login call.
- Reuse the saved session first. On an explicit authentication failure, the CLI automatically submits
  saved credentials once; a read operation is replayed at most once. Only a structured rejection from
  `password-login` removes the saved password and user session while keeping the phone. Network,
  rate-limit, captcha, TTOCR, SSO, token, and other upstream failures keep the saved password and are
  not converted into credential prompts.
- When `has_saved_account` is true but `has_saved_credentials` is false, call
  `ask_user_question` once to ask the current internal operator only for the Lixiao password, then use
  `auth login --password ...` to reuse the saved phone. When there is no usable Lixiao session or
  saved account, call `ask_user_question` once to ask only for the Lixiao account and wait for the
  current internal operator's answer. Then call `ask_user_question` a second time to ask only for the
  Lixiao password and wait for the answer. Each call must contain exactly one credential question;
  never request both values in one call or one question. Never contact the customer, recommend asking
  the customer, or request that the customer provide credentials again.
- Automatically obtain the Lixiao application token from `getApp` when it is absent. The fixed UC login entry supplies only the temporary bootstrap value required to call `getApp`; persist and use only `data.appToken` from the structured response. Do not ask users to provide or configure it during an Agent workflow.
- Passing `--unlock-if-needed` is the authorization to consume one unlock for that specific enterprise when product details are hidden. Do not ask a separate unlock question or add the flag to bulk queries.
- When an unlock is actually performed, `company-products --unlock-if-needed` immediately retries products, reads the company card, contact count, and contact details, then confirms product visibility once more. Do not run this workflow against a bulk search result.
- Never print saved tokens, cookies, passwords, app secrets, or authorization headers. The CLI redacts these fields, including in `--dry-run` output.
- Override storage only for isolated testing with `--config-dir` or `LIXIAO_CONFIG_DIR`.
- Serialize authentication and credential writes that share one `config-dir`. For parallel business
  tasks, authenticate once in the parent Agent and let child tasks reuse the read-only session, or
  provide isolated config directories for tests.

## Login Workflow

1. Inspect non-secret session state:

```powershell
uv run --no-project python $cli auth status
```

2. Run `auth ensure`. It verifies both the app session and a read-only business endpoint, then uses the
saved phone and password to log in once only when authentication has expired. If the saved login is
rejected, inspect `auth status`. When the phone remains saved but the password is absent, ask the
current internal operator only “您的励销密码是什么？” and reuse that phone without exposing it:

```powershell
uv run --no-project python $cli auth login --password <励销密码>
```

When the account is also absent, ask the current internal operator only “您的励销账号是什么？” and
wait, then ask only “您的励销密码是什么？” and wait. Never combine the questions. Submit both
through the filtered direct command, then resume the original task without a second confirmation:

```powershell
uv run --no-project python $cli auth login --phone <励销账号> --password <励销密码>
```

`auth login --interactive` ignores saved credentials, then prompts for the login phone/account followed by a hidden password. `--interactive` and `--phone` are mutually exclusive so the account does not need to appear in shell history. Account-password login is preferred over QR or manual captcha login:

```powershell
uv run --no-project python $cli auth login --interactive
```

The captcha gateway defaults to `https://gateway.iyw.cn/iyw-fusion-api/v1/ttocr/recognize`; override the URL with `--ttocr-url` or `LIXIAO_TTOCR_URL`. Authentication follows `iyw-image-workflows`: first read `access_token` from `~/.iyw-claw/iyw-account-token.json`, then fall back to `IYW_TOKEN`, and finally to the legacy saved token. Send only the `token` header; never send `tokenInfo`. If the account token is expired, sign in to IYW Claw again so it refreshes the shared token file.

A successful response has `code: 1` and returns a new `challenge`, `validate`, and `seccode` under `data`. Those three returned values are passed to Lixiao password login. The CLI also accepts the upstream-compatible `challenge|validate` string and derives `seccode` as `validate|jordan`.

After password login, the CLI sends the returned `ticket` and `x-lx-gid` to the `lxcrm.weiwenjia.com` SSO callback with the same cookie jar, then loads `/pioneers`. It extracts `window.current_user_token` from that page and stores it as the business API token used by subsequent `Authorization: Token token=...` headers. The HTML and token are never printed.

Only when local status confirms missing credentials, the agent must request them from the current internal operator through two sequential `ask_user_question` calls, one credential per call, then immediately submit them through the filtered direct-login command and continue the original task without a second confirmation. Never contact the customer for credentials. Do not display the values in a response, log, or follow-up message.

3. Reuse the saved session. Later runs load it automatically. Read operations re-login and replay only
once after an authentication error. Real workflows preflight the session once and are never replayed as
a whole; `company-unlock` and guarded product unlocks are not replayed after the request is sent:

```powershell
uv run --no-project python $cli auth ensure
```

4. Fall back to the manual Geetest flow when the captcha gateway is unavailable. Create a challenge, solve Geetest externally, then pass the proof fields. A successful manual login also saves the account:

```powershell
uv run --no-project python $cli auth captcha
uv run --no-project python $cli auth password --phone <phone> --challenge <challenge> --validate <validate> --seccode <seccode>
```

5. Fall back to QR login when neither password flow is available. Create the QR code, open the returned `data.url`, then poll using the returned `data.code`:

```powershell
uv run --no-project python $cli auth qr-start
uv run --no-project python $cli auth qr-wait --code <code> --wait-seconds 120
```

6. The password flow configures the business authorization token automatically. Use hidden input only as a fallback if the CRM page layout changes or a business call reports `authentication_required`:

```powershell
uv run --no-project python $cli auth set-business-token
```

Never reuse the UC SSO access token as business authorization; they remain distinct credentials.

## Business Workflows

Agents must use these business commands for sales-assistant work. They accept business inputs and
hide endpoint ordering, request construction, pagination, detail aggregation, and guarded unlocks:

```powershell
uv run --no-project python $cli workflow ecommerce-search `
  --keyword <keyword> --platform 1688 --platform 天猫 --platform 京东 `
  --limit-per-platform 100
uv run --no-project python $cli workflow company-profile `
  --id <company-id-1> --id <company-id-2>
uv run --no-project python $cli workflow search-conditions
uv run --no-project python $cli workflow advanced-search `
  --condition @enterprise-condition.json --limit 50 --page-size 10
uv run --no-project python $cli workflow tender-search `
  --keyword <keyword> --limit 50 --page-size 10
uv run --no-project python $cli workflow channel-search `
  --condition @channel-condition.json --keyword <keyword>
uv run --no-project python $cli workflow search-templates --search-name <name>
```

Every real ecommerce search first fetches `search-condition-config`, so platform labels and IDs,
the product-name field, its relation operator, and input constraints are resolved from the latest
server configuration. Captured mappings are used only for offline dry-run plans. Do not use
`agent-reach`, curl, browser capture, or raw `api` subcommands to discover Lixiao requests or collect
candidates. A missing or changed contract must fail visibly inside the workflow.

For enterprise criteria, call `workflow search-conditions` first when the available filter names or
values are unknown, then pass one JSON object through `workflow advanced-search --condition`. The
object must have the captured root `cn`, `cr`, and `cv` fields; encode keywords such as industry,
company name, product, region, or qualification inside that condition tree. `--limit` defaults to
100, `--page-size` defaults to 10 and must be from 1 through 100. `tender-search` and
`channel-search` accept an optional condition JSON object and optional `--keyword`; their default
condition is an empty captured composite filter. `search-templates` lists saved templates and accepts
`--template-type` (default 0), `--page-size` (default 20), `--page-num` (default 1), and
`--search-name` (default empty). See [references/commands.md](references/commands.md) for the full
parameter table and input examples.

`company-profile` always applies the guarded product unlock when needed and aggregates the company
card, products, exhibitions, management, recruitment, intellectual property, brand, contact count,
and contacts. 搜索工作流不自动批量解锁候选企业。The Agent decides which specific enterprises are
relevant to the user's goal, then passes only those IDs to `company-profile`; it checks each selected
enterprise, unlocks only hidden products, and verifies visibility without a separate unlock question.

## Diagnostic APIs

Read [references/commands.md](references/commands.md) when selecting an operation or constructing a search body.

The `api` subcommands below are retained for manual diagnostics and workflow maintenance. Agents must
not compose them during normal sales-assistant execution.

Use required fields as named flags. Override or add query parameters with repeatable `--query KEY=VALUE`:

```powershell
uv run --no-project python $cli api company-card --id <company-id>
uv run --no-project python $cli api company-products --id <company-id> --query page=1
```

For the current enterprise, pass `--unlock-if-needed` directly when its hidden product details are required. The flag is sufficient authorization: let the CLI check visibility first, consume quota only if products are hidden, and retry the detail request once. Do not add it to bulk search results:

```powershell
uv run --no-project python $cli api company-products --id <company-id> --unlock-if-needed
```

For a platform-specific contact lookup source, override the default source explicitly:

```powershell
uv run --no-project python $cli api company-products --id <company-id> --unlock-if-needed --contact-source scene_search.searchEcommercePlatformEnterpriseAlibaba_detail
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
- The CLI owns the single automatic login and read replay. The Agent must not add another authentication
  retry. Retry other requests only when `retryable: true`; permission errors are not authentication.
- Do not unlock every company returned by a search. Apply `--unlock-if-needed` only to the current specific company.
- After a real unlock, inspect `contacts_after_unlock`. It contains the company card, contact count, and contact details, or a nested error if that follow-up permission is unavailable.
- `auth logout` removes the plaintext phone, password, tokens, cookies, and sessions in the Lixiao
  credential file.
