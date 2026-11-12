import type { SkillMarketV2Item } from "@/lib/skill-market"

export function capabilityStatus(
  item: SkillMarketV2Item
):
  | "building"
  | "buildFailed"
  | "incompatible"
  | "update"
  | "blocked"
  | "preparing"
  | null {
  if (item.currentVersion.status === "artifact_pending") return "building"
  if (item.currentVersion.status === "failed") return "buildFailed"
  if (item.compatibility === "incompatible") return "incompatible"
  if (item.installState === "update_available") return "update"
  if (item.installState === "blocked") return "blocked"
  if (item.installState === "preparing") return "preparing"
  return null
}
