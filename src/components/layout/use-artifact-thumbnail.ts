import { useEffect, useMemo, useRef, useState } from "react"
import { useActiveFolder } from "@/contexts/active-folder-context"
import { useAppWorkspaceStore } from "@/stores/app-workspace-store"
import type { TaskArtifactInfo } from "@/lib/api"
import { findOwningFolder } from "@/lib/file-open-target"
import { toAbsoluteFilePath } from "@/lib/file-path-display"
import {
  loadLocalArtifactThumbnail,
  type LocalThumbnailRequest,
} from "@/lib/local-artifact-thumbnail"
import { getTransport, type Transport } from "@/lib/transport"

const MIN_EDGE = 64
const MAX_EDGE = 2048

export function useArtifactThumbnail(item: TaskArtifactInfo, image: boolean) {
  const [loaded, setLoaded] = useState<{
    key: string
    src: string
    transport: Transport
  } | null>(null)
  const { ref, edge, fit, visible } = useThumbnailViewport(
    image && item.status === "available",
    () => setLoaded(null)
  )
  const request = useThumbnailRequest(item, edge, fit)
  const transport = getTransport()
  const key = JSON.stringify(request)
  useEffect(() => {
    if (!image || !visible || !edge || !request || item.status !== "available")
      return
    const controller = new AbortController()
    void loadLocalArtifactThumbnail(transport, request, controller.signal)
      .then((src) => {
        if (!controller.signal.aborted) setLoaded({ key, src, transport })
      })
      .catch(() => {})
    return () => {
      controller.abort()
    }
  }, [image, visible, edge, request, item.status, key, transport])
  return {
    ref,
    thumbnail:
      visible && loaded?.key === key && loaded.transport === transport
        ? loaded.src
        : null,
  }
}

function useThumbnailRequest(
  item: TaskArtifactInfo,
  edge: number,
  fit: LocalThumbnailRequest["fit"]
): LocalThumbnailRequest | null {
  const folders = useAppWorkspaceStore((state) => state.folders)
  const { activeFolder } = useActiveFolder()
  const folderPath =
    folders.find((folder) => folder.id === item.folderId)?.path ??
    activeFolder?.path
  const absolutePath = toAbsoluteFilePath(item.path, folderPath)
  const owner = absolutePath ? findOwningFolder(absolutePath, folders) : null
  const rootPath = owner?.rootPath
  const path = owner?.relPath ?? absolutePath
  return useMemo(
    () =>
      path
        ? {
            path,
            rootPath,
            edge,
            fit,
            revision: item.lastCheckedAt ?? "",
          }
        : null,
    [path, rootPath, edge, fit, item.lastCheckedAt]
  )
}

function useThumbnailViewport(enabled: boolean, onHidden: () => void) {
  const ref = useRef<HTMLSpanElement>(null)
  const [edge, setEdge] = useState(0)
  const [fit, setFit] = useState<LocalThumbnailRequest["fit"]>("cover")
  const [visible, setVisible] = useState(false)
  const hiddenCallback = useRef(onHidden)
  useEffect(() => {
    hiddenCallback.current = onHidden
  }, [onHidden])
  useEffect(() => {
    const element = ref.current
    if (!element || !enabled) return
    const measure = () => {
      const rect = element.getBoundingClientRect()
      setFit(
        getComputedStyle(element).backgroundSize === "contain"
          ? "contain"
          : "cover"
      )
      const pixels =
        Math.max(rect.width, rect.height) * (window.devicePixelRatio || 1)
      setEdge(
        Math.min(
          MAX_EDGE,
          Math.max(MIN_EDGE, Math.ceil(pixels / MIN_EDGE) * MIN_EDGE)
        )
      )
    }
    const resize = new ResizeObserver(measure)
    const observer = new IntersectionObserver(
      ([entry]) => {
        setVisible(entry.isIntersecting)
        if (entry.isIntersecting) measure()
        else hiddenCallback.current()
      },
      { rootMargin: "200px" }
    )
    resize.observe(element)
    observer.observe(element)
    return () => {
      resize.disconnect()
      observer.disconnect()
    }
  }, [enabled])
  return { ref, edge, fit, visible }
}
