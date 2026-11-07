const VERSION =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/
const MAX_COMPONENT = 18446744073709551615n
const MAX_BYTES = 64

export function validatePluginMinimumVersion(
  manifest: Record<string, unknown>
) {
  const value = manifest.minClientVersion
  if (Object.prototype.hasOwnProperty.call(manifest, "minClientVersion")) {
    const match = typeof value === "string" && value.match(VERSION)
    if (
      !match ||
      match[0] !== value ||
      value.length > MAX_BYTES ||
      match.slice(1, 4).some((part) => BigInt(part) > MAX_COMPONENT) ||
      match[4]
        ?.split(".")
        .some(
          (part) =>
            /^\d+$/.test(part) && part.length > 1 && part.startsWith("0")
        ) ||
      manifest.schemaVersion !== 2
    )
      throw new Error("invalidPluginManifest")
  }
  const components = manifest.components as Record<string, unknown> | undefined
  if (
    manifest.schemaVersion === 2 &&
    Array.isArray(components?.apps) &&
    components.apps.length &&
    !value
  )
    throw new Error("invalidPluginManifest")
}
