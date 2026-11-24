import { usePluginPresentation } from "@/components/layout/plugin-presentation"
import { isTaskReference } from "../composer-commands"
import type { ReferenceAttrs } from "../types"

// 展示名称从市场目录补全；调用标识与发送序列化保持原值。
export function useReferenceSource(data: ReferenceAttrs) {
  const catalog = usePluginPresentation()
  if (data.refType !== "skill" || isTaskReference(data)) return null
  const source = data.meta?.composerSource
  const marketId = source?.marketId ?? data.meta?.marketSkillId
  const item =
    catalog.get(source?.slug ?? "") ??
    catalog.get(marketId ? "market:" + marketId : "") ??
    catalog.get(data.meta?.marketSkillSlug ?? data.id)
  const kind = source?.kind ?? item?.kind
  if (kind !== "plugin" && kind !== "skill") return null
  return {
    kind,
    label: item?.label.trim() || data.label || data.id,
    src: item?.iconUrl,
  }
}
