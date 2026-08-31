"use client"

import { useMemo, useState } from "react"
import { ChevronDown } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { ModelOptionList } from "@/components/chat/model-option-list"
import { useScrollbarSafeDismiss } from "@/hooks/use-scrollbar-safe-dismiss"
import { cn } from "@/lib/utils"
import type { ModelOptionGroup } from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ModelOptionPickerProps {
  option: SessionConfigOptionInfo
  /** The grouped list to show (derived `provider/` groups, or a single
   *  headerless group for a long flat list). */
  groups: ModelOptionGroup[]
  behaviorOptions?: SessionConfigOptionInfo[]
  onSelect: (configId: string, valueId: string) => void
  onBehaviorSelect?: (configId: string, valueId: string) => void
}

// Model picker Popover with the searchable list and model behavior cascade.
// The dismiss guard handles WebKit bouncing focus outside during a scrollbar drag.
export function ModelOptionPicker({
  option,
  groups,
  behaviorOptions = [],
  onSelect,
  onBehaviorSelect,
}: ModelOptionPickerProps) {
  const t = useTranslations("Folder.chat.messageInput")
  const [open, setOpen] = useState(false)
  const { contentRef, onPointerDownOutside, onFocusOutside } =
    useScrollbarSafeDismiss()
  const kind = option.kind.type === "select" ? option.kind : null
  const currentValue = kind?.current_value ?? ""
  const currentLabel = useMemo(() => {
    for (const group of groups) {
      for (const opt of group.options) {
        if (opt.value === currentValue) return opt.name
      }
    }
    return currentValue
  }, [groups, currentValue])

  if (!kind) return null

  return (
    <Popover
      open={open}
      onOpenChange={(next) => {
        setOpen(next)
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
          className="min-w-0 gap-0.5 px-1 text-muted-foreground"
        >
          <span className="max-w-[10rem] truncate">{currentLabel}</span>
          <ChevronDown className="size-3 shrink-0 text-muted-foreground" />
        </Button>
      </PopoverTrigger>
      <PopoverContent
        ref={contentRef}
        side="top"
        align="start"
        onPointerDownOutside={onPointerDownOutside}
        onFocusOutside={onFocusOutside}
        className={cn(
          "max-w-[calc(100vw-1rem)] overflow-hidden p-0",
          behaviorOptions.length > 0 ? "w-auto" : "w-[22rem]"
        )}
      >
        <ModelOptionList
          groups={groups}
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
            onBehaviorSelect?.(configId, valueId)
            setOpen(false)
          }}
          autoFocus
        />
      </PopoverContent>
    </Popover>
  )
}
