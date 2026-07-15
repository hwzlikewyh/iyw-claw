import type { PermissionOptionInfo } from "@/lib/types"

export type PermissionOptionLabelKey =
  | "allowOnce"
  | "allowForSession"
  | "allowCommandsStartingWith"
  | "allowAlways"
  | "reject"
  | "rejectAlways"

export interface PermissionOptionLabel {
  key: PermissionOptionLabelKey
  command?: string
}

const COMMAND_PREFIX = /^Allow Commands Starting With\s+(.+)$/i

export function resolvePermissionOptionLabel(
  option: PermissionOptionInfo
): PermissionOptionLabel | null {
  const command = option.name.match(COMMAND_PREFIX)?.[1]
  if (command) return { key: "allowCommandsStartingWith", command }

  switch (option.kind) {
    case "allow_once":
      return { key: "allowOnce" }
    case "allow_always":
      return /session/i.test(`${option.option_id} ${option.name}`)
        ? { key: "allowForSession" }
        : { key: "allowAlways" }
    case "reject_once":
      return { key: "reject" }
    case "reject_always":
      return { key: "rejectAlways" }
    default:
      return null
  }
}
