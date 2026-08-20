import { isModelConfigOption } from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"

export type OrderedSessionSelector =
  | { kind: "mode" }
  | { kind: "config"; option: SessionConfigOptionInfo }

function isReasoningOption(option: SessionConfigOptionInfo): boolean {
  const id = option.id.trim().toLowerCase().replace(/_/g, "-")
  return (
    id.includes("reasoning") || id.includes("thought") || id.includes("effort")
  )
}

export function orderSessionSelectors(
  showMode: boolean,
  options: SessionConfigOptionInfo[]
): OrderedSessionSelector[] {
  const models = options.filter(isModelConfigOption)
  const reasoning = options.filter(
    (option) => !isModelConfigOption(option) && isReasoningOption(option)
  )
  const others = options.filter(
    (option) => !isModelConfigOption(option) && !isReasoningOption(option)
  )
  return [
    ...(showMode ? ([{ kind: "mode" }] as const) : []),
    ...models.map((option) => ({ kind: "config" as const, option })),
    ...reasoning.map((option) => ({ kind: "config" as const, option })),
    ...others.map((option) => ({ kind: "config" as const, option })),
  ]
}
