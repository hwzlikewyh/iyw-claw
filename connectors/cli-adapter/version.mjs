import { execute } from "./process.mjs"

const VERSION_TIMEOUT_MS = 10_000
const VERSION_PATTERN = "([0-9]+(?:\\.[0-9]+){2,})"

function atLeast(actual, minimum) {
  const left = actual.split(".").map(Number)
  const right = minimum.split(".").map(Number)
  if ([...left, ...right].some((part) => !Number.isSafeInteger(part)))
    return false
  for (let index = 0; index < Math.max(left.length, right.length); index++) {
    const difference = (left[index] ?? 0) - (right[index] ?? 0)
    if (difference !== 0) return difference > 0
  }
  return true
}

export async function verifyVersion(context, command) {
  const expected = context.manifest.versionCheck
  const result = await execute(context, command, {
    timeoutMs: VERSION_TIMEOUT_MS,
  })
  const match = new RegExp(expected.pattern || VERSION_PATTERN).exec(
    `${result.stdout}\n${result.stderr}`
  )
  const actual = match?.[1] ?? match?.[0]
  if (result.code !== 0 || !actual || !atLeast(actual, expected.minimum)) {
    throw new Error(
      `连接器版本检查未通过，需要 ${expected.minimum} 或更新版本；退出码 ${result.code}，检测版本 ${actual || "无法解析"}`
    )
  }
}
