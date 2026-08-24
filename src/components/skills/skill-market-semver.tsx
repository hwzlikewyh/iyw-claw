import type { SkillDependencyInput } from "@/lib/skill-market"

const MAX_SKILL_DEPENDENCIES = 16
const DEPENDENCY_SLUG_PATTERN = /^[a-z0-9](?:[a-z0-9-]{0,126}[a-z0-9])?$/
const SEMVER_PATTERN =
  /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9]\d*|\d*[A-Za-z-][0-9A-Za-z-]*))*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/

export type SkillDependencyValidation =
  | { valid: true }
  | { valid: false; line: number | null }

export function isValidSemVer(version: string): boolean {
  return SEMVER_PATTERN.test(version.trim())
}

type DependencyLine = { line: string; number: number }

function dependencyLines(value: string): DependencyLine[] {
  return value
    .split(/\r?\n/)
    .map((line, index) => ({ line: line.trim(), number: index + 1 }))
    .filter(({ line }) => Boolean(line))
}

function parseDependencyLine(
  line: string,
  seen: Set<string>
): SkillDependencyInput | null {
  const separator = line.lastIndexOf("@")
  const slug = line.slice(0, separator).trim()
  const version = line.slice(separator + 1).trim()
  if (
    separator <= 0 ||
    !DEPENDENCY_SLUG_PATTERN.test(slug) ||
    !isValidSemVer(version) ||
    seen.has(slug)
  ) {
    return null
  }
  seen.add(slug)
  return { slug, version }
}

export function parseSkillDependencies(value: string): SkillDependencyInput[] {
  const lines = dependencyLines(value)
  if (lines.length > MAX_SKILL_DEPENDENCIES) {
    throw new Error("tooManyDependencies")
  }
  const seen = new Set<string>()
  return lines.map(({ line }) => {
    const dependency = parseDependencyLine(line, seen)
    if (!dependency) throw new Error("invalidDependency")
    return dependency
  })
}

export function validateSkillDependencies(
  value: string
): SkillDependencyValidation {
  const lines = dependencyLines(value)
  if (lines.length > MAX_SKILL_DEPENDENCIES) {
    return { valid: false, line: null }
  }
  const seen = new Set<string>()
  for (const { line, number } of lines) {
    if (!parseDependencyLine(line, seen)) {
      return { valid: false, line: number }
    }
  }
  return { valid: true }
}

export function isValidSkillDependencies(value: string): boolean {
  return validateSkillDependencies(value).valid
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
