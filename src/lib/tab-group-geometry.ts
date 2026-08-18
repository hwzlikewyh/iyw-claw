import type { LayoutNode, SplitOrientation } from "./tab-group-layout"

export interface GroupRect {
  x: number
  y: number
  w: number
  h: number
}

export interface HandleRect {
  splitId: string
  index: number
  orientation: SplitOrientation
  x: number
  y: number
  length: number
  nodeStart: number
  nodeExtent: number
}

function safeRatios(node: Extract<LayoutNode, { type: "split" }>): number[] {
  const ratios = node.children.map((_, index) => {
    const ratio = node.ratios[index]
    return Number.isFinite(ratio) && ratio > 0 ? ratio : 0
  })
  const total = ratios.reduce((sum, ratio) => sum + ratio, 0)
  if (total <= 0) return ratios.map(() => 1 / node.children.length)
  return ratios.map((ratio) => ratio / total)
}

function appendRects(
  node: LayoutNode,
  rect: GroupRect,
  result: { groups: Map<string, GroupRect>; handles: HandleRect[] }
): void {
  const { groups, handles } = result
  if (node.type === "group") {
    groups.set(node.id, rect)
    return
  }
  const horizontal = node.orientation === "horizontal"
  let cursor = horizontal ? rect.x : rect.y
  const ratios = safeRatios(node)

  node.children.forEach((child, index) => {
    const extent = (horizontal ? rect.w : rect.h) * ratios[index]
    if (index > 0) {
      handles.push({
        splitId: node.id,
        index: index - 1,
        orientation: node.orientation,
        x: horizontal ? cursor : rect.x,
        y: horizontal ? rect.y : cursor,
        length: horizontal ? rect.h : rect.w,
        nodeStart: horizontal ? rect.x : rect.y,
        nodeExtent: horizontal ? rect.w : rect.h,
      })
    }
    const childRect = horizontal
      ? { x: cursor, y: rect.y, w: extent, h: rect.h }
      : { x: rect.x, y: cursor, w: rect.w, h: extent }
    appendRects(child, childRect, result)
    cursor += extent
  })
}

export function computeRects(tree: LayoutNode): {
  groups: Map<string, GroupRect>
  handles: HandleRect[]
} {
  const groups = new Map<string, GroupRect>()
  const handles: HandleRect[] = []
  appendRects(tree, { x: 0, y: 0, w: 100, h: 100 }, { groups, handles })
  return { groups, handles }
}

export function isLayoutNode(value: unknown): value is LayoutNode {
  if (typeof value !== "object" || value === null) return false
  const node = value as Record<string, unknown>
  if (node.type === "group")
    return typeof node.id === "string" && node.id.length > 0
  if (node.type !== "split" || typeof node.id !== "string") return false
  if (node.orientation !== "horizontal" && node.orientation !== "vertical") {
    return false
  }
  if (!Array.isArray(node.children) || !Array.isArray(node.ratios)) return false
  if (node.children.length < 2 || node.children.length !== node.ratios.length) {
    return false
  }
  if (
    !node.ratios.every(
      (ratio) =>
        typeof ratio === "number" && Number.isFinite(ratio) && ratio > 0
    )
  ) {
    return false
  }
  return node.children.every(isLayoutNode)
}
