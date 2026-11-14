const INSTALLATIONS_CHANGED = "app://skill-market-installations-changed"
let channel: BroadcastChannel | null = null

function installationChannel() {
  if (typeof window === "undefined" || typeof BroadcastChannel === "undefined")
    return null
  if (!channel) {
    channel = new BroadcastChannel(INSTALLATIONS_CHANGED)
    channel.onmessage = () =>
      window.dispatchEvent(new Event(INSTALLATIONS_CHANGED))
  }
  return channel
}

export function notifySkillMarketInstallationsChanged() {
  if (typeof window === "undefined") return
  window.dispatchEvent(new Event(INSTALLATIONS_CHANGED))
  installationChannel()?.postMessage("changed")
}

export function onSkillMarketInstallationsChanged(listener: () => void) {
  if (typeof window === "undefined") return () => {}
  installationChannel()
  window.addEventListener(INSTALLATIONS_CHANGED, listener)
  return () => window.removeEventListener(INSTALLATIONS_CHANGED, listener)
}
