"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  fetchRemoteImageBlob,
  isRemoteImageUrl,
} from "@/components/image-editor/remote-image-source"

export interface ElementSize {
  width: number
  height: number
}

export interface LoadedImageState {
  source: string
  image: HTMLImageElement | null
  originalBlob: Blob | null
  failed: boolean
}

export function useElementSize(): [
  React.RefCallback<HTMLDivElement>,
  ElementSize,
] {
  const [size, setSize] = useState<ElementSize>({ width: 0, height: 0 })
  const observerRef = useRef<ResizeObserver | null>(null)
  const callbackRef = useCallback((element: HTMLDivElement | null) => {
    observerRef.current?.disconnect()
    observerRef.current = null
    if (!element) return
    const update = (bounds: DOMRectReadOnly) =>
      setSize({ width: bounds.width, height: bounds.height })
    update(element.getBoundingClientRect())
    observerRef.current = new ResizeObserver((entries) => {
      const bounds = entries[0]?.contentRect
      if (bounds) update(bounds)
    })
    observerRef.current.observe(element)
  }, [])
  return [callbackRef, size]
}

export function useLoadedImage(src: string, open: boolean): LoadedImageState {
  const source = open ? src : ""
  const [state, setState] = useState<LoadedImageState>({
    source: "",
    image: null,
    originalBlob: null,
    failed: false,
  })
  useEffect(() => {
    if (!source) return
    let active = true
    let image: HTMLImageElement | null = null
    let objectUrl: string | null = null
    const fail = (error?: unknown) => {
      if (objectUrl) {
        URL.revokeObjectURL(objectUrl)
        objectUrl = null
      }
      if (error && active)
        console.error("[image-viewer] remote image load failed", { error })
      if (active) {
        setState({ source, image: null, originalBlob: null, failed: true })
      }
    }
    const load = async () => {
      try {
        const originalBlob = isRemoteImageUrl(source)
          ? await fetchRemoteImageBlob(source)
          : null
        if (!active) return
        objectUrl = originalBlob ? URL.createObjectURL(originalBlob) : null
        image = new window.Image()
        image.decoding = "async"
        image.onload = () => {
          if (active) setState({ source, image, originalBlob, failed: false })
        }
        image.onerror = () => fail()
        image.src = objectUrl ?? source
      } catch (error) {
        fail(error)
      }
    }
    void load()
    return () => {
      active = false
      if (image) {
        image.onload = null
        image.onerror = null
      }
      if (objectUrl) URL.revokeObjectURL(objectUrl)
    }
  }, [source])
  return state.source === source
    ? state
    : { source, image: null, originalBlob: null, failed: false }
}
