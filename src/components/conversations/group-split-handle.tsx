"use client"

import { useCallback, type RefObject } from "react"
import { cn } from "@/lib/utils"
import type { HandleRect } from "@/lib/tab-group-layout"
import {
  boundaryFractionForHandle,
  splitHandleStyle,
  useSplitKeyboardResize,
  useSplitPointerResize,
} from "./group-split-handle-resize"

interface GroupSplitHandleProps {
  handle: HandleRect
  containerRef: RefObject<HTMLDivElement | null>
  onResize: (splitId: string, index: number, boundaryFraction: number) => void
}

function SplitHandleLine({
  vertical,
  dragging,
}: {
  vertical: boolean
  dragging: boolean
}) {
  const size = vertical
    ? dragging
      ? "h-full w-[3px]"
      : "h-full w-px group-hover/split-handle:w-[3px]"
    : dragging
      ? "h-[3px] w-full"
      : "h-px w-full group-hover/split-handle:h-[3px]"
  return (
    <div
      className={cn(
        "bg-border transition-[width,height,background-color] duration-150",
        size,
        dragging
          ? "bg-foreground/60"
          : "group-hover/split-handle:bg-foreground/40"
      )}
    />
  )
}

function splitHandleClass(vertical: boolean): string {
  return cn(
    "group/split-handle absolute z-30 flex touch-none select-none items-center justify-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
    vertical ? "cursor-col-resize" : "cursor-row-resize"
  )
}

export function GroupSplitHandle({
  handle,
  containerRef,
  onResize,
}: GroupSplitHandleProps) {
  const verticalLine = handle.orientation === "horizontal"
  const boundaryFraction = boundaryFractionForHandle(handle)
  const commitResize = useCallback(
    (fraction: number) => {
      if (!Number.isFinite(fraction)) return
      onResize(handle.splitId, handle.index, fraction)
    },
    [handle.index, handle.splitId, onResize]
  )
  const {
    dragging,
    onPointerDown,
    onPointerMove,
    endDrag,
    onLostPointerCapture,
  } = useSplitPointerResize(handle, containerRef, commitResize)
  const handleKeyDown = useSplitKeyboardResize(
    handle,
    boundaryFraction,
    commitResize
  )

  return (
    <div
      role="separator"
      tabIndex={0}
      aria-label="调整分屏大小"
      aria-orientation={verticalLine ? "vertical" : "horizontal"}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(boundaryFraction * 100)}
      onKeyDown={handleKeyDown}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={endDrag}
      onPointerCancel={endDrag}
      onLostPointerCapture={onLostPointerCapture}
      className={splitHandleClass(verticalLine)}
      style={splitHandleStyle(handle)}
    >
      <SplitHandleLine vertical={verticalLine} dragging={dragging} />
    </div>
  )
}
