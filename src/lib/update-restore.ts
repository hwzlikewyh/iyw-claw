// Restore-after-relaunch: when the desktop app relaunches itself (applying a
// staged update, or a settings action that requires a restart), the new
// process boots at the default entry route. Persisting the route the user was
// on lets the entry page put them right back, making the relaunch feel like a
// refresh instead of a reset.
//
// localStorage is the carrier: it survives the process swap (same webview
// origin) and writes synchronously, so saving immediately before the relaunch
// call cannot race the process exit. A separate last-route key is refreshed
// while the main window runs because an external installer can terminate the
// old process without giving the renderer a final callback.

const RELAUNCH_RESTORE_KEY = "iyw-claw.update-restore-route"
const INSTALLER_RESTORE_KEY = "iyw-claw.installer-restore-route"

// Generous enough for a slow update install + relaunch; short enough that a
// crashed relaunch doesn't teleport the user days later.
const RESTORE_TTL_MS = 30 * 60_000

type RestoreSource = "installer" | "relaunch"

interface RestorePayload {
  route: string
  savedAt: number
  source: RestoreSource
}

function isSafeAppRoute(route: string): boolean {
  if (!route.startsWith("/") || route.startsWith("//")) return false
  try {
    const parsed = new URL(route, window.location.origin)
    if (parsed.origin !== window.location.origin) return false
    if (parsed.pathname === "/" || parsed.pathname === "/index.html") {
      return false
    }
    return (
      !parsed.pathname.startsWith("/login") &&
      !parsed.pathname.startsWith("/pet")
    )
  } catch {
    return false
  }
}

function saveRoute(key: string, source: RestoreSource, route: string): void {
  if (!isSafeAppRoute(route)) return
  const payload: RestorePayload = { route, savedAt: Date.now(), source }
  window.localStorage.setItem(key, JSON.stringify(payload))
}

function consumeRoute(key: string): string | null {
  const raw = window.localStorage.getItem(key)
  if (!raw) return null
  window.localStorage.removeItem(key)
  const parsed = JSON.parse(raw) as Partial<RestorePayload>
  if (typeof parsed.route !== "string" || typeof parsed.savedAt !== "number") {
    return null
  }
  const age = Date.now() - parsed.savedAt
  if (age < 0 || age > RESTORE_TTL_MS) return null
  return isSafeAppRoute(parsed.route) ? parsed.route : null
}

export function saveRouteForRestore(): void {
  if (typeof window === "undefined") return
  try {
    const route = window.location.pathname + window.location.search
    saveRoute(RELAUNCH_RESTORE_KEY, "relaunch", route)
  } catch {
    // Storage unavailable — restore is best-effort.
  }
}

/** Refresh the main window route used only by an installer-triggered launch. */
export function rememberRouteForInstaller(route: string): void {
  if (typeof window === "undefined") return
  try {
    saveRoute(INSTALLER_RESTORE_KEY, "installer", route)
  } catch {
    // Storage unavailable — restore is best-effort.
  }
}

/** One-shot: returns the saved route (and clears it) if it is fresh and safe
 * to navigate to, else null. */
export function consumeRestoredRoute(): string | null {
  if (typeof window === "undefined") return null
  try {
    return consumeRoute(RELAUNCH_RESTORE_KEY)
  } catch {
    return null
  }
}

export function consumeInstallerRestoredRoute(): string | null {
  if (typeof window === "undefined") return null
  try {
    return consumeRoute(INSTALLER_RESTORE_KEY)
  } catch {
    return null
  }
}
