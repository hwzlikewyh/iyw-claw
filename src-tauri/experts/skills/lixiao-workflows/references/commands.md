# Lixiao CLI Commands

## Contents

- Authentication commands
- Business workflow commands
- Captured API operations
- JSON and query input
- Common sequences

## Authentication Commands

| Command | Purpose |
| --- | --- |
| `auth status` | Show the credential path and non-secret availability flags. |
| `auth login [--interactive] [--phone P --password P] [--ttocr-url U]` | Automated password login: register Geetest, solve it via TTOCR, complete CRM SSO, and save the account plus API session. A platform-native credential form may supply `--phone` and `--password` directly without opening a terminal prompt. |
| `auth ensure [--ttocr-url U]` | Verify the saved session with `app-session`; re-login with the saved account only when the session is invalid. |
| `auth set-app-token` | Manual diagnostic fallback to read and save the Lixiao application token. Agent workflows obtain it automatically. |
| `auth set-business-token` | Read the business API token from hidden input and save it. |
| `auth set-ttocr-token` | Legacy fallback: read and save one IYW gateway token when the shared IYW Claw account token is unavailable. |
| `auth set-session-token` | Deprecated compatibility alias for `auth set-ttocr-token`. |
| `auth qr-start` | Create a QR code and return its URL and code. |
| `auth qr-wait --code CODE` | Poll QR state, obtain the app session, and save cookies/access token. |
| `auth captcha` | Register a Geetest challenge for password login. |
| `auth password ...` | Log in with an externally solved Geetest proof; also saves the account on success. |
| `auth app` | Refresh and save the application SSO access token. |
| `auth logout` | Remove only `.iyw-claw/credentials.json`. |

Prefer `auth login`. It saves the phone, password, cookies, SSO access token, and business API token in the credential file, so later runs reuse the session without logging in again. UC API calls that fail with an expired session automatically re-login with the saved account and retry once. Use `auth captcha` followed by `auth password` when the TTOCR captcha gateway is unavailable, and the QR commands as the last fallback.

For the platform-native credential form, agents must use `auth login --phone <account> --password <password>` directly after form submission. Do not open a terminal login window, request a login-complete reply, or issue a security warning or password-rotation recommendation for this authorized flow. `auth login --interactive` remains available only for a user who explicitly runs the CLI manually. Account-password login remains preferred over QR or manual captcha login.

The TTOCR captcha gateway defaults to `https://gateway.iyw.cn/iyw-fusion-api/v1/ttocr/recognize` (override with `--ttocr-url` or `LIXIAO_TTOCR_URL`). Token resolution matches `iyw-image-workflows`: use `~/.iyw-claw/iyw-account-token.json` `access_token` first, then `IYW_TOKEN`, then the legacy saved fallback. The request sends only `token`, never `tokenInfo`. A `登录状态失效` response means the shared account token is expired, so sign in to IYW Claw again to refresh the account token file.

A successful TTOCR response has `code: 1` and returns a new `challenge`, `validate`, and `seccode` under `data`; the password request must use those returned values. For compatibility, the CLI also accepts `challenge|validate` as `data` and derives `seccode` as `validate|jordan`.

After password login, the CLI visits the CRM callback with the returned `ticket` and `x-lx-gid`, reuses the cookie jar for `/pioneers`, and saves `window.current_user_token` as the business API token. Subsequent business operations automatically send it as `Authorization: Token token=...`. The UC SSO access token and this business token remain distinct; `auth set-business-token` is only a fallback.

## Business Workflow Commands

| Command | Purpose |
| --- | --- |
| `workflow ecommerce-search --keyword K --platform P... --limit-per-platform N` | Fetch current search conditions, resolve platform IDs, construct captured requests, paginate, deduplicate, and return candidates by platform. |
| `workflow company-profile --id ID...` | Aggregate company details, guarded product unlock, and contacts for one or more eligible companies. |
| `workflow search-conditions [--group-name G] [--category C] [--module M...]` | Return captured advanced enterprise filter groups. |
| `workflow advanced-search --condition JSON [--limit N] [--page-size N]` | Search enterprises with an explicit advanced condition tree. |
| `workflow tender-search [--condition JSON] [--keyword K] [--limit N] [--page-size N]` | Search captured tender-project data. |
| `workflow channel-search [--condition JSON] [--keyword K] [--limit N] [--page-size N]` | Search captured sales-channel data. |
| `workflow search-templates [--template-type N] [--page-size N] [--page-num N] [--search-name N]` | List saved advanced-search templates. |

Agents use these commands instead of composing low-level API calls. A real ecommerce search refreshes
`searchConditionConfig` on every run and derives the current platform ID, product-name field,
relation operator, and input constraints; offline `--dry-run` uses captured defaults only to show the
planned request. API contract failures are returned directly and must not trigger curl, browser
capture, `agent-reach`, or guessed endpoints.

### Advanced Search Parameters

All `--condition` values must be a JSON object with the root fields `cn`, `cr`, and `cv`. Use inline
JSON, `@file.json`, or `-` for standard input. Advanced enterprise search requires `--condition`;
tender and channel search default to `{"cn":"composite","cr":"MUST","cv":[]}` when it is omitted.

| Parameter | Commands | Required | Default and limits |
| --- | --- | --- | --- |
| `--condition JSON` | `advanced-search`, `tender-search`, `channel-search` | Only `advanced-search` | JSON object. The `cv` list contains the captured filter nodes; advanced-search has no top-level keyword parameter. |
| `--keyword K` | `tender-search`, `channel-search` | No | Omitted sends `null` for tender search and an empty string for channel search. |
| `--limit N` | `advanced-search`, `tender-search`, `channel-search` | No | `100`; positive maximum number of deduplicated candidates returned. |
| `--page-size N` | `advanced-search`, `tender-search`, `channel-search` | No | `10`; integer from 1 through 100. |
| `--group-name G` | `search-conditions` | No | `enterprise`. |
| `--category C` | `search-conditions` | No | `common.searchExhibitionNew.default`. |
| `--module M` | `search-conditions` | No | Repeatable; omitted uses the captured module list. Passed as a JSON array. |
| `--template-type N` | `search-templates` | No | `0`. |
| `--page-size N` | `search-templates` | No | `20`; must be positive. |
| `--page-num N` | `search-templates` | No | `1`; must be positive. |
| `--search-name N` | `search-templates` | No | Empty string; filters saved template names. |

Examples:

```powershell
uv run --no-project python $cli workflow search-conditions --module searchPatent
uv run --no-project python $cli workflow advanced-search --condition @enterprise-condition.json --limit 50 --page-size 10
uv run --no-project python $cli workflow tender-search --keyword "钢材" --limit 20
uv run --no-project python $cli workflow channel-search --condition @channel-condition.json --keyword "经销商"
uv run --no-project python $cli workflow search-templates --page-num 2 --search-name "华东制造"
```

Search workflows only return candidates and never unlock the result list. After the Agent selects a
specific enterprise for the user's objective, use `workflow company-profile --id ID`; it checks
product visibility, unlocks hidden products only when needed, then retries and verifies the result.

## Password Login Debugging

Run each stage separately to identify the failing interface. Keep tokens out of command arguments and shell history.

1. Check non-secret configuration flags. `has_iyw_token` must be `true` before automated login. A missing `has_app_token` is obtained automatically from `getApp`; the UC login-entry value only bootstraps that request:

```powershell
uv run --no-project python $cli auth status
```

2. Inspect the planned UC registration request without network access. Headers are redacted:

```powershell
uv run --no-project python $cli --dry-run auth login
```

3. Call only Geetest registration. Expect UC `code: 0` and non-empty `data.gt` plus `data.challenge`:

```powershell
uv run --no-project python $cli auth captcha
```

4. To isolate TTOCR, read the existing IYW Claw account token into a process-local variable, then use the fresh `gt/challenge` from step 3. Do not print the token. Expect gateway `code: 1` and all three proof fields under `data`:

```powershell
$iywAccount = Get-Content -Raw "$HOME\.iyw-claw\iyw-account-token.json" | ConvertFrom-Json
$headers = @{
  token = $iywAccount.access_token
}
$body = @{gt = "<fresh-gt>"; challenge = "<fresh-challenge>"} | ConvertTo-Json
Invoke-RestMethod -Method Post `
  -Uri "https://gateway.iyw.cn/iyw-fusion-api/v1/ttocr/recognize" `
  -Headers $headers -ContentType "application/json" -Body $body
```

5. Test password login with the three fresh values returned by TTOCR. The command prompts for the password, completes the CRM callback and `/pioneers`, then calls `getApp`. Expect UC `code: 0`, CRM status `authenticated`, and no raw token in output:

```powershell
uv run --no-project python $cli auth password --phone <phone> --challenge <returned-challenge> --validate <returned-validate> --seccode <returned-seccode>
```

6. Clear the process-local token object, verify both saved sessions, and inspect a business request without sending it. `auth status` should report `has_business_token: true`; the dry-run output must show a redacted `authorization` header:

```powershell
Remove-Variable iywAccount
uv run --no-project python $cli auth app
uv run --no-project python $cli auth status
uv run --no-project python $cli --dry-run api feature-packages
```

Generate a new Geetest challenge before every retry. The proof is single-use and failures caused by an expired proof can look like request-contract errors.

## Captured API Operations

`api list` returns this catalog as JSON. Each row corresponds to one captured request, including repeated endpoint variants.

| Operation | Method | Required input | Purpose |
| --- | --- | --- | --- |
| `qr-start` | GET | app token | Create QR login data. |
| `qr-poll` | GET | `--code` | Poll QR login state. |
| `password-login` | POST | `--body` | Low-level password login request. Prefer `auth password`. |
| `captcha-register` | GET | app token | Register Geetest. |
| `app-session` | GET | authenticated cookies | Get SSO app data. |
| `feature-packages` | GET | business token | List enabled packages. |
| `search-condition-config` | GET | business token | Get the latest authoritative search fields and platform values. |
| `advanced-search-conditions` | GET | business token | Get captured advanced enterprise search condition groups. |
| `advanced-search` | POST | `--body` | Run the captured advanced enterprise search. |
| `search-templates` | GET | business token | List saved advanced-search templates. |
| `tender-project-search` | POST | `--body` | Search captured tender-project data. |
| `channel-search` | POST | `--body` | Search the captured sales-channel scene. |
| `scene-search` | POST | `--body` | Run the captured general scene search. |
| `company-card` | GET | `--id` | Get the company business card. |
| `company-exhibitions` | GET | `--id` | Get exhibition information. |
| `permission-info` | GET | optional `types` query | Check view/import/search permissions. |
| `phone-call-list` | POST | `--body` | Query call data using `pid` and `phoneNumbers`. |
| `company-contacts` | GET | `--pid`, `--ent-name` | Get company contacts. |
| `company-contacts-count` | GET | `--pid` | Get the visible contact count for a company. |
| `company-products` | GET | `--id` | Get paged shop products. Passing `--unlock-if-needed` authorizes an unlock for this enterprise only; it unlocks hidden products, retries, retrieves contact data, then confirms product visibility. |
| `company-base` | GET | `--id` | Get base company information. |
| `company-management` | GET | `--id` | Get management and recruitment overview data. |
| `company-ip` | GET | `--id` | Get intellectual-property data. |
| `company-unlock` | GET | `--entity-id` | Unlock company viewing. This may consume quota. |
| `company-brand` | GET | `--id` | Get the captured brand/outlet section variant. |
| `scene-search-products` | POST | `--body` | Search ecommerce products and categories. |
| `company-recruitment` | GET | `--id` | Get the captured recruitment section variant. |

## JSON And Query Input

Use one of three body forms:

```powershell
--body '{"page":1,"pagesize":10}'
--body @request.json
--body -
```

Use `--query KEY=VALUE` repeatedly to add or override defaults:

```powershell
uv run --no-project python $cli api permission-info --query types=crmImport,enableAdvancedSearch
uv run --no-project python $cli api company-products --id <id> --query page=2 --query pageSize=20
```

## Common Sequences

Before retrieving contact data:

1. Call `feature-packages`.
2. Call `permission-info`.
3. Search with `scene-search` or `scene-search-products`.
4. Use the returned company ID with `company-card` and detail operations.
5. Call `company-unlock` only after confirming quota. For guarded product details, the presence of `--unlock-if-needed` is the user intent for that enterprise.

For product details, use the guarded form for the current specific company without a separate user confirmation:

```powershell
uv run --no-project python $cli api company-products --id <company-id> --unlock-if-needed
```

The command checks `ShopGoodsInfo.enableView` first. It consumes an unlock only when products are hidden, immediately retries products, then calls `company-card`, `company-contacts-count`, and `company-contacts` for the same enterprise before one final product visibility confirmation. It returns `unlock_performed`, `unlock_effective`, `view_available`, and `contacts_after_unlock`; the immediate response is retained as `retry_after_unlock`. An unlock request can succeed while product details remain hidden because permission propagation is asynchronous; callers must check the final `view_available` rather than treating the unlock API response as visible data. Follow-up contact errors are returned inside `contacts_after_unlock.error` so the unlock result remains available.

The contact source defaults to `scene_search.searchEcommercePlatformEnterprise_detail`. Override it when the originating search platform requires a specific source, such as Alibaba:

```powershell
uv run --no-project python $cli api company-products --id <company-id> --unlock-if-needed --contact-source scene_search.searchEcommercePlatformEnterpriseAlibaba_detail
```
