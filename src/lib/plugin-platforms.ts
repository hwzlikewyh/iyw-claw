export const PLUGIN_PLATFORM_LABELS = {
  "windows-x86_64": "Windows x64",
  "windows-aarch64": "Windows ARM64",
  "macos-x86_64": "macOS Intel",
  "macos-aarch64": "macOS Apple Silicon",
  "linux-x86_64": "Linux x64",
  "linux-aarch64": "Linux ARM64",
  "windows7-x86_64": "Windows 7 x64",
  "windows7-i686": "Windows 7 x86",
} as const

export function normalizedPluginPlatforms(value: unknown): string[] {
  if (value == null) return []
  if (
    !Array.isArray(value) ||
    value.length > Object.keys(PLUGIN_PLATFORM_LABELS).length ||
    new Set(value).size !== value.length ||
    value.some(
      (key) =>
        typeof key !== "string" ||
        !Object.prototype.hasOwnProperty.call(PLUGIN_PLATFORM_LABELS, key)
    )
  )
    throw new Error("invalidPluginManifest")
  return [...value].sort()
}

export function pluginPlatformLabel(value: string): string {
  if (!Object.prototype.hasOwnProperty.call(PLUGIN_PLATFORM_LABELS, value))
    return value
  return (
    PLUGIN_PLATFORM_LABELS[value as keyof typeof PLUGIN_PLATFORM_LABELS] ||
    value
  )
}
