"use client"

import { Box, Check } from "lucide-react"
import { useTranslations } from "next-intl"
import { ModelIcon } from "@/components/chat/model-icon"
import { ModelPriceMultiplier } from "@/components/chat/model-price-multiplier"
import { cn } from "@/lib/utils"
import type { SessionConfigSelectOptionInfo } from "@/lib/types"

interface ModelOptionRowProps {
  option: SessionConfigSelectOptionInfo
  selected: boolean
  active: boolean
  id: string
  onPreview: () => void
  onSelect: () => void
}

function ModelOptionContent({
  option,
  selected,
}: Pick<ModelOptionRowProps, "option" | "selected">) {
  const t = useTranslations("Folder.chat.messageInput.modelPicker")
  return (
    <>
      <span
        className={cn("min-w-0 flex-1 truncate", selected && "font-semibold")}
      >
        {option.name}
      </span>
      {option.modelBehavior?.reasoningOptions.length ? (
        <span className="shrink-0 rounded-full bg-primary/10 px-1.5 text-[10px] text-primary">
          {t("reasoning")}
        </span>
      ) : null}
      <ModelPriceMultiplier
        value={option.priceMultiplier}
        className="w-12 text-[11px]"
      />
      <span className="flex size-3.5 shrink-0 items-center">
        {selected ? <Check className="size-3.5 text-primary" /> : null}
      </span>
    </>
  )
}

export function ModelOptionRow({
  option,
  selected,
  active,
  id,
  onPreview,
  onSelect,
}: ModelOptionRowProps) {
  return (
    <button
      type="button"
      role="option"
      id={id}
      aria-selected={selected}
      title={option.name}
      onMouseEnter={onPreview}
      onFocus={onPreview}
      onClick={onSelect}
      className={cn(
        "flex min-h-8 w-full items-center gap-2 rounded-lg px-2 py-1.5 text-left text-sm transition-colors",
        active && "bg-accent",
        selected && "bg-primary/10 text-foreground"
      )}
    >
      {option.iconUrl ? (
        <ModelIcon src={option.iconUrl} />
      ) : (
        <Box className="size-4 shrink-0 text-muted-foreground" />
      )}
      <ModelOptionContent option={option} selected={selected} />
    </button>
  )
}
