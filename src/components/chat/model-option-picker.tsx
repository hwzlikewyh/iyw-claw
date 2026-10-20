"use client"

import { useMemo, useState } from "react"
import { Box, ChevronDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { ModelOptionList } from "@/components/chat/model-option-list"
import { ModelIcon } from "@/components/chat/model-icon"
import { useScrollbarSafeDismiss } from "@/hooks/use-scrollbar-safe-dismiss"
import { cn } from "@/lib/utils"
import type { ModelOptionGroup } from "@/lib/model-config-groups"
import type { AgentType, SessionConfigOptionInfo } from "@/lib/types"

interface ModelOptionPickerProps {
  option: SessionConfigOptionInfo
  /** The grouped list to show (derived `provider/` groups, or a single
   *  headerless group for a long flat list). */
  groups: ModelOptionGroup[]
  behaviorOptions?: SessionConfigOptionInfo[]
  onSelect: (configId: string, valueId: string) => void
  onBehaviorSelect?: (
    modelValue: string,
    configId: string,
    valueId: string
  ) => void
  onOpen?: () => void
  agentType?: AgentType | null
}

// Model picker Popover with the searchable list and model behavior cascade.
// The dismiss guard handles WebKit bouncing focus outside during a scrollbar drag.
export function ModelOptionPicker({
  option,
  groups,
  behaviorOptions = [],
  onSelect,
  onBehaviorSelect,
  onOpen,
  agentType,
}: ModelOptionPickerProps) {
  const t = useTranslations("Folder.chat.messageInput")
  const [open, setOpen] = useState(false)
  const { contentRef, onPointerDownOutside, onFocusOutside } =
    useScrollbarSafeDismiss()
  const kind = option.kind.type === "select" ? option.kind : null
  const currentValue = kind?.current_value ?? ""
  const currentOption = useMemo(() => {
    for (const group of groups) {
      for (const opt of group.options) {
        if (opt.value === currentValue) return opt
      }
    }
    return null
  }, [groups, currentValue])
  const currentLabel =
    currentOption?.name ??
    (currentValue
      ? t("modelPicker.unavailable", { model: currentValue })
      : t("noModels"))

  if (!kind) return null

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
        if (next) onOpen?.()
      }}
    >
      <PopoverTrigger asChild>
        <Button
          variant="ghost"
          size="xs"
          title={option.name}
          aria-label={
            currentLabel ? `${option.name}: ${currentLabel}` : option.name
          }
          className="min-w-0 gap-1.5 rounded-full bg-primary/10 px-2.5 text-foreground hover:bg-primary/15"
        >
          {currentOption?.iconUrl ? (
            <ModelIcon src={currentOption.iconUrl} />
          ) : (
            <Box className="size-3.5 shrink-0" />
          )}
          <span className="max-w-[10rem] truncate">{currentLabel}</span>
          <ChevronDown
            className={cn(
              "size-3 shrink-0 text-muted-foreground transition-transform",
              open && "rotate-180"
            )}
          />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        ref={contentRef}
        side="top"
        align="start"
        onPointerDownOutside={onPointerDownOutside}
        onFocusOutside={onFocusOutside}
        className="w-auto max-w-[calc(100vw-1rem)] bg-transparent p-0 shadow-none ring-0"
      >
        <ModelOptionList
          groups={groups}
          agentType={agentType}
          currentValue={currentValue}
          onSelect={(value) => {
            onSelect(option.id, value)
            setOpen(false)
          }}
          searchPlaceholder={t("searchModel")}
          searchAriaLabel={t("searchModelAria")}
          listAriaLabel={t("modelListLabel")}
          emptyLabel={t("noModels")}
          behaviorOptions={behaviorOptions}
          onBehaviorSelect={(modelValue, configId, valueId) => {
            if (modelValue !== currentValue) {
              onSelect(option.id, modelValue)
            }
            onBehaviorSelect?.(modelValue, configId, valueId)
            setOpen(false)
          }}
          autoFocus
        />
      </PopoverContent>
    </Popover>
  )
}
