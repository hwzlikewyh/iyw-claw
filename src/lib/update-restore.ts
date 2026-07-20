// Restore-after-relaunch: when the desktop app relaunches itself (applying a
// staged update, or a settings action that requires a restart), the new
// process boots at the default entry route. Persisting the route the user was
// on lets the entry page put them right back, making the relaunch feel like a
// refresh instead of a reset.
//
// localStorage is the carrier: it survives the process swap (same webview
// origin) and writes synchronously, so saving immediately before the relaunch
// call cannot race the process exit.

const RESTORE_KEY = "iyw-claw.update-restore-route"

// Generous enough for a slow update install + relaunch; short enough that a
// crashed relaunch doesn't teleport the user days later.
const RESTORE_TTL_MS = 30 * 60_000

export function saveRouteForRestore(): void {
  if (typeof window === "undefined") return
  try {
    const route = window.location.pathname + window.location.search
    // Auxiliary windows (pet) and the login page boot their own fixed routes;
    // restoring into them from the main window would be wrong.
    if (route.startsWith("/login") || route.startsWith("/pet")) return
    window.localStorage.setItem(
      RESTORE_KEY,
      JSON.stringify({ route, savedAt: Date.now() })
    )
  } catch {
    // Storage unavailable — restore is best-effort.
  }
}

/** One-shot: returns the saved route (and clears it) if it is fresh and safe
 * to navigate to, else null. */
export function consumeRestoredRoute(): string | null {
  if (typeof window === "undefined") return null
  try {
    const raw = window.localStorage.getItem(RESTORE_KEY)
    if (!raw) return null
    window.localStorage.removeItem(RESTORE_KEY)
    const parsed = JSON.parse(raw) as { route?: unknown; savedAt?: unknown }
    if (
      typeof parsed.route !== "string" ||
      typeof parsed.savedAt !== "number"
    ) {
      return null
    }
    if (Date.now() - parsed.savedAt > RESTORE_TTL_MS) return null
    // Same-origin app routes only — never an absolute/protocol-relative URL.
    if (!parsed.route.startsWith("/") || parsed.route.startsWith("//")) {
      return null
    }
    if (parsed.route.startsWith("/login") || parsed.route.startsWith("/pet")) {
      return null
    }
    return parsed.route
  } catch {
    return null
  }
}
