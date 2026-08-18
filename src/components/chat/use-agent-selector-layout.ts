"use client"

import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useState,
} from "react"
import type { RefObject } from "react"
import type { AcpAgentInfo, AgentType } from "@/lib/types"

const SELECTED_PADDING_EXTRA = 8
const SELECTED_COMPACT_WIDTH = 128
const useIsomorphicLayoutEffect =
  typeof window !== "undefined" ? useLayoutEffect : useEffect

interface Indicator {
  left: number
  width: number
}

export interface AgentSelectorRefs {
  wrapperRef: RefObject<HTMLDivElement | null>
  frameRef: RefObject<HTMLDivElement | null>
  itemRefs: RefObject<Map<AgentType, HTMLButtonElement>>
  moreRef: RefObject<HTMLButtonElement | null>
  selectedLabelRef: RefObject<HTMLSpanElement | null>
  unitWidthRef: RefObject<number>
}

interface LayoutInput extends AgentSelectorRefs {
  agentCount: number
  selected: AgentType | null
}

interface LayoutMeasurement {
  visibleOtherCount: number | null
  indicator: Indicator | null
  compactSelected: boolean
}

export interface AgentSelectorLayout {
  visible: AcpAgentInfo[]
  hidden: AcpAgentInfo[]
  indicator: Indicator | null
  compactSelected: boolean
  moreOpen: boolean
  setMoreOpen: (open: boolean) => void
}

function px(value: string): number {
  const number = Number.parseFloat(value)
  return Number.isFinite(number) ? number : 0
}

function partitionAgents(
  agents: AcpAgentInfo[],
  selected: AgentType | null,
  visibleOtherCount: number | null
) {
  if (visibleOtherCount === null) return { visible: agents, hidden: [] }
  const visible: AcpAgentInfo[] = []
  const hidden: AcpAgentInfo[] = []
  let taken = 0
  for (const agent of agents) {
    if (agent.agent_type === selected || taken < visibleOtherCount) {
      visible.push(agent)
      if (agent.agent_type !== selected) taken += 1
    } else hidden.push(agent)
  }
  return { visible, hidden }
}

function readUnitWidth(input: LayoutInput): number {
  let measured = 0
  let samples = 0
  for (const [agentType, element] of input.itemRefs.current) {
    if (agentType === input.selected) continue
    const width = element.getBoundingClientRect().width
    if (width <= 0) continue
    measured = measured > 0 ? Math.min(measured, width) : width
    samples += 1
  }
  const cached = input.unitWidthRef.current
  if (measured > 0 && (samples > 1 || cached === 0 || measured <= cached)) {
    input.unitWidthRef.current = measured
    return measured
  }
  return cached
}

function readLayout(input: LayoutInput): LayoutMeasurement | null {
  const wrapper = input.wrapperRef.current
  const frame = input.frameRef.current
  if (!wrapper || !frame) return null
  const outer = wrapper.getBoundingClientRect().width
  const unit = readUnitWidth(input)
  const style = window.getComputedStyle(frame)
  const inner =
    outer -
    px(style.paddingLeft) -
    px(style.paddingRight) -
    px(style.borderLeftWidth) -
    px(style.borderRightWidth)
  const labelWidth = input.selectedLabelRef.current?.scrollWidth ?? 0
  const selectedWidth = input.selected
    ? unit + SELECTED_PADDING_EXTRA + labelWidth
    : 0
  const others = input.agentCount - (input.selected ? 1 : 0)
  let visibleOtherCount: number | null = null
  if (outer > 0 && unit > 0 && selectedWidth + others * unit > inner) {
    const moreWidth =
      input.moreRef.current?.getBoundingClientRect().width || unit
    const fits = Math.floor((inner - selectedWidth - moreWidth) / unit)
    visibleOtherCount = Math.max(0, Math.min(others, fits))
  }
  const selectedButton = input.selected
    ? input.itemRefs.current.get(input.selected)
    : null
  const frameRect = frame.getBoundingClientRect()
  const buttonRect = selectedButton?.getBoundingClientRect()
  return {
    visibleOtherCount,
    compactSelected: outer > 0 && outer < SELECTED_COMPACT_WIDTH,
    indicator: buttonRect
      ? { left: buttonRect.left - frameRect.left, width: buttonRect.width }
      : null,
  }
}

interface ResizeMeasurementInput {
  wrapperRef: AgentSelectorRefs["wrapperRef"]
  itemRefs: AgentSelectorRefs["itemRefs"]
  moreRef: AgentSelectorRefs["moreRef"]
  measure: () => void
  visible: AcpAgentInfo[]
}

function useResizeMeasurement(input: ResizeMeasurementInput) {
  const { wrapperRef, itemRefs, moreRef, measure, visible } = input
  useEffect(() => {
    const wrapper = wrapperRef.current
    if (!wrapper || typeof ResizeObserver === "undefined") return
    const observer = new ResizeObserver(measure)
    for (const button of itemRefs.current.values()) observer.observe(button)
    if (moreRef.current) observer.observe(moreRef.current)
    observer.observe(wrapper)
    window.addEventListener("resize", measure)
    return () => {
      observer.disconnect()
      window.removeEventListener("resize", measure)
    }
  }, [itemRefs, measure, moreRef, visible, wrapperRef])
}

export function useAgentSelectorLayout(
  agents: AcpAgentInfo[],
  selected: AgentType | null,
  refs: AgentSelectorRefs
): AgentSelectorLayout {
  const [visibleOtherCount, setVisibleOtherCount] = useState<number | null>(
    null
  )
  const [indicator, setIndicator] = useState<Indicator | null>(null)
  const [compactSelected, setCompactSelected] = useState(false)
  const [moreOpen, setMoreOpen] = useState(false)
  const { visible, hidden } = useMemo(
    () => partitionAgents(agents, selected, visibleOtherCount),
    [agents, selected, visibleOtherCount]
  )
  const measure = useCallback(() => {
    const result = readLayout({ ...refs, agentCount: agents.length, selected })
    if (!result) return
    setVisibleOtherCount(result.visibleOtherCount)
    setCompactSelected(result.compactSelected)
    setIndicator((current) =>
      current?.left === result.indicator?.left &&
      current?.width === result.indicator?.width
        ? current
        : result.indicator
    )
    if (result.visibleOtherCount === null) setMoreOpen(false)
  }, [agents.length, refs, selected])
  useIsomorphicLayoutEffect(measure, [measure, visible])
  useResizeMeasurement({
    wrapperRef: refs.wrapperRef,
    itemRefs: refs.itemRefs,
    moreRef: refs.moreRef,
    measure,
    visible,
  })
  return {
    visible,
    hidden,
    indicator,
    compactSelected,
    moreOpen,
    setMoreOpen,
  }
}
