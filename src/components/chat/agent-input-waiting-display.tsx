"use client"

import { useEffect, useRef, useState } from "react"
import { Reorder } from "motion/react"

import {
  AgentInputWaitingRow,
  isAgentInputLocked,
} from "@/components/chat/agent-input-waiting-row"
import type { AgentInputItem } from "@/lib/types"

interface AgentInputWaitingDisplayProps {
  items: AgentInputItem[]
  onDelete?: (id: string) => void
  onRetry?: (id: string) => void
  onReorder?: (orderedIds: string[]) => Promise<void>
  onForceThrough?: (messageId: string, expectedPrefixIds: string[]) => void
}

function sameOrder(current: AgentInputItem[], next: AgentInputItem[]): boolean {
  return (
    current.length === next.length &&
    current.every((item, index) => item.id === next[index]?.id)
  )
}

type WaitingSection =
  | { kind: "locked"; item: AgentInputItem }
  | { kind: "movable"; items: AgentInputItem[] }

function splitAtLockedItems(items: AgentInputItem[]): WaitingSection[] {
  const sections: WaitingSection[] = []
  let movable: AgentInputItem[] = []
  const flushMovable = () => {
    if (movable.length > 0) {
      sections.push({ kind: "movable", items: movable })
      movable = []
    }
  }
  for (const item of items) {
    if (isAgentInputLocked(item)) {
      flushMovable()
      sections.push({ kind: "locked", item })
    } else {
      movable.push(item)
    }
  }
  flushMovable()
  return sections
}

interface MovableSectionProps {
  items: AgentInputItem[]
  visible: AgentInputItem[]
  onDelete?: (id: string) => void
  onRetry?: (id: string) => void
  onForceThrough?: (messageId: string, expectedPrefixIds: string[]) => void
  onCommit?: (items: AgentInputItem[]) => Promise<void>
  forceDisabled: boolean
}

function useMovableOrder(
  items: AgentInputItem[],
  onCommit?: (items: AgentInputItem[]) => Promise<void>
) {
  const [orderedItems, setOrderedItems] = useState(items)
  const [submitting, setSubmitting] = useState(false)
  const orderedItemsRef = useRef(items)
  const authoritativeOrder = items.map((item) => item.id).join(":")
  const orderChanged = !sameOrder(items, orderedItems)

  useEffect(() => {
    orderedItemsRef.current = items
    setOrderedItems(items)
  }, [authoritativeOrder, items])

  const previewOrder = (next: AgentInputItem[]) => {
    orderedItemsRef.current = next
    setOrderedItems(next)
  }

  const commitOrder = async () => {
    const next = orderedItemsRef.current
    if (!onCommit || sameOrder(items, next)) return
    setSubmitting(true)
    try {
      await onCommit(next)
    } catch {
      previewOrder(items)
    } finally {
      setSubmitting(false)
    }
  }
  return { orderedItems, previewOrder, commitOrder, submitting, orderChanged }
}

function MovableSection({ items, onCommit, ...rowProps }: MovableSectionProps) {
  const order = useMovableOrder(items, onCommit)

  return (
    <Reorder.Group
      as="div"
      axis="y"
      values={order.orderedItems}
      onReorder={order.previewOrder}
      className="space-y-1"
    >
      {order.orderedItems.map((item) => (
        <AgentInputWaitingRow
          key={item.id}
          {...rowProps}
          item={item}
          index={rowProps.visible.findIndex(
            (candidate) => candidate.id === item.id
          )}
          onReorderFinished={order.commitOrder}
          reorderDisabled={!onCommit || order.submitting || items.length < 2}
          forceDisabled={
            rowProps.forceDisabled || order.submitting || order.orderChanged
          }
        />
      ))}
    </Reorder.Group>
  )
}

interface WaitingSectionsProps extends AgentInputWaitingDisplayProps {
  visible: AgentInputItem[]
  sections: WaitingSection[]
  forceDisabled: boolean
}

function WaitingSections(props: WaitingSectionsProps) {
  const { visible, sections, onReorder, forceDisabled } = props
  return sections.map((section, sectionIndex) => {
    if (section.kind === "locked") {
      return (
        <AgentInputWaitingRow
          key={section.item.id}
          item={section.item}
          index={visible.findIndex((item) => item.id === section.item.id)}
          visible={visible}
          onDelete={props.onDelete}
          onRetry={props.onRetry}
          onForceThrough={props.onForceThrough}
          reorderDisabled
          forceDisabled={forceDisabled}
        />
      )
    }
    const onCommit = onReorder
      ? (next: AgentInputItem[]) =>
          submitSegmentOrder(sections, sectionIndex, next, onReorder)
      : undefined
    return (
      <MovableSection
        key={section.items.map((item) => item.id).join(":")}
        items={section.items}
        visible={visible}
        onDelete={props.onDelete}
        onRetry={props.onRetry}
        onForceThrough={props.onForceThrough}
        onCommit={onCommit}
        forceDisabled={forceDisabled}
      />
    )
  })
}

async function submitSegmentOrder(
  sections: WaitingSection[],
  sectionIndex: number,
  next: AgentInputItem[],
  onReorder: (orderedIds: string[]) => Promise<void>
) {
  const current = sections[sectionIndex]
  if (current?.kind !== "movable" || sameOrder(current.items, next)) return
  const orderedIds = sections.flatMap((section, index) => {
    if (section.kind === "locked") return []
    return (index === sectionIndex ? next : section.items).map(
      (item) => item.id
    )
  })
  await onReorder(orderedIds)
}

export function AgentInputWaitingDisplay({
  items,
  onDelete,
  onRetry,
  onReorder,
  onForceThrough,
}: AgentInputWaitingDisplayProps) {
  const visible = items.filter((item) =>
    ["waiting", "dispatching", "fallback_queued", "failed"].includes(
      item.status
    )
  )
  if (visible.length === 0) return null
  const sections = splitAtLockedItems(visible)
  const forceDisabled = visible.some(
    (item) => item.force_batch_id != null || item.force_requested_at != null
  )

  return (
    <div className="max-h-40 space-y-1 overflow-y-auto pb-1">
      <WaitingSections
        visible={visible}
        sections={sections}
        onDelete={onDelete}
        onRetry={onRetry}
        onReorder={onReorder}
        onForceThrough={onForceThrough}
        forceDisabled={forceDisabled}
        items={items}
      />
    </div>
  )
}
