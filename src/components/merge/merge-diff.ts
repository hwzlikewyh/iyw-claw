import { computeLineDiff, type DiffHunk } from "./line-diff"
export { computeLineDiff, type DiffHunk } from "./line-diff"

/**
 * Line-level diff engine for three-way merge.
 *
 * Computes diffs between base↔ours and base↔theirs, then aligns
 * them into MergeHunks classified as left-only, right-only, or conflict.
 */

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

export type HunkStatus = "pending" | "applied" | "ignored"

export interface MergeHunk {
  id: string
  /** Start index in base lines, 0-based */
  baseStart: number
  /** Number of base lines covered */
  baseCount: number
  /** Diff hunk from ours (left) side, null if unchanged */
  leftHunk: DiffHunk | null
  /** Diff hunk from theirs (right) side, null if unchanged */
  rightHunk: DiffHunk | null
  type: "left-only" | "right-only" | "conflict"
}

// ---------------------------------------------------------------------------
// Three-way merge hunk computation
// ---------------------------------------------------------------------------

interface RangedHunk {
  baseStart: number
  baseEnd: number // exclusive
  hunk: DiffHunk
  side: "left" | "right"
}

/**
 * Given diff hunks from base→ours and base→theirs, produce
 * a list of MergeHunks sorted by base position.
 */
export function computeMergeHunks(
  base: string,
  ours: string,
  theirs: string
): MergeHunk[] {
  const baseLines = base.split("\n")
  const oursLines = ours.split("\n")
  const theirsLines = theirs.split("\n")

  const leftDiffs = computeLineDiff(baseLines, oursLines)
  const rightDiffs = computeLineDiff(baseLines, theirsLines)

  // Convert to ranged hunks for overlap detection
  const ranged: RangedHunk[] = []

  for (const h of leftDiffs) {
    ranged.push({
      baseStart: h.baseStart,
      baseEnd: h.baseStart + Math.max(h.baseCount, 1), // at least 1 for insertions
      hunk: h,
      side: "left",
    })
  }
  for (const h of rightDiffs) {
    ranged.push({
      baseStart: h.baseStart,
      baseEnd: h.baseStart + Math.max(h.baseCount, 1),
      hunk: h,
      side: "right",
    })
  }

  // Sort by baseStart, then by side (left first)
  ranged.sort(
    (a, b) => a.baseStart - b.baseStart || (a.side === "left" ? -1 : 1)
  )

  // Merge overlapping hunks from different sides into conflicts
  const mergeHunks: MergeHunk[] = []
  const used = new Set<number>()

  for (let i = 0; i < ranged.length; i++) {
    if (used.has(i)) continue

    const r = ranged[i]

    // Check for overlapping hunk from the other side
    let paired: RangedHunk | null = null
    let pairedIdx = -1

    for (let j = i + 1; j < ranged.length; j++) {
      if (used.has(j)) continue
      const s = ranged[j]
      if (s.side === r.side) continue
      // Check overlap: ranges [r.baseStart, r.baseEnd) and [s.baseStart, s.baseEnd)
      if (s.baseStart < r.baseEnd && r.baseStart < s.baseEnd) {
        paired = s
        pairedIdx = j
        break
      }
      // If s starts beyond r, no more overlaps possible
      if (s.baseStart >= r.baseEnd) break
    }

    if (paired && pairedIdx >= 0) {
      used.add(pairedIdx)

      // Check if both sides made identical changes — treat as non-conflict
      const leftH = r.side === "left" ? r.hunk : paired.hunk
      const rightH = r.side === "right" ? r.hunk : paired.hunk

      const identical =
        leftH.baseStart === rightH.baseStart &&
        leftH.baseCount === rightH.baseCount &&
        leftH.newLines.length === rightH.newLines.length &&
        leftH.newLines.every((line, k) => line === rightH.newLines[k])

      if (identical) {
        // Both sides made the same change — treat as left-only (auto-applicable)
        const bStart = Math.min(r.baseStart, paired.baseStart)
        const bEnd = Math.max(r.baseEnd, paired.baseEnd)
        mergeHunks.push({
          id: `hunk-${mergeHunks.length}`,
          baseStart: bStart,
          baseCount: bEnd - bStart,
          leftHunk: leftH,
          rightHunk: null,
          type: "left-only",
        })
      } else {
        // Conflict
        const bStart = Math.min(r.baseStart, paired.baseStart)
        const bEnd = Math.max(r.baseEnd, paired.baseEnd)
        mergeHunks.push({
          id: `hunk-${mergeHunks.length}`,
          baseStart: bStart,
          baseCount: bEnd - bStart,
          leftHunk: r.side === "left" ? r.hunk : paired.hunk,
          rightHunk: r.side === "right" ? r.hunk : paired.hunk,
          type: "conflict",
        })
      }
    } else {
      // Single-side change
      mergeHunks.push({
        id: `hunk-${mergeHunks.length}`,
        baseStart: r.baseStart,
        baseCount: r.hunk.baseCount,
        leftHunk: r.side === "left" ? r.hunk : null,
        rightHunk: r.side === "right" ? r.hunk : null,
        type: r.side === "left" ? "left-only" : "right-only",
      })
    }
  }

  // Sort by baseStart
  mergeHunks.sort((a, b) => a.baseStart - b.baseStart)

  return mergeHunks
}

// ---------------------------------------------------------------------------
// Result builder
// ---------------------------------------------------------------------------

export interface AppliedHunkInfo {
  id: string
  side: "left" | "right"
}

/**
 * Build the result content by starting from base and applying
 * hunks that have been accepted.
 *
 * @param base     Original base content
 * @param hunks    All merge hunks
 * @param applied  Map of hunk id → which side was applied
 */
export function buildResult(
  base: string,
  hunks: MergeHunk[],
  applied: Map<string, "left" | "right">
): string {
  const baseLines = base.split("\n")
  const result: string[] = []
  let baseIdx = 0

  // Process hunks in order of baseStart
  const sorted = [...hunks].sort((a, b) => a.baseStart - b.baseStart)

  for (const hunk of sorted) {
    // Copy unchanged base lines before this hunk
    while (baseIdx < hunk.baseStart) {
      result.push(baseLines[baseIdx])
      baseIdx++
    }

    const appliedSide = applied.get(hunk.id)

    if (appliedSide) {
      // Apply the chosen side's content
      const diffHunk = appliedSide === "left" ? hunk.leftHunk : hunk.rightHunk
      if (diffHunk) {
        result.push(...diffHunk.newLines)
      }
      // Skip over the base lines that were replaced
      baseIdx = hunk.baseStart + hunk.baseCount
    } else {
      // Not applied — keep base content
      for (let i = 0; i < hunk.baseCount; i++) {
        if (baseIdx < baseLines.length) {
          result.push(baseLines[baseIdx])
          baseIdx++
        }
      }
    }
  }

  // Copy remaining base lines
  while (baseIdx < baseLines.length) {
    result.push(baseLines[baseIdx])
    baseIdx++
  }

  return result.join("\n")
}
