type InstalledBrand = {
  brandId: string
  displayName: string
  logoUrl?: string | null
  urlScheme: string
}

declare global {
  interface Window {
    __IYW_BRAND__?: InstalledBrand
  }
}

const installed =
  typeof window === "undefined" ? undefined : window.__IYW_BRAND__
export const APP_NAME = installed?.displayName || "原助理"
export const APP_LOGO = installed?.logoUrl || "/icon.svg"
export const APP_SCHEME = installed?.urlScheme || "iyw-assistant"
