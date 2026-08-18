export type SplitOrientation = "horizontal" | "vertical"

export interface GroupLeaf {
  type: "group"
  id: string
}

export interface SplitNode {
  type: "split"
  id: string
  orientation: SplitOrientation
  children: LayoutNode[]
  ratios: number[]
}

export type LayoutNode = GroupLeaf | SplitNode
export type SplitDirection = "right" | "down"

export const ROOT_GROUP_ID = "g-main"
export const MIN_SPLIT_RATIO = 0.15

export function makeGroupId(): string {
  return `g-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
}

export function singleGroupLayout(id = ROOT_GROUP_ID): GroupLeaf {
  return { type: "group", id }
}

export function leafIds(tree: LayoutNode): string[] {
  if (tree.type === "group") return [tree.id]
  return tree.children.flatMap(leafIds)
}

export function firstLeafId(tree: LayoutNode): string {
  return tree.type === "group" ? tree.id : firstLeafId(tree.children[0])
}

function normalizeRatios(ratios: number[], count: number): number[] {
  if (count <= 0) return []
  const positive = Array.from({ length: count }, (_, index) => {
    const ratio = ratios[index]
    return Number.isFinite(ratio) && ratio > 0 ? ratio : 0
  })
  const total = positive.reduce((sum, ratio) => sum + ratio, 0)
  if (total <= 0) return positive.map(() => 1 / count)
  return positive.map((ratio) => ratio / total)
}

function normalizeSplit(node: SplitNode): LayoutNode {
  const sourceRatios = normalizeRatios(node.ratios, node.children.length)
  const children: LayoutNode[] = []
  const ratios: number[] = []

  node.children.forEach((rawChild, index) => {
    const child = normalizeLayout(rawChild)
    const share = sourceRatios[index]
    if (child.type === "split" && child.orientation === node.orientation) {
      child.children.forEach((grandchild, childIndex) => {
        children.push(grandchild)
        ratios.push(share * child.ratios[childIndex])
      })
      return
    }
    children.push(child)
    ratios.push(share)
  })

  if (children.length === 0) return singleGroupLayout()
  if (children.length === 1) return children[0]
  return { ...node, children, ratios: normalizeRatios(ratios, children.length) }
}

/** Canonicalize ratios, single-child nodes and same-axis nesting. */
export function normalizeLayout(tree: LayoutNode): LayoutNode {
  return tree.type === "group" ? tree : normalizeSplit(tree)
}

function replaceGroup(
  node: LayoutNode,
  groupId: string,
  replacement: LayoutNode
): LayoutNode {
  if (node.type === "group") return node.id === groupId ? replacement : node
  let changed = false
  const children = node.children.map((child) => {
    const next = replaceGroup(child, groupId, replacement)
    changed ||= next !== child
    return next
  })
  return changed ? normalizeLayout({ ...node, children }) : node
}

export function splitGroup(
  tree: LayoutNode,
  groupId: string,
  options: { direction: SplitDirection; newGroupId: string }
): LayoutNode {
  const { direction, newGroupId } = options
  const ids = leafIds(tree)
  if (!ids.includes(groupId) || ids.includes(newGroupId)) return tree
  const orientation = direction === "right" ? "horizontal" : "vertical"
  const replacement: SplitNode = {
    type: "split",
    id: `s-${newGroupId}`,
    orientation,
    children: [singleGroupLayout(groupId), singleGroupLayout(newGroupId)],
    ratios: [0.5, 0.5],
  }
  return replaceGroup(tree, groupId, replacement)
}

function pruneGroup(node: LayoutNode, groupId: string): LayoutNode | null {
  if (node.type === "group") return node.id === groupId ? null : node
  const children: LayoutNode[] = []
  const ratios: number[] = []
  node.children.forEach((child, index) => {
    const next = pruneGroup(child, groupId)
    if (!next) return
    children.push(next)
    ratios.push(node.ratios[index] ?? 0)
  })
  if (children.length === 0) return null
  if (children.length === 1) return children[0]
  return normalizeLayout({ ...node, children, ratios })
}

export function removeGroup(tree: LayoutNode, groupId: string): LayoutNode {
  const ids = leafIds(tree)
  if (ids.length < 2 || !ids.includes(groupId)) return tree
  return pruneGroup(tree, groupId) ?? tree
}

export function neighborGroupId(
  tree: LayoutNode,
  groupId: string
): string | null {
  const ids = leafIds(tree)
  const index = ids.indexOf(groupId)
  if (index < 0 || ids.length < 2) return null
  return index > 0 ? ids[index - 1] : ids[index + 1]
}

function pruneDeadGroups(
  node: LayoutNode,
  liveGroupIds: ReadonlySet<string>
): LayoutNode | null {
  if (node.type === "group") return liveGroupIds.has(node.id) ? node : null
  const children: LayoutNode[] = []
  const ratios: number[] = []
  node.children.forEach((child, index) => {
    const next = pruneDeadGroups(child, liveGroupIds)
    if (!next) return
    children.push(next)
    ratios.push(node.ratios[index] ?? 0)
  })
  if (children.length === 0) return null
  if (children.length === 1) return children[0]
  return normalizeLayout({ ...node, children, ratios })
}

export function normalizeTree(
  tree: LayoutNode,
  liveGroupIds: ReadonlySet<string>
): LayoutNode {
  if (leafIds(tree).every((groupId) => liveGroupIds.has(groupId))) return tree
  return (
    pruneDeadGroups(tree, liveGroupIds) ?? singleGroupLayout(firstLeafId(tree))
  )
}

function parentSplitId(tree: LayoutNode, groupId: string): string | null {
  if (tree.type === "group") return null
  if (
    tree.children.some(
      (child) => child.type === "group" && child.id === groupId
    )
  ) {
    return tree.id
  }
  for (const child of tree.children) {
    const found = parentSplitId(child, groupId)
    if (found) return found
  }
  return null
}

function toggleSplit(node: LayoutNode, splitId: string): LayoutNode {
  if (node.type === "group") return node
  if (node.id === splitId) {
    return {
      ...node,
      orientation:
        node.orientation === "horizontal" ? "vertical" : "horizontal",
    }
  }
  const children = node.children.map((child) => toggleSplit(child, splitId))
  const changed = children.some(
    (child, index) => child !== node.children[index]
  )
  return changed ? { ...node, children } : node
}

export function toggleOrientation(
  tree: LayoutNode,
  groupId: string
): LayoutNode {
  const splitId = parentSplitId(tree, groupId)
  return splitId ? normalizeLayout(toggleSplit(tree, splitId)) : tree
}

function resizeNode(
  node: LayoutNode,
  splitId: string,
  resize: { handleIndex: number; boundaryFraction: number }
): LayoutNode {
  const { handleIndex, boundaryFraction } = resize
  if (node.type === "group") return node
  if (node.id === splitId) {
    if (handleIndex < 0 || handleIndex >= node.children.length - 1) return node
    const ratios = normalizeRatios(node.ratios, node.children.length)
    const prefix = ratios.slice(0, handleIndex).reduce((a, b) => a + b, 0)
    const pairTotal = ratios[handleIndex] + ratios[handleIndex + 1]
    const minimum = Math.min(MIN_SPLIT_RATIO, pairTotal / 2)
    const left = Math.min(
      Math.max(boundaryFraction - prefix, minimum),
      pairTotal - minimum
    )
    if (Math.abs(left - ratios[handleIndex]) < 0.0001) return node
    ratios[handleIndex] = left
    ratios[handleIndex + 1] = pairTotal - left
    return { ...node, ratios }
  }
  const children = node.children.map((child) =>
    resizeNode(child, splitId, resize)
  )
  const changed = children.some(
    (child, index) => child !== node.children[index]
  )
  return changed ? { ...node, children } : node
}

export function resizeSplitAt(
  tree: LayoutNode,
  splitId: string,
  resize: { handleIndex: number; boundaryFraction: number }
): LayoutNode {
  const { boundaryFraction } = resize
  if (!Number.isFinite(boundaryFraction)) return tree
  return resizeNode(tree, splitId, resize)
}

export { computeRects, isLayoutNode } from "./tab-group-geometry"
export type { GroupRect, HandleRect } from "./tab-group-geometry"
