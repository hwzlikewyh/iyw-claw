"use client"

import {
  useState,
  useEffect,
  useLayoutEffect,
  useRef,
  useCallback,
} from "react"
import { loadReferenceFiles } from "@/lib/reference-file-loader"
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
  /** 文件引用菜单只在首次搜索时加载，普通文件搜索仍按 enabled 自动加载。 */
  automatic?: boolean
}

interface UseFileTreeResult {
  allFiles: FlatFileEntry[]
  loading: boolean
  loaded: boolean
  load: () => Promise<FlatFileEntry[]>
  reset: () => void
}

interface FileLoad {
  path: string
  controller: AbortController
  promise: Promise<FlatFileEntry[]>
}

const useCommitEffect =
  typeof window === "undefined" ? useEffect : useLayoutEffect

export function useFileTree({
  folderPath,
  enabled,
  automatic = true,
}: UseFileTreeOptions): UseFileTreeResult {
  const [allFiles, setAllFiles] = useState<FlatFileEntry[]>([])
  const [loading, setLoading] = useState(false)
  const [loadedPath, setLoadedPath] = useState<string | null>(null)
  const current = useRef<FileLoad | null>(null)

  const reset = useCallback(() => {
    current.current?.controller.abort()
    current.current = null
    setAllFiles([])
    setLoadedPath(null)
    setLoading(false)
  }, [])

  useCommitEffect(() => {
    reset()
    return () => {
      current.current?.controller.abort()
      current.current = null
    }
  }, [folderPath, enabled, reset])

  const load = useCallback((): Promise<FlatFileEntry[]> => {
    if (!enabled || !folderPath) return Promise.resolve([])
    if (current.current?.path === folderPath) return current.current.promise
    const controller = new AbortController()
    const request: FileLoad = {
      path: folderPath,
      controller,
      promise: Promise.resolve([]),
    }
    current.current = request
    setLoading(true)
    request.promise = loadReferenceFiles(folderPath, controller.signal)
      .then((files) => {
        if (current.current !== request) return []
        setLoadedPath(folderPath)
        setAllFiles(files)
        return files
      })
      .catch(() => {
        controller.abort()
        if (current.current === request) {
          current.current = null
          setLoading(false)
        }
        return []
      })
      .finally(() => {
        if (current.current === request) {
          setLoading(false)
        }
      })
    return request.promise
  }, [enabled, folderPath])

  useEffect(() => {
    let canceled = false
    if (automatic && enabled) {
      queueMicrotask(() => {
        if (!canceled) void load()
      })
    }
    return () => {
      canceled = true
    }
  }, [automatic, enabled, load])

  return {
    allFiles,
    loading,
    loaded: loadedPath !== null && loadedPath === folderPath,
    load,
    reset,
  }
}
