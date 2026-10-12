"use client"

import { useMemo, useState } from "react"
import { Check, ChevronRight } from "lucide-react"
import { useTranslations } from "next-intl"
import { cn } from "@/lib/utils"
import { CONTEXT_WINDOW_CONFIG_ID } from "@/lib/model-context-window"
import { useModelPanePosition } from "@/components/chat/use-model-pane-position"
import type {
  SessionConfigOptionInfo,
  SessionConfigSelectOptionInfo,
} from "@/lib/types"

interface ModelBehaviorMenuProps {
  options: SessionConfigOptionInfo[]
  onSelect?: (configId: string, valueId: string) => void
  compact?: boolean
}

function currentLabel(option: SessionConfigOptionInfo): string {
  return (
    option.kind.options.find(({ value }) => value === option.kind.current_value)
      ?.name ?? option.kind.current_value
  )
}

function BehaviorValueButton({
  value,
  selected,
  onSelect,
}: {
  value: SessionConfigSelectOptionInfo
  selected: boolean
  onSelect: () => void
}) {
  const t = useTranslations("Folder.chat.messageInput.modelPicker")
  return (
    <button
      type="button"
      aria-current={selected ? "true" : undefined}
      onClick={onSelect}
      className={cn(
        "flex w-full items-start gap-2 rounded-lg px-2 py-2 text-left text-sm hover:bg-accent",
        selected && "bg-primary/10"
      )}
    >
      <span className="min-w-0 flex-1">
        <span className="block truncate">
          {value.value === "auto"
            ? value.name.replace(/^Auto/, t("auto"))
            : value.name}
        </span>
        {value.description ? (
          <span className="mt-0.5 block text-xs text-muted-foreground">
            {value.description}
          </span>
        ) : null}
      </span>
      {selected ? (
        <Check className="mt-0.5 size-4 shrink-0 text-primary" />
      ) : null}
    </button>
  )
}

function BehaviorValues({
  option,
  name,
  onSelect,
  compact,
}: {
  option: SessionConfigOptionInfo
  name: string
  onSelect: NonNullable<ModelBehaviorMenuProps["onSelect"]>
  compact: boolean
}) {
  const t = useTranslations("Folder.chat.messageInput.modelPicker")
  const { paneRef, style } = useModelPanePosition(compact)
  return (
    <div
      ref={paneRef}
      style={style}
      className={cn("z-20 w-46 px-1", compact && "w-full")}
    >
      <div
        className="max-h-64 overflow-y-auto rounded-2xl border border-primary/10 bg-popover p-1 shadow-lg"
        role="group"
        aria-label={name}
      >
        <div className="px-2 py-1.5 text-xs text-muted-foreground">{name}</div>
        {option.kind.options.map((value) => (
          <BehaviorValueButton
            key={value.value}
            value={
              option.id === CONTEXT_WINDOW_CONFIG_ID
                ? { ...value, description: null }
                : value
            }
            selected={value.value === option.kind.current_value}
            onSelect={() => onSelect(option.id, value.value)}
          />
        ))}
        {option.id === CONTEXT_WINDOW_CONFIG_ID ? (
          <p className="border-t px-2 py-2 text-xs leading-5 text-muted-foreground">
            {t("contextHint")}
          </p>
        ) : null}
      </div>
    </div>
  )
}

export function ModelBehaviorMenu({
  options,
  onSelect,
  compact = false,
}: ModelBehaviorMenuProps) {
  const t = useTranslations("Folder.chat.messageInput.modelPicker")
  const visible = useMemo(
    () => options.filter((option) => option.kind.options.length > 0),
    [options]
  )
  const [activeId, setActiveId] = useState<string | null>(null)
  if (visible.length === 0) return null
  const active = visible.find(({ id }) => id === activeId)
  const optionName = (option: SessionConfigOptionInfo) =>
    option.id === CONTEXT_WINDOW_CONFIG_ID ? t("context") : option.name

  return (
    <div className={cn("relative bg-popover", compact && "w-full")}>
      <div className="py-1" aria-label={t("behavior")}>
        {visible.map((option) => (
          <button
            key={option.id}
            type="button"
            aria-expanded={active?.id === option.id}
            disabled={!onSelect || option.kind.options.length < 2}
            onMouseEnter={() => setActiveId(option.id)}
            onFocus={() => setActiveId(option.id)}
            onClick={() => setActiveId(option.id)}
            className={cn(
              "flex w-full items-center gap-2 rounded-md px-2 py-2 text-left text-sm",
              active?.id === option.id && "bg-accent text-accent-foreground"
            )}
          >
            <span className="min-w-0 flex-1 truncate">
              {optionName(option)}
            </span>
            <span className="text-xs text-muted-foreground">
              {option.kind.current_value === "auto"
                ? currentLabel(option).replace(/^Auto/, t("auto"))
                : currentLabel(option)}
            </span>
            {onSelect && option.kind.options.length > 1 ? (
              <ChevronRight className="size-3.5 shrink-0" />
            ) : null}
          </button>
        ))}
      </div>
      {active && onSelect ? (
        <BehaviorValues
          key={active.id}
          option={active}
          name={optionName(active)}
          onSelect={onSelect}
          compact={compact}
        />
      ) : null}
    </div>
  )
}
