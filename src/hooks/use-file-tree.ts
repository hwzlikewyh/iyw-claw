"use client"

import { useState, useEffect, useRef, useCallback } from "react"
import ig from "ignore"
import { getFileTree, readFilePreview } from "@/lib/api"
import type { FileTreeNode } from "@/lib/types"

export interface FlatFileEntry {
  name: string
  /** Relative path from folder root (same as FileTreeNode.path) */
  relativePath: string
  kind: "file" | "dir"
  /** Pre-computed lowercase relativePath for filtering */
  lowerPath: string
  /** Pre-computed lowercase name for filtering */
  lowerName: string
}

export function flattenTree(nodes: FileTreeNode[]): FlatFileEntry[] {
  const entries: FlatFileEntry[] = []
  function walk(node: FileTreeNode) {
    entries.push({
      name: node.name,
      relativePath: node.path,
      kind: node.kind,
      lowerPath: node.path.toLowerCase(),
      lowerName: node.name.toLowerCase(),
    })
    if (node.kind === "dir" && node.children) {
      for (const child of node.children) {
        walk(child)
      }
    }
  }
  for (const node of nodes) {
    walk(node)
  }
  return entries
}

/** Check whether any ancestor directory of `path` is in `ignoredDirs`. */
export function hasIgnoredAncestor(
  path: string,
  ignoredDirs: Set<string>
): boolean {
  let idx = path.indexOf("/")
  while (idx !== -1) {
    if (ignoredDirs.has(path.slice(0, idx))) return true
    idx = path.indexOf("/", idx + 1)
  }
  return false
}

interface UseFileTreeOptions {
  folderPath: string | undefined
  enabled: boolean
}

interface UseFileTreeResult {
  allFiles: FlatFileEntry[]
  loading: boolean
  loaded: boolean
  /** Clear cached data so the next `enabled=true` triggers a fresh load. */
  reset: () => void
}

export function useFileTree({
  folderPath,
  enabled,
}: UseFileTreeOptions): UseFileTreeResult {
  const [allFiles, setAllFiles] = useState<FlatFileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const loadedForPathRef = useRef<string | null>(null)

  useEffect(() => {
    if (!enabled || !folderPath) return
    if (loadedForPathRef.current === folderPath) return

    let canceled = false
    setLoading(true)

    async function load() {
      try {
        const tree = await getFileTree(folderPath!, 10)
        const flat = flattenTree(tree)

        // Collect all .gitignore files from the tree
        const gitignoreEntries = flat.filter(
          (f) => f.kind === "file" && f.name === ".gitignore"
        )

        // Build matchers keyed by directory prefix
        const matchers: {
          prefix: string
          matcher: ReturnType<typeof ig>
        }[] = []
        await Promise.all(
          gitignoreEntries.map(async (entry) => {
            try {
              const result = await readFilePreview(
                folderPath!,
                entry.relativePath
              )
              const lastSlash = entry.relativePath.lastIndexOf("/")
              const dir =
                lastSlash === -1 ? "" : entry.relativePath.slice(0, lastSlash)
              matchers.push({
                prefix: dir ? dir + "/" : "",
                matcher: ig().add(result.content),
              })
            } catch {
              // skip unreadable .gitignore
            }
          })
        )

        // Sort matchers by prefix length (shortest/root first)
        matchers.sort((a, b) => a.prefix.length - b.prefix.length)

        // Filter: check each entry against all applicable .gitignore matchers
        const ignoredDirs = new Set<string>()
        const filtered = flat.filter((f) => {
          if (f.name === ".gitignore") return false
          if (hasIgnoredAncestor(f.relativePath, ignoredDirs)) return false
          for (const { prefix, matcher } of matchers) {
            if (!f.relativePath.startsWith(prefix)) continue
            const relPath = f.relativePath.slice(prefix.length)
            if (!relPath) continue
            const testPath = f.kind === "dir" ? `${relPath}/` : relPath
            if (matcher.ignores(testPath)) {
              if (f.kind === "dir") ignoredDirs.add(f.relativePath)
              return false
            }
          }
          return true
        })

        if (!canceled) {
          setAllFiles(filtered)
          loadedForPathRef.current = folderPath!
        }
      } catch {
        if (!canceled) setAllFiles([])
      } finally {
        if (!canceled) setLoading(false)
      }
    }

    void load()
    return () => {
      canceled = true
    }
  }, [enabled, folderPath])

  const reset = useCallback(() => {
    loadedForPathRef.current = null
    setAllFiles([])
  }, [])

  return {
    allFiles,
    loading,
    loaded: loadedForPathRef.current === folderPath,
    reset,
  }
}
