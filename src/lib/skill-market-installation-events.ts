const INSTALLATIONS_CHANGED = "app://skill-market-installations-changed"

export function notifySkillMarketInstallationsChanged() {
  if (typeof window === "undefined") return
  window.dispatchEvent(new Event(INSTALLATIONS_CHANGED))
}

export function onSkillMarketInstallationsChanged(listener: () => void) {
  if (typeof window === "undefined") return () => {}
  window.addEventListener(INSTALLATIONS_CHANGED, listener)
  return () => window.removeEventListener(INSTALLATIONS_CHANGED, listener)
}
