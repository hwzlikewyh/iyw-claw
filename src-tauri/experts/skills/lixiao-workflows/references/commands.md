# Lixiao CLI Commands

## Contents

- Authentication commands
- Captured API operations
- JSON and query input
- Common sequences

## Authentication Commands

| Command | Purpose |
| --- | --- |
| `auth status` | Show the credential path and non-secret availability flags. |
| `auth set-app-token` | Read the Lixiao application token from hidden input and save it. |
| `auth set-business-token` | Read the business API token from hidden input and save it. |
| `auth qr-start` | Create a QR code and return its URL and code. |
| `auth qr-wait --code CODE` | Poll QR state, obtain the app session, and save cookies/access token. |
| `auth captcha` | Register a Geetest challenge for password login. |
| `auth password ...` | Log in with Geetest proof; prompt for but never save the password. |
| `auth app` | Refresh and save the application SSO access token. |
| `auth logout` | Remove only `.iyw-claw/credentials.json`. |

The SSO access token and business API token are distinct in the captured traffic. Configure the business token separately when required.

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
| `scene-search` | POST | `--body` | Run the captured general scene search. |
| `company-card` | GET | `--id` | Get the company business card. |
| `company-exhibitions` | GET | `--id` | Get exhibition information. |
| `permission-info` | GET | optional `types` query | Check view/import/search permissions. |
| `phone-call-list` | POST | `--body` | Query call data using `pid` and `phoneNumbers`. |
| `company-contacts` | GET | `--pid`, `--ent-name` | Get company contacts. |
| `company-products` | GET | `--id` | Get paged shop products. |
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
5. Call `company-unlock` only after confirming quota and user intent.
