"use client"

import { ModelBehaviorMenu } from "@/components/chat/model-behavior-menu"
import { ModelPriceMultiplier } from "@/components/chat/model-price-multiplier"
import { useModelPanePosition } from "@/components/chat/use-model-pane-position"
import { useTranslations } from "next-intl"
import { getCachedGatewayModels } from "@/lib/gateway-model-catalog"
import {
  contextWindowLabel,
  CONTEXT_WINDOW_CONFIG_ID,
} from "@/lib/model-context-window"
import { cn } from "@/lib/utils"
import type {
  AgentType,
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"

interface ModelBehaviorPreviewPanelProps {
  model: SessionConfigSelectOptionInfo
  agentType?: AgentType | null
  options: SessionConfigOptionInfo[]
  onSelect?: (modelValue: string, configId: string, valueId: string) => void
  compact: boolean
  offset?: number
}

function ModelDetailSummary({
  model,
  agentType,
  options,
}: Pick<ModelBehaviorPreviewPanelProps, "model" | "agentType" | "options">) {
  const t = useTranslations("Folder.chat.messageInput.modelPicker")
  const catalogModel = getCachedGatewayModels(agentType ?? undefined).find(
    (item) => item.id === model.value
  )
  const maximum = catalogModel?.contextWindow
  return (
    <>
      <div className="space-y-1 border-b border-primary/10 px-2 pt-1 pb-3">
        <div className="text-sm font-medium break-words">{model.name}</div>
        {model.description ? (
          <p className="text-xs leading-5 text-muted-foreground break-words">
            {model.description}
          </p>
        ) : null}
      </div>
      {model.priceMultiplier != null ? (
        <div className="flex items-center justify-between gap-2 px-2 py-2.5 text-sm">
          <span>{t("consumption")}</span>
          <ModelPriceMultiplier value={model.priceMultiplier} />
        </div>
      ) : null}
      {maximum &&
      !options.some((option) => option.id === CONTEXT_WINDOW_CONFIG_ID) ? (
        <div className="flex items-center justify-between gap-2 px-2 py-2.5 text-sm">
          <span>{t("context")}</span>
          <span className="text-xs text-muted-foreground">
            {contextWindowLabel(maximum)}
          </span>
        </div>
      ) : null}
    </>
  )
}

export function ModelBehaviorPreviewPanel(
  props: ModelBehaviorPreviewPanelProps
) {
  const { model, options, onSelect, compact, offset } = props
  const { paneRef, style } = useModelPanePosition(compact, offset)
  return (
    <div
      data-model-pane
      ref={paneRef}
      style={style}
      className={cn("z-10 w-[15.5rem] max-w-full px-1", compact && "w-full")}
    >
      <div
        className={cn(
          "w-60 max-w-full rounded-2xl border border-primary/10 bg-popover p-2 text-popover-foreground shadow-lg",
          compact && "w-full"
        )}
      >
        <ModelDetailSummary {...props} />
        <ModelBehaviorMenu
          options={options}
          onSelect={
            onSelect
              ? (configId, valueId) => onSelect(model.value, configId, valueId)
              : undefined
          }
          compact={compact}
        />
      </div>
    </div>
  )
}
