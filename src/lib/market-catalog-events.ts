const CATALOG_CHANGED = "app://capability-market-catalog-changed"

export function notifyMarketCatalogChanged() {
  if (typeof window !== "undefined")
    window.dispatchEvent(new Event(CATALOG_CHANGED))
}

export function onMarketCatalogChanged(listener: () => void) {
  if (typeof window === "undefined") return () => {}
  window.addEventListener(CATALOG_CHANGED, listener)
  return () => window.removeEventListener(CATALOG_CHANGED, listener)
}
