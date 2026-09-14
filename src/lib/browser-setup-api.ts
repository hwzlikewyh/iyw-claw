import { getTransport } from "@/lib/transport"

export function prepareBrowserExtension(): Promise<string> {
  return getTransport().call("internet_tools_prepare_extension")
}

export function openBrowserExtensionSettings(
  browser: "chrome" | "edge"
): Promise<void> {
  return getTransport().call("internet_tools_open_extension_settings", {
    browser,
  })
}
