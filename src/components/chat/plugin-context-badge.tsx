import { ComposerSourceChip } from "./composer-source-chip"
import { usePluginPresentation } from "@/components/layout/plugin-presentation"

export function PluginContextBadge({
  label,
  slug,
  className,
  onClick,
  onRemove,
  removeLabel,
}: {
  label: string
  slug?: string
  className?: string
  onClick?: () => void
  onRemove?: () => void
  removeLabel?: string
}) {
  const item = usePluginPresentation().get(slug ?? "")
  return (
    <ComposerSourceChip
      kind="plugin"
      label={item?.label.trim() || label}
      src={item?.iconUrl}
      className={className}
      onOpen={onClick}
      onRemove={onRemove}
      removeLabel={removeLabel}
    />
  )
}
