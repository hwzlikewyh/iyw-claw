const VERSION = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/
const MAX_VERSION_BYTES = 64
const MAX_COMPONENT = 18446744073709551615n

export function minimumClientVersion(value) {
  if (value === undefined) return undefined
  const match = typeof value === "string" && value.match(VERSION)
  if (!match || match[0] !== value || value.length > MAX_VERSION_BYTES ||
      match.slice(1, 4).some(part => BigInt(part) > MAX_COMPONENT) ||
      match[4]?.split(".").some(part => /^\d+$/.test(part) && part.length > 1 && part.startsWith("0")))
    throw Error("minClientVersion must be a canonical SemVer string of at most 64 bytes")
  return value
}
