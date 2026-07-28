const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/

export function isValidSemVer(version: string): boolean {
  return SEMVER_PATTERN.test(version.trim())
}

function parseSemVer(value: string) {
  const withoutBuild = value.split("+", 1)[0]
  const separator = withoutBuild.indexOf("-")
  const core = separator < 0 ? withoutBuild : withoutBuild.slice(0, separator)
  const pre = separator < 0 ? "" : withoutBuild.slice(separator + 1)
  return {
    core: core.split(".").map(BigInt),
    pre: pre.split(".").filter(Boolean),
  }
}

export function compareSemVer(left: string, right: string): number {
  const a = parseSemVer(left)
  const b = parseSemVer(right)
  for (let index = 0; index < 3; index += 1) {
    if (a.core[index] !== b.core[index]) {
      return a.core[index] > b.core[index] ? 1 : -1
    }
  }
  if (!a.pre.length || !b.pre.length) {
    if (a.pre.length === b.pre.length) return 0
    return a.pre.length ? -1 : 1
  }
  for (
    let index = 0;
    index < Math.max(a.pre.length, b.pre.length);
    index += 1
  ) {
    if (a.pre[index] == null || b.pre[index] == null) {
      return a.pre[index] == null ? -1 : 1
    }
    if (a.pre[index] === b.pre[index]) continue
    const aNumber = /^\d+$/.test(a.pre[index])
    const bNumber = /^\d+$/.test(b.pre[index])
    if (aNumber && bNumber) {
      return BigInt(a.pre[index]) > BigInt(b.pre[index]) ? 1 : -1
    }
    if (aNumber !== bNumber) return aNumber ? -1 : 1
    return a.pre[index].localeCompare(b.pre[index]) > 0 ? 1 : -1
  }
  return 0
}
