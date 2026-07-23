"use client"

import { useEffect } from "react"
import {
  clampImageZoom,
  IMAGE_ZOOM_STEP,
  type ImagePreviewNavigation,
  type ImageViewerMode,
} from "./image-editor-model"

interface ImageViewerShortcutOptions {
  active: boolean
  mode: ImageViewerMode
  ready: boolean
  zoom: number
  navigation?: ImagePreviewNavigation
  onZoomChange: (zoom: number) => void
  onRotate: (delta: number) => void
  onDownload: () => void
}

function isEditingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  )
}

function handleNavigation(
  key: string,
  navigation: ImagePreviewNavigation | undefined
): boolean {
  if (!navigation || navigation.total <= 1) return false
  if (key === "ArrowLeft" && navigation.index > 0) {
    navigation.onIndexChange(navigation.index - 1)
    return true
  }
  if (key === "ArrowRight" && navigation.index < navigation.total - 1) {
    navigation.onIndexChange(navigation.index + 1)
    return true
  }
  return false
}

export function useImageViewerShortcuts(options: ImageViewerShortcutOptions) {
  useEffect(() => {
    if (!options.active) return
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        options.mode !== "view" ||
        !options.ready ||
        event.ctrlKey ||
        event.metaKey ||
        event.altKey ||
        isEditingTarget(event.target)
      ) {
        return
      }
      const key = event.key
      let handled = handleNavigation(key, options.navigation)
      if (key === "+" || key === "=") {
        options.onZoomChange(clampImageZoom(options.zoom + IMAGE_ZOOM_STEP))
        handled = true
      } else if (key === "-" || key === "_") {
        options.onZoomChange(clampImageZoom(options.zoom - IMAGE_ZOOM_STEP))
        handled = true
      } else if (key === "0") {
        options.onZoomChange(1)
        handled = true
      } else if (key.toLowerCase() === "r") {
        options.onRotate(event.shiftKey ? -90 : 90)
        handled = true
      } else if (key.toLowerCase() === "d") {
        options.onDownload()
        handled = true
      }
      if (handled) event.preventDefault()
    }
    window.addEventListener("keydown", onKeyDown)
    return () => window.removeEventListener("keydown", onKeyDown)
  }, [options])
}
