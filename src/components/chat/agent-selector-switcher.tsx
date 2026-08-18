"use client"

import { MoreHorizontal } from "lucide-react"
import { useCallback, useMemo, useRef, type RefObject } from "react"
import type { AcpAgentInfo, AgentType } from "@/lib/types"
import { getAgentDisplayName } from "@/lib/agent-sdk-presentation"
import { AgentIcon } from "@/components/agent-icon"
import {
  Popover,
  PopoverContent,
  PopoverTrigger,
} from "@/components/ui/popover"
import { cn } from "@/lib/utils"
import {
  useAgentSelectorLayout,
  type AgentSelectorLayout,
  type AgentSelectorRefs,
} from "./use-agent-selector-layout"

interface SwitcherProps {
  agents: AcpAgentInfo[]
  selected: AgentType | null
  disabled: boolean
  align: "start" | "center"
  onSelect: (agentType: AgentType) => void
  moreLabel: (count: number) => string
}

interface AgentButtonProps {
  agent: AcpAgentInfo
  disabled: boolean
  onSelect: (agentType: AgentType) => void
}

function AgentOption({ agent, disabled, onSelect }: AgentButtonProps) {
  const label = getAgentDisplayName(agent.agent_type)
  return (
    <button
      type="button"
      data-slot="agent-option"
      disabled={disabled || !agent.available}
      onClick={() => onSelect(agent.agent_type)}
      className={cn(
        "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-left text-sm transition-colors",
        disabled || !agent.available
          ? "cursor-not-allowed opacity-40"
          : "cursor-pointer hover:bg-accent hover:text-accent-foreground"
      )}
    >
      <AgentIcon agentType={agent.agent_type} className="h-4 w-4 shrink-0" />
      <span className="min-w-0 flex-1 truncate">{label}</span>
    </button>
  )
}

function AgentPillLabel({
  label,
  selected,
  compact,
  selectedLabelRef,
}: {
  label: string
  selected: boolean
  compact: boolean
  selectedLabelRef: RefObject<HTMLSpanElement | null>
}) {
  const visible = selected && !compact
  return (
    <span
      className={cn(
        "grid min-w-0 transition-[grid-template-columns] duration-300",
        visible ? "grid-cols-[1fr]" : "grid-cols-[0fr]"
      )}
    >
      <span
        ref={selected ? selectedLabelRef : undefined}
        className={cn(
          "min-w-0 overflow-hidden text-ellipsis whitespace-nowrap transition-opacity duration-300",
          visible ? "opacity-100" : "opacity-0"
        )}
      >
        {label}
      </span>
    </span>
  )
}

function AgentPill({
  agent,
  disabled,
  selected,
  compact,
  itemRef,
  selectedLabelRef,
  onSelect,
}: AgentButtonProps & {
  selected: boolean
  compact: boolean
  itemRef: (element: HTMLButtonElement | null) => void
  selectedLabelRef: RefObject<HTMLSpanElement | null>
}) {
  const label = getAgentDisplayName(agent.agent_type)
  return (
    <button
      type="button"
      ref={itemRef}
      data-slot="agent-pill"
      aria-pressed={selected}
      title={label}
      disabled={disabled || !agent.available}
      onClick={() => onSelect(agent.agent_type)}
      className={cn(
        "relative z-10 inline-flex items-center justify-center gap-1.5 rounded-full text-xs font-medium transition-all duration-300",
        selected ? "min-w-0 shrink px-3 py-2" : "shrink-0 px-2 py-2",
        disabled || !agent.available
          ? "cursor-not-allowed opacity-40"
          : "cursor-pointer",
        selected
          ? "text-foreground"
          : "text-muted-foreground hover:text-foreground/70"
      )}
    >
      <AgentIcon agentType={agent.agent_type} className="h-4 w-4 shrink-0" />
      <AgentPillLabel
        label={label}
        selected={selected}
        compact={compact}
        selectedLabelRef={selectedLabelRef}
      />
    </button>
  )
}

function MoreAgents({
  layout,
  moreRef,
  disabled,
  onSelect,
  label,
}: {
  layout: AgentSelectorLayout
  moreRef: AgentSelectorRefs["moreRef"]
  disabled: boolean
  onSelect: (agentType: AgentType) => void
  label: string
}) {
  return (
    <Popover open={layout.moreOpen} onOpenChange={layout.setMoreOpen}>
      <PopoverTrigger asChild>
        <button
          ref={moreRef}
          type="button"
          data-slot="agent-selector-more"
          disabled={disabled}
          title={label}
          aria-label={label}
          className={cn(
            "relative z-10 inline-flex shrink-0 cursor-pointer items-center justify-center rounded-full px-2 py-2 text-muted-foreground transition-colors hover:text-foreground/70 data-[state=open]:bg-background/70 data-[state=open]:text-foreground",
            disabled && "cursor-not-allowed opacity-40"
          )}
        >
          <MoreHorizontal className="h-4 w-4" aria-hidden />
        </button>
      </PopoverTrigger>
      <PopoverContent
        align="end"
        className="max-h-(--radix-popover-content-available-height) w-56 gap-0.5 overflow-x-hidden overflow-y-auto rounded-md p-1"
      >
        {layout.hidden.map((agent) => (
          <AgentOption
            key={agent.agent_type}
            agent={agent}
            disabled={disabled}
            onSelect={(agentType) => {
              layout.setMoreOpen(false)
              onSelect(agentType)
            }}
          />
        ))}
      </PopoverContent>
    </Popover>
  )
}

function SelectionIndicator({
  indicator,
}: {
  indicator: AgentSelectorLayout["indicator"]
}) {
  if (!indicator) return null
  return (
    <div
      className="absolute top-0.5 bottom-0.5 rounded-full bg-background shadow-sm ring-1 ring-border/50 transition-all duration-300 ease-[cubic-bezier(0.4,0,0.2,1)]"
      style={{ left: indicator.left, width: indicator.width }}
    />
  )
}

function SelectorFrame({
  layout,
  frameRef,
  moreRef,
  selectedLabelRef,
  setItemRef,
  selected,
  disabled,
  onSelect,
  moreLabel,
}: Omit<SwitcherProps, "agents" | "align"> & {
  layout: AgentSelectorLayout
  frameRef: AgentSelectorRefs["frameRef"]
  moreRef: AgentSelectorRefs["moreRef"]
  selectedLabelRef: AgentSelectorRefs["selectedLabelRef"]
  setItemRef: (
    agentType: AgentType
  ) => (element: HTMLButtonElement | null) => void
}) {
  return (
    <div
      ref={frameRef}
      className="relative inline-flex max-w-full items-center overflow-hidden rounded-full border border-border/50 bg-muted/50 p-0.5"
    >
      <SelectionIndicator indicator={layout.indicator} />
      {layout.visible.map((agent) => (
        <AgentPill
          key={agent.agent_type}
          agent={agent}
          disabled={disabled}
          selected={selected === agent.agent_type}
          compact={layout.compactSelected}
          itemRef={setItemRef(agent.agent_type)}
          selectedLabelRef={selectedLabelRef}
          onSelect={onSelect}
        />
      ))}
      {layout.hidden.length > 0 ? (
        <MoreAgents
          layout={layout}
          moreRef={moreRef}
          disabled={disabled}
          onSelect={onSelect}
          label={moreLabel(layout.hidden.length)}
        />
      ) : null}
    </div>
  )
}

export function AgentSelectorSwitcher(props: SwitcherProps) {
  const wrapperRef = useRef<HTMLDivElement>(null)
  const frameRef = useRef<HTMLDivElement>(null)
  const itemRefs = useRef(new Map<AgentType, HTMLButtonElement>())
  const moreRef = useRef<HTMLButtonElement>(null)
  const selectedLabelRef = useRef<HTMLSpanElement>(null)
  const unitWidthRef = useRef(0)
  const setItemRef = useCallback(
    (agentType: AgentType) => (element: HTMLButtonElement | null) => {
      if (element) itemRefs.current.set(agentType, element)
      else itemRefs.current.delete(agentType)
    },
    []
  )
  const refs = useMemo<AgentSelectorRefs>(
    () => ({
      wrapperRef,
      frameRef,
      itemRefs,
      moreRef,
      selectedLabelRef,
      unitWidthRef,
    }),
    [frameRef, itemRefs, moreRef, selectedLabelRef, unitWidthRef, wrapperRef]
  )
  const layout = useAgentSelectorLayout(props.agents, props.selected, refs)
  return (
    <div
      ref={wrapperRef}
      data-slot="agent-selector"
      className={cn(
        "@container flex min-w-0 flex-1 items-center",
        props.align === "center" && "justify-center"
      )}
    >
      <SelectorFrame
        {...props}
        layout={layout}
        frameRef={frameRef}
        moreRef={moreRef}
        selectedLabelRef={selectedLabelRef}
        setItemRef={setItemRef}
      />
    </div>
  )
}
