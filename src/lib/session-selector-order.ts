import { isModelConfigOption } from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"

export type OrderedSessionSelector =
  | { kind: "mode" }
  | { kind: "config"; option: SessionConfigOptionInfo }

export function orderSessionSelectors(
  showMode: boolean,
  options: SessionConfigOptionInfo[]
): OrderedSessionSelector[] {
  const models = options.filter(isModelConfigOption)
  const others = options.filter((option) => !isModelConfigOption(option))
  return [
    ...(showMode ? ([{ kind: "mode" }] as const) : []),
    ...models.map((option) => ({ kind: "config" as const, option })),
    ...others.map((option) => ({ kind: "config" as const, option })),
  ]
}
