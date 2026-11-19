import { ComposerSourceIcon } from "./composer-source-chip"
import { usePluginPresentation } from "@/components/layout/plugin-presentation"

export function ComposerSkillIcon({
  id,
  label,
  marketId,
}: {
  id: string
  label: string
  marketId?: string | null
}) {
  const catalog = usePluginPresentation()
  const item = marketId
    ? (catalog.get("market:" + marketId) ?? catalog.get(id))
    : catalog.get(id)
  return <ComposerSourceIcon kind="skill" label={label} src={item?.iconUrl} />
}
