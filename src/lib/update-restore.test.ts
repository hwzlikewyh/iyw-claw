import { beforeEach, describe, expect, it, vi } from "vitest"

import { consumeRestoredRoute, saveRouteForRestore } from "@/lib/update-restore"

const RESTORE_KEY = "iyw-claw.update-restore-route"

function setLocation(pathname: string, search = "") {
  window.history.replaceState(null, "", `${pathname}${search}`)
}

describe("update-restore", () => {
  beforeEach(() => {
    window.localStorage.clear()
    setLocation("/workspace", "?session=abc")
    vi.restoreAllMocks()
  })

  it("round-trips the current route through a save + consume", () => {
    setLocation("/settings/system", "?tab=network")
    saveRouteForRestore()
    expect(consumeRestoredRoute()).toBe("/settings/system?tab=network")
  })

  it("consuming clears the stored route (one-shot)", () => {
    saveRouteForRestore()
    expect(consumeRestoredRoute()).toBe("/workspace?session=abc")
    expect(consumeRestoredRoute()).toBeNull()
  })

  it("does not save auxiliary or login routes", () => {
    setLocation("/login")
    saveRouteForRestore()
    expect(consumeRestoredRoute()).toBeNull()

    setLocation("/pet-panel")
    saveRouteForRestore()
    expect(consumeRestoredRoute()).toBeNull()
  })

  it("rejects stale saved routes", () => {
    saveRouteForRestore()
    const saved = JSON.parse(window.localStorage.getItem(RESTORE_KEY) ?? "{}")
    window.localStorage.setItem(
      RESTORE_KEY,
      JSON.stringify({ ...saved, savedAt: Date.now() - 31 * 60_000 })
    )
    expect(consumeRestoredRoute()).toBeNull()
  })

  it("rejects malformed or non-app-route payloads", () => {
    window.localStorage.setItem(RESTORE_KEY, "not json")
    expect(consumeRestoredRoute()).toBeNull()

    window.localStorage.setItem(
      RESTORE_KEY,
      JSON.stringify({ route: "https://evil.example", savedAt: Date.now() })
    )
    expect(consumeRestoredRoute()).toBeNull()

    window.localStorage.setItem(
      RESTORE_KEY,
      JSON.stringify({ route: "//evil.example", savedAt: Date.now() })
    )
    expect(consumeRestoredRoute()).toBeNull()

    window.localStorage.setItem(
      RESTORE_KEY,
      JSON.stringify({ route: "/login?next=x", savedAt: Date.now() })
    )
    expect(consumeRestoredRoute()).toBeNull()
  })
})
