"use client"

import { useCallback, useId, useMemo, useRef, useState } from "react"
import { Search } from "lucide-react"
import { Virtualizer, type VirtualizerHandle } from "virtua"
import { cn } from "@/lib/utils"
import { ScrollArea } from "@/components/ui/scroll-area"
import { ModelOptionRow } from "@/components/chat/model-option-row"
import { ModelFastModeToggle } from "@/components/chat/model-picker-controls"
import { ModelBehaviorPreviewPanel } from "@/components/chat/model-behavior-preview-panel"
import { useModelOptionKeyboard } from "@/components/chat/use-model-option-keyboard"
import {
  filterModelGroups,
  flattenModelGroups,
  type ModelOptionGroup,
} from "@/lib/model-config-groups"
import type { AgentType, SessionConfigOptionInfo } from "@/lib/types"
import { useModelBehaviorPreview } from "@/components/chat/use-model-behavior-preview"

interface ModelOptionListProps {
  groups: ModelOptionGroup[]
  currentValue: string
  onSelect: (value: string) => void
  searchPlaceholder: string
  searchAriaLabel: string
  listAriaLabel: string
  emptyLabel: string
  behaviorOptions?: SessionConfigOptionInfo[]
  onBehaviorSelect?: (
    modelValue: string,
    configId: string,
    valueId: string
  ) => void
  compact?: boolean
  /** Focus the search box on mount (the wide popover opens straight into it). */
  autoFocus?: boolean
  agentType?: AgentType | null
}

const ROW_ESTIMATE_PX = 32
const MAX_LIST_HEIGHT_PX = 320

export function ModelOptionList({
  groups,
  currentValue,
  onSelect,
  searchPlaceholder,
  searchAriaLabel,
  listAriaLabel,
  emptyLabel,
  behaviorOptions = [],
  onBehaviorSelect,
  compact = false,
  autoFocus = false,
  agentType,
}: ModelOptionListProps) {
  const [query, setQuery] = useState("")
  const listRef = useRef<HTMLDivElement>(null)
  const [previewOffset, setPreviewOffset] = useState(0)
  const [activeIndex, setActiveIndex] = useState(() => {
    const options = groups.flatMap((group) => group.options)
    return Math.max(
      0,
      options.findIndex((option) => option.value === currentValue)
    )
  })
  const virtualizerRef = useRef<VirtualizerHandle>(null)
  const viewportRef = useRef<HTMLElement | null>(null)
  const [viewportEl, setViewportEl] = useState<HTMLElement | null>(null)
  const handleViewportRef = useCallback((element: HTMLElement | null) => {
    viewportRef.current = element
    setViewportEl(element)
  }, [])
  const baseId = useId()
  const listId = `${baseId}-list`
  const optionId = useCallback(
    (optionIndex: number) => `${baseId}-opt-${optionIndex}`,
    [baseId]
  )

  const rows = useMemo(
    () => flattenModelGroups(filterModelGroups(groups, query)),
    [groups, query]
  )
  const optionRowIndices = useMemo(
    () => rows.flatMap((row, index) => (row.kind === "option" ? [index] : [])),
    [rows]
  )
  const modelOptions = useMemo(
    () => rows.flatMap((row) => (row.kind === "option" ? [row.option] : [])),
    [rows]
  )
  const optionCount = optionRowIndices.length
  const {
    behaviorModelValue,
    setBehaviorModelValue,
    behaviorOptionsForModel,
    activeBehaviorOptions,
    activeModel,
  } = useModelBehaviorPreview(
    modelOptions,
    currentValue,
    behaviorOptions,
    agentType
  )
  const optionIndexByRow = useMemo(() => {
    const map = new Map<number, number>()
    optionRowIndices.forEach((rowIndex, optionIndex) =>
      map.set(rowIndex, optionIndex)
    )
    return map
  }, [optionRowIndices])

  const activeIndexClamped =
    optionCount === 0 ? 0 : Math.min(activeIndex, optionCount - 1)

  const moveActiveTo = useCallback(
    (next: number) => {
      if (optionCount === 0) return
      const clamped = Math.max(0, Math.min(optionCount - 1, next))
      setActiveIndex(clamped)
      setBehaviorModelValue(modelOptions[clamped].value)
      virtualizerRef.current?.scrollToIndex(optionRowIndices[clamped], {
        align: "nearest",
      })
    },
    [optionCount, optionRowIndices, modelOptions, setBehaviorModelValue]
  )

  const handleKeyDown = useModelOptionKeyboard({
    rows,
    optionRowIndices,
    activeIndexClamped,
    optionCount,
    currentValue,
    moveActiveTo,
    onSelect,
    behaviorOptionsForModel,
    showBehavior: setBehaviorModelValue,
  })

  const listHeight = Math.min(
    MAX_LIST_HEIGHT_PX,
    Math.max(rows.length, 1) * ROW_ESTIMATE_PX
  )

  const activeFlatIndex = optionRowIndices[activeIndexClamped]

  return (
    <div
      ref={listRef}
      className={cn(
        "relative flex min-w-0 flex-col",
        compact && "max-h-[min(60vh,24rem)] w-full overflow-y-auto"
      )}
      onMouseLeave={() => setBehaviorModelValue(null)}
    >
      <div
        className={cn(
          "flex min-w-0 flex-col overflow-hidden rounded-2xl border border-primary/10 bg-popover text-popover-foreground shadow-lg",
          compact ? "w-full" : "w-[21rem] max-w-full max-sm:w-full"
        )}
      >
        <ModelFastModeToggle
          options={behaviorOptions}
          onSelect={
            onBehaviorSelect
              ? (configId, valueId) =>
                  onBehaviorSelect(currentValue, configId, valueId)
              : undefined
          }
        />
        <div className="flex items-center gap-2 border-b border-primary/10 px-3 py-2">
          <Search className="size-4 shrink-0 text-muted-foreground" />
          <input
            type="text"
            value={query}
            autoFocus={autoFocus}
            spellCheck={false}
            autoComplete="off"
            role="combobox"
            aria-expanded
            aria-controls={listId}
            aria-activedescendant={
              optionCount > 0 ? optionId(activeIndexClamped) : undefined
            }
            aria-label={searchAriaLabel}
            placeholder={searchPlaceholder}
            onChange={(event) => {
              setQuery(event.target.value)
              setActiveIndex(0)
              setBehaviorModelValue(null)
            }}
            onKeyDown={handleKeyDown}
            className="w-full bg-transparent text-sm outline-none placeholder:text-muted-foreground"
          />
        </div>

        {optionCount === 0 ? (
          <div className="px-3 py-6 text-center text-sm text-muted-foreground">
            {emptyLabel}
          </div>
        ) : (
          <div style={{ height: listHeight }}>
            <ScrollArea onViewportRef={handleViewportRef} className="h-full">
              <div
                role="listbox"
                id={listId}
                aria-label={listAriaLabel}
                className="p-1"
              >
                {viewportEl ? (
                  <Virtualizer
                    ref={virtualizerRef}
                    scrollRef={viewportRef}
                    keepMounted={
                      activeFlatIndex != null ? [activeFlatIndex] : undefined
                    }
                  >
                    {rows.map((row, flatIndex) => {
                      if (row.kind === "header") {
                        return (
                          <div
                            key={row.key}
                            role="presentation"
                            className="truncate px-2 pt-2 pb-0.5 text-xs font-medium text-muted-foreground"
                          >
                            {row.name}
                          </div>
                        )
                      }
                      const optionIndex = optionIndexByRow.get(flatIndex) ?? 0
                      const selected = row.option.value === currentValue
                      const active = optionIndex === activeIndexClamped
                      return (
                        <ModelOptionRow
                          key={row.key}
                          option={row.option}
                          selected={selected}
                          active={active}
                          id={optionId(optionIndex)}
                          onPreview={() => {
                            setActiveIndex(optionIndex)
                            setBehaviorModelValue(row.option.value)
                            const bounds = document
                              .getElementById(optionId(optionIndex))
                              ?.getBoundingClientRect()
                            const listBounds =
                              listRef.current?.getBoundingClientRect()
                            setPreviewOffset(
                              bounds && listBounds
                                ? bounds.top - listBounds.top
                                : 0
                            )
                          }}
                          onSelect={() => {
                            const rowBehaviorOptions = behaviorOptionsForModel(
                              row.option
                            )
                            if (selected && rowBehaviorOptions.length > 0) {
                              setBehaviorModelValue(row.option.value)
                              return
                            }
                            onSelect(row.option.value)
                          }}
                        />
                      )
                    })}
                  </Virtualizer>
                ) : null}
              </div>
            </ScrollArea>
          </div>
        )}
      </div>
      {activeModel ? (
        <ModelBehaviorPreviewPanel
          key={behaviorModelValue}
          model={activeModel}
          agentType={agentType}
          offset={previewOffset}
          options={activeBehaviorOptions}
          onSelect={onBehaviorSelect}
          compact={compact}
        />
      ) : null}
    </div>
  )
}
