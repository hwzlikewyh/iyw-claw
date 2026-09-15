"use client"

import { useEffect, useRef, useState } from "react"
import { getServerBaseUrl, getActiveRemoteConnectionId } from "@/lib/transport"

import { acquireResource, type ResourceState } from "./preview-resource-client"

export function usePreviewVisibility<T extends HTMLElement = HTMLDivElement>() {
  const ref = useRef<T>(null)
  const [visible, setVisible] = useState(false)
  useEffect(() => {
    const element = ref.current
    if (!element) return
    let intersects = false
    const update = () => setVisible(intersects && !document.hidden)
    const observer = new IntersectionObserver(([entry]) => {
      intersects = entry.isIntersecting
      update()
    })
    observer.observe(element)
    document.addEventListener("visibilitychange", update)
    return () => {
      observer.disconnect()
      document.removeEventListener("visibilitychange", update)
    }
  }, [])
  return { ref, visible }
}

export function usePreviewResource(
  rootPath: string,
  path: string,
  enabled: boolean
) {
  const [state, setState] = useState<ResourceState | null>(null)
  const connectionId = getActiveRemoteConnectionId()
  const base = getServerBaseUrl()
  const key = `${connectionId ?? "local"}\0${base}\0${rootPath}\0${path}`
  useEffect(() => {
    if (!enabled) return
    const release = acquireResource({ rootPath, path, key }, setState)
    return () => {
      release()
      setState(null)
    }
  }, [enabled, key, path, rootPath])
  return enabled && state?.key === key ? state : null
}
