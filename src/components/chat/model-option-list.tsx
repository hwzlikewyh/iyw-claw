"use client"

import { useCallback, useId, useMemo, useRef, useState } from "react"
import { Check, Search } from "lucide-react"
import { Virtualizer, type VirtualizerHandle } from "virtua"
import { cn } from "@/lib/utils"
import { ScrollArea } from "@/components/ui/scroll-area"
import { DropdownRadioItemContent } from "@/components/chat/dropdown-radio-item-content"
import { ModelIcon } from "@/components/chat/model-icon"
import { ModelBehaviorMenu } from "@/components/chat/model-behavior-menu"
import {
  filterModelGroups,
  flattenModelGroups,
  type ModelOptionGroup,
} from "@/lib/model-config-groups"
import type { SessionConfigOptionInfo } from "@/lib/types"

interface ModelOptionListProps {
  groups: ModelOptionGroup[]
  currentValue: string
  onSelect: (value: string) => void
  searchPlaceholder: string
  searchAriaLabel: string
  listAriaLabel: string
  emptyLabel: string
  behaviorOptions?: SessionConfigOptionInfo[]
  onBehaviorSelect?: (configId: string, valueId: string) => void
  compact?: boolean
  /** Focus the search box on mount (the wide popover opens straight into it). */
  autoFocus?: boolean
}

const ROW_ESTIMATE_PX = 44
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
}: ModelOptionListProps) {
  const [query, setQuery] = useState("")
  const [activeIndex, setActiveIndex] = useState(0)
  const [behaviorVisible, setBehaviorVisible] = useState(false)
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
  const optionCount = optionRowIndices.length
  const hasBehaviorMenu =
    behaviorOptions.length > 0 && onBehaviorSelect !== undefined
  const behaviorSummary = useMemo(
    () =>
      behaviorOptions
        .map((option) => {
          const current = option.kind.options.find(
            ({ value }) => value === option.kind.current_value
          )
          return `${option.name}：${current?.name ?? option.kind.current_value}`
        })
        .join(" · "),
    [behaviorOptions]
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
      virtualizerRef.current?.scrollToIndex(optionRowIndices[clamped], {
        align: "nearest",
      })
    },
    [optionCount, optionRowIndices]
  )

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (event.nativeEvent.isComposing || event.key === "Process") return
      switch (event.key) {
        case "ArrowDown":
          event.preventDefault()
          moveActiveTo(activeIndexClamped + 1)
          break
        case "ArrowUp":
          event.preventDefault()
          moveActiveTo(activeIndexClamped - 1)
          break
        case "Home":
          event.preventDefault()
          moveActiveTo(0)
          break
        case "End":
          event.preventDefault()
          moveActiveTo(optionCount - 1)
          break
        case "Enter": {
          const rowIndex = optionRowIndices[activeIndexClamped]
          const row = rowIndex != null ? rows[rowIndex] : undefined
          if (row && row.kind === "option") {
            event.preventDefault()
            if (row.option.value === currentValue && hasBehaviorMenu) {
              setBehaviorVisible(true)
            } else {
              onSelect(row.option.value)
            }
          }
          break
        }
        default:
          break
      }
    },
    [
      activeIndexClamped,
      currentValue,
      hasBehaviorMenu,
      moveActiveTo,
      onSelect,
      optionCount,
      optionRowIndices,
      rows,
    ]
  )

  const listHeight = Math.min(
    MAX_LIST_HEIGHT_PX,
    Math.max(rows.length, 1) * ROW_ESTIMATE_PX
  )

  const activeFlatIndex = optionRowIndices[activeIndexClamped]

  return (
    <div
      className={cn("flex min-w-0 items-start", compact && "w-full flex-col")}
      onMouseLeave={() => setBehaviorVisible(false)}
    >
      <div
        className={cn(
          "flex min-w-0 flex-col",
          compact ? "w-full" : "w-[22rem] shrink-0"
        )}
      >
        <div className="flex items-center gap-2 border-b px-2.5 py-2">
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
              setBehaviorVisible(false)
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
                        <button
                          key={row.key}
                          type="button"
                          role="option"
                          id={optionId(optionIndex)}
                          aria-selected={selected}
                          title={row.option.name}
                          onMouseMove={() => {
                            setActiveIndex(optionIndex)
                            setBehaviorVisible(selected)
                          }}
                          onClick={() => {
                            if (selected && hasBehaviorMenu) {
                              setBehaviorVisible(true)
                              return
                            }
                            onSelect(row.option.value)
                          }}
                          className={cn(
                            "flex w-full items-start gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
                            active && "bg-accent text-accent-foreground",
                            selected && !active && "bg-accent/60"
                          )}
                        >
                          <span className="flex size-4 shrink-0 items-center justify-center pt-0.5">
                            {row.option.iconUrl ? (
                              <ModelIcon src={row.option.iconUrl} />
                            ) : selected ? (
                              <Check className="size-4" />
                            ) : null}
                          </span>
                          <DropdownRadioItemContent
                            label={row.option.name}
                            description={
                              selected && behaviorSummary
                                ? behaviorSummary
                                : row.option.description
                            }
                          />
                        </button>
                      )
                    })}
                  </Virtualizer>
                ) : null}
              </div>
            </ScrollArea>
          </div>
        )}
      </div>
      {behaviorVisible && onBehaviorSelect ? (
        <ModelBehaviorMenu
          options={behaviorOptions}
          onSelect={onBehaviorSelect}
          compact={compact}
        />
      ) : null}
    </div>
  )
}
