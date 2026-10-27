import type { SessionConfigOptionInfo } from "@/lib/types"

export function isModelConfigOption(option: SessionConfigOptionInfo): boolean {
  return (
    option.id === "model" ||
    (option.category === "model" && option.id !== "provider")
  )
}

function normalizeModelId(value: string): string {
  return value
    .trim()
    .toLowerCase()
    .replace(/[\/_]+/g, "-")
    .replace(/-v\d+(?:\.\d+)?(?=-|$)/g, "")
}

export function currentModelName(
  options: SessionConfigOptionInfo[] | undefined
): string | null {
  const model = options?.find(isModelConfigOption)
  return model ? modelDisplayName(options, model.kind.current_value) : null
}

export function modelDisplayName(
  options: SessionConfigOptionInfo[] | undefined,
  value: string | null | undefined
): string | null {
  const currentValue = value?.trim()
  if (!currentValue) return null
  const model = options?.find(isModelConfigOption)
  if (!model) return currentValue
  const candidates = [
    ...model.kind.options,
    ...model.kind.groups.flatMap((group) => group.options),
  ]
  const normalized = normalizeModelId(currentValue)
  const matches = candidates.filter((option) => {
    const candidate = normalizeModelId(option.value)
    return (
      candidate === normalized ||
      candidate.startsWith(`${normalized}-`) ||
      normalized.startsWith(`${candidate}-`)
    )
  })
  return matches.length === 1
    ? matches[0].name.trim() || currentValue
    : currentValue
}
