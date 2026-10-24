import { exceedsLineDiffBudget } from "@/lib/line-change-stats"

export interface DiffHunk {
  /** Start index in the "old" (base) array, 0-based */
  baseStart: number
  /** Number of lines removed from base (0 = pure insertion) */
  baseCount: number
  /** Replacement lines from the "new" side */
  newLines: string[]
}

// 只比较真实改动窗口，避免局部编辑为整份文件分配二维表。
export function computeLineDiff(a: string[], b: string[]): DiffHunk[] {
  let prefix = 0
  while (prefix < a.length && prefix < b.length && a[prefix] === b[prefix])
    prefix += 1
  let suffix = 0
  while (
    suffix < a.length - prefix &&
    suffix < b.length - prefix &&
    a[a.length - suffix - 1] === b[b.length - suffix - 1]
  )
    suffix += 1
  const oldWindow = a.slice(prefix, a.length - suffix)
  const newWindow = b.slice(prefix, b.length - suffix)
  if (!oldWindow.length && !newWindow.length) return []
  // 沿用统一差异预算；大范围改写保留为完整替换块，不丢弃任一侧内容。
  if (
    !oldWindow.length ||
    !newWindow.length ||
    exceedsLineDiffBudget(oldWindow, newWindow)
  )
    return [
      { baseStart: prefix, baseCount: oldWindow.length, newLines: newWindow },
    ]
  return computeChangedWindow(oldWindow, newWindow).map((hunk) => ({
    ...hunk,
    baseStart: hunk.baseStart + prefix,
  }))
}

// ---------------------------------------------------------------------------
// LCS-based line diff
// ---------------------------------------------------------------------------

/**
 * Compute the Longest Common Subsequence table for two string arrays.
 * Returns a 2D array where dp[i][j] = LCS length for a[0..i-1], b[0..j-1].
 */
function lcsTable(a: string[], b: string[]): number[][] {
  const m = a.length
  const n = b.length
  const dp: number[][] = Array.from({ length: m + 1 }, () =>
    new Array<number>(n + 1).fill(0)
  )
  for (let i = 1; i <= m; i++) {
    for (let j = 1; j <= n; j++) {
      if (a[i - 1] === b[j - 1]) {
        dp[i][j] = dp[i - 1][j - 1] + 1
      } else {
        dp[i][j] = Math.max(dp[i - 1][j], dp[i][j - 1])
      }
    }
  }
  return dp
}

/**
 * Backtrack the LCS table to produce edit operations.
 * Returns an array of { type, aIdx, bIdx } entries.
 */
interface EditOp {
  type: "equal" | "delete" | "insert"
  aIdx: number // index in a (-1 for insert)
  bIdx: number // index in b (-1 for delete)
}

function backtrackLCS(a: string[], b: string[], dp: number[][]): EditOp[] {
  const ops: EditOp[] = []
  let i = a.length
  let j = b.length

  while (i > 0 || j > 0) {
    if (i > 0 && j > 0 && a[i - 1] === b[j - 1]) {
      ops.push({ type: "equal", aIdx: i - 1, bIdx: j - 1 })
      i--
      j--
    } else if (j > 0 && (i === 0 || dp[i][j - 1] >= dp[i - 1][j])) {
      ops.push({ type: "insert", aIdx: -1, bIdx: j - 1 })
      j--
    } else {
      ops.push({ type: "delete", aIdx: i - 1, bIdx: -1 })
      i--
    }
  }

  return ops.reverse()
}

/**
 * Compute line-level diff hunks between old (a) and new (b) arrays.
 */
function computeChangedWindow(a: string[], b: string[]): DiffHunk[] {
  const dp = lcsTable(a, b)
  const ops = backtrackLCS(a, b, dp)

  const hunks: DiffHunk[] = []
  let idx = 0

  while (idx < ops.length) {
    const op = ops[idx]

    if (op.type === "equal") {
      idx++
      continue
    }

    // Start of a change region
    let baseStart = op.type === "delete" ? op.aIdx : -1
    let baseCount = 0
    const newLines: string[] = []

    while (idx < ops.length && ops[idx].type !== "equal") {
      const cur = ops[idx]
      if (cur.type === "delete") {
        if (baseStart === -1) baseStart = cur.aIdx
        baseCount++
      } else {
        // insert
        if (baseStart === -1) {
          // Pure insertion — position it at the next base line
          // Find the previous equal op's aIdx + 1, or 0
          baseStart = findInsertionPoint(ops, idx)
        }
        newLines.push(b[cur.bIdx])
      }
      idx++
    }

    hunks.push({ baseStart, baseCount, newLines })
  }

  return hunks
}

/**
 * For a pure insertion (no deletes in this hunk), determine
 * where in the base array to anchor it.
 */
function findInsertionPoint(ops: EditOp[], currentIdx: number): number {
  // Walk backwards to find the last "equal" or "delete" op
  for (let k = currentIdx - 1; k >= 0; k--) {
    if (ops[k].type === "equal" || ops[k].type === "delete") {
      return ops[k].aIdx + 1
    }
  }
  // If nothing found, insert at start
  return 0
}
