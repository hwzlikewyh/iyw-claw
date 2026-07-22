"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import type { Dispatch, MutableRefObject, SetStateAction } from "react"

import { toErrorMessage } from "@/lib/app-error"
import { getFileTree } from "@/lib/api"
import { joinFsPath } from "@/lib/path-utils"
import type { FileTreeNode } from "@/lib/types"

const DIRECTORY_CACHE_TTL_MS = 2_000
const DIRECTORY_CACHE_MAX_ENTRIES = 96

interface CachedDirectory {
  expiresAt: number
  nodes: FileTreeNode[]
}

const directoryCache = new Map<string, CachedDirectory>()
const directoryRequests = new Map<string, Promise<FileTreeNode[]>>()

function directoryKey(rootPath: string, relativePath: string): string {
  return `${rootPath}\0${relativePath}`
}

function readCachedDirectory(key: string): FileTreeNode[] | null {
  const cached = directoryCache.get(key)
  if (!cached) return null
  if (cached.expiresAt <= Date.now()) {
    directoryCache.delete(key)
    return null
  }
  directoryCache.delete(key)
  directoryCache.set(key, cached)
  return cached.nodes
}

function cacheDirectory(key: string, nodes: FileTreeNode[]): void {
  directoryCache.set(key, {
    expiresAt: Date.now() + DIRECTORY_CACHE_TTL_MS,
    nodes,
  })
  while (directoryCache.size > DIRECTORY_CACHE_MAX_ENTRIES) {
    const oldestKey = directoryCache.keys().next().value
    if (oldestKey === undefined) break
    directoryCache.delete(oldestKey)
  }
}

function prefixNodePaths(
  nodes: FileTreeNode[],
  parentPath: string
): FileTreeNode[] {
  return nodes.map((node) => {
    const path = parentPath ? `${parentPath}/${node.path}` : node.path
    if (node.kind === "file") return { ...node, path }
    return {
      ...node,
      path,
      children: prefixNodePaths(node.children, parentPath),
    }
  })
}

function requestDirectory(
  rootPath: string,
  relativePath: string
): Promise<FileTreeNode[]> {
  const key = directoryKey(rootPath, relativePath)
  const cached = readCachedDirectory(key)
  if (cached) return Promise.resolve(cached)
  const pending = directoryRequests.get(key)
  if (pending) return pending

  const request = getFileTree(joinFsPath(rootPath, relativePath), 1)
    .then((nodes) => prefixNodePaths(nodes, relativePath))
    .then((nodes) => {
      cacheDirectory(key, nodes)
      return nodes
    })
    .finally(() => directoryRequests.delete(key))
  directoryRequests.set(key, request)
  return request
}

function replaceDirectoryChildren(
  nodes: FileTreeNode[],
  directoryPath: string,
  children: FileTreeNode[]
): FileTreeNode[] {
  return nodes.map((node) => {
    if (node.kind === "file") return node
    if (node.path === directoryPath) return { ...node, children }
    return {
      ...node,
      children: replaceDirectoryChildren(
        node.children,
        directoryPath,
        children
      ),
    }
  })
}

export function prefetchWorkspaceRoot(rootPath: string): void {
  void requestDirectory(rootPath, "").catch(() => {})
}

export interface WorkspaceTreeState {
  nodes: FileTreeNode[]
  loading: boolean
  error: string | null
  loadedPaths: Set<string>
  loadingPaths: Set<string>
  pathErrors: Map<string, string>
  loadDirectory: (path: string) => Promise<void>
}

interface RootLoadOptions {
  activeRootRef: MutableRefObject<string>
  setNodes: Dispatch<SetStateAction<FileTreeNode[]>>
  setLoading: Dispatch<SetStateAction<boolean>>
  setError: Dispatch<SetStateAction<string | null>>
  setLoadedPaths: Dispatch<SetStateAction<Set<string>>>
}

function useRootDirectoryLoad(rootPath: string, options: RootLoadOptions) {
  const { activeRootRef, setNodes, setLoading, setError, setLoadedPaths } =
    options
  useEffect(() => {
    activeRootRef.current = rootPath
    let cancelled = false
    requestDirectory(rootPath, "")
      .then((rootNodes) => {
        if (cancelled) return
        setNodes(rootNodes)
        setLoadedPaths((paths) => new Set(paths).add(""))
        setError(null)
      })
      .catch((reason) => {
        if (cancelled) return
        const message = toErrorMessage(reason)
        console.error("[workspace-files] root directory load failed", {
          message,
        })
        setError(message)
      })
      .finally(() => {
        if (!cancelled) setLoading(false)
      })
    return () => {
      cancelled = true
      activeRootRef.current = ""
    }
  }, [activeRootRef, rootPath, setError, setLoadedPaths, setLoading, setNodes])
}

interface DirectoryLoaderOptions {
  rootPath: string
  activeRootRef: MutableRefObject<string>
  setNodes: Dispatch<SetStateAction<FileTreeNode[]>>
  setLoadedPaths: Dispatch<SetStateAction<Set<string>>>
  setLoadingPaths: Dispatch<SetStateAction<Set<string>>>
  setPathErrors: Dispatch<SetStateAction<Map<string, string>>>
}

function useDirectoryLoader(options: DirectoryLoaderOptions) {
  const {
    rootPath,
    activeRootRef,
    setNodes,
    setLoadedPaths,
    setLoadingPaths,
    setPathErrors,
  } = options
  return useCallback(
    async (path: string) => {
      setLoadingPaths((paths) => new Set(paths).add(path))
      setPathErrors((errors) => {
        const next = new Map(errors)
        next.delete(path)
        return next
      })
      try {
        const children = await requestDirectory(rootPath, path)
        if (activeRootRef.current !== rootPath) return
        setNodes((nodes) => replaceDirectoryChildren(nodes, path, children))
        setLoadedPaths((paths) => new Set(paths).add(path))
      } catch (reason) {
        if (activeRootRef.current !== rootPath) return
        const message = toErrorMessage(reason)
        console.error("[workspace-files] directory load failed", {
          directory: path,
          message,
        })
        setPathErrors((errors) => new Map(errors).set(path, message))
      } finally {
        if (activeRootRef.current === rootPath) {
          setLoadingPaths((paths) => {
            const next = new Set(paths)
            next.delete(path)
            return next
          })
        }
      }
    },
    [
      activeRootRef,
      rootPath,
      setLoadedPaths,
      setLoadingPaths,
      setNodes,
      setPathErrors,
    ]
  )
}

export function useLazyWorkspaceTree(rootPath: string): WorkspaceTreeState {
  const [cachedRoot] = useState(() =>
    readCachedDirectory(directoryKey(rootPath, ""))
  )
  const [nodes, setNodes] = useState<FileTreeNode[]>(cachedRoot ?? [])
  const [loading, setLoading] = useState(cachedRoot === null)
  const [error, setError] = useState<string | null>(null)
  const [loadedPaths, setLoadedPaths] = useState(
    () => new Set(cachedRoot === null ? [] : [""])
  )
  const [loadingPaths, setLoadingPaths] = useState(new Set<string>())
  const [pathErrors, setPathErrors] = useState(new Map<string, string>())
  const activeRootRef = useRef(rootPath)
  const rootOptions = {
    activeRootRef,
    setNodes,
    setLoading,
    setError,
    setLoadedPaths,
  }
  useRootDirectoryLoad(rootPath, rootOptions)
  const loadDirectory = useDirectoryLoader({
    rootPath,
    activeRootRef,
    setNodes,
    setLoadedPaths,
    setLoadingPaths,
    setPathErrors,
  })
  return {
    nodes,
    loading,
    error,
    loadedPaths,
    loadingPaths,
    pathErrors,
    loadDirectory,
  }
}
