// Shared helpers for web-mode HTTP calls — the JSON transport in
// `web-transport.ts` and direct multipart/file callers in `lib/api.ts` both
// need consistent token retrieval and 401 redirect behavior. Keeping them in
// one place means a future move from `localStorage` to cookies (or rotation
// rules, multi-tenant prefixing, etc.) doesn't have to be remembered at every
// call site.

const TOKEN_KEY = "iyw-claw_token"
const DEVELOPMENT_SERVER_URL = "http://127.0.0.1:3080"
const DEVELOPMENT_ACCESS_TOKEN = "hwz123456"

function isDevelopment(): boolean {
  return process.env.NODE_ENV === "development"
}

export function getIywClawWebBaseUrl(): string {
  if (isDevelopment()) return DEVELOPMENT_SERVER_URL
  return window.location.origin
}

export function getIywClawDefaultLoginToken(): string {
  return isDevelopment() ? DEVELOPMENT_ACCESS_TOKEN : ""
}

export function getIywClawToken(): string {
  return localStorage.getItem(TOKEN_KEY) ?? ""
}

export function redirectToIywClawLogin(): void {
  if (window.location.pathname.startsWith("/login")) return
  localStorage.removeItem(TOKEN_KEY)
  window.location.href = "/login"
}
