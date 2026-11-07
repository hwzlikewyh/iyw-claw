import type {
  SkillMarketListQueryV2,
  SkillMarketV2Item,
  SkillMarketInstallState,
} from "./skill-market"

const INSTALL_ORDER: Record<SkillMarketInstallState, number> = {
  installed: 0,
  update_available: 1,
  blocked: 2,
  preparing: 3,
  not_installed: 4,
}

export function orderSkillCatalog(
  items: SkillMarketV2Item[],
  query: SkillMarketListQueryV2
): SkillMarketV2Item[] {
  const result = items.filter(
    (item) =>
      (!query.packageType ||
        (query.packageType === "plugin"
          ? item.packageType === "plugin"
          : item.packageType !== "plugin")) &&
      (query.compatibility === "all" ||
        item.compatibility === query.compatibility) &&
      (query.distribution === "all" ||
        item.distributionPolicy === query.distribution)
  )
  if (query.sort === "updated")
    return result.sort(
      (left, right) =>
        right.updatedAt.localeCompare(left.updatedAt) ||
        left.id.localeCompare(right.id)
    )
  if (query.sort === "name")
    return result.sort(
      (left, right) =>
        left.displayName.localeCompare(right.displayName) ||
        left.id.localeCompare(right.id)
    )
  if (query.sort === "installed")
    return result.sort(
      (left, right) =>
        INSTALL_ORDER[left.installState] - INSTALL_ORDER[right.installState] ||
        left.id.localeCompare(right.id)
    )
  return result
}
