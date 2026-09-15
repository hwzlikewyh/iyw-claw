"use client"

import { useEffect, useRef, useState } from "react"
import {
  getDocument,
  GlobalWorkerOptions,
  TextLayer,
  type PDFDocumentLoadingTask,
  type PDFDocumentProxy,
} from "pdfjs-dist"
import { acquirePreviewSlot } from "./preview-task-slots"

GlobalWorkerOptions.workerSrc = "/preview-assets/pdf/pdf.worker.min.mjs"
const MAX_IMAGE_PIXELS = 16_000_000

export function usePdfDocument(src: string, visible: boolean) {
  const [document, setDocument] = useState<PDFDocumentProxy | null>(null)
  const [error, setError] = useState(false)
  const [needsPassword, setNeedsPassword] = useState(false)
  const passwordCallback = useRef<((password: string) => void) | null>(null)
  useEffect(() => {
    if (!visible) return
    const controller = new AbortController()
    let task: PDFDocumentLoadingTask | undefined
    let release: (() => void) | undefined
    const destroy = () => destroyPdfTask(task, release)
    void acquirePreviewSlot(controller.signal)
      .then(async (slot) => {
        release = slot
        if (controller.signal.aborted) {
          slot()
          return
        }
        setError(false)
        task = createPdfTask(src)
        task.onPassword = (update: (password: string) => void) => {
          if (controller.signal.aborted) return
          passwordCallback.current = update
          setNeedsPassword(true)
        }
        const loaded = await task.promise
        if (!controller.signal.aborted) {
          setDocument(loaded)
          setNeedsPassword(false)
        }
      })
      .catch(() => {
        if (!controller.signal.aborted) setError(true)
        destroy()
      })
    return () => {
      controller.abort()
      passwordCallback.current = null
      setDocument(null)
      destroy()
    }
  }, [src, visible])
  return { document, error, needsPassword, passwordCallback }
}

function destroyPdfTask(
  task: PDFDocumentLoadingTask | undefined,
  release: (() => void) | undefined
) {
  if (!task) {
    release?.()
    return
  }
  void task
    .destroy()
    .catch(() => {})
    .finally(() => {
      TextLayer.cleanup()
      release?.()
    })
}

function createPdfTask(src: string) {
  return getDocument({
    url: src,
    cMapUrl: "/preview-assets/pdf/cmaps/",
    cMapPacked: true,
    standardFontDataUrl: "/preview-assets/pdf/standard_fonts/",
    wasmUrl: "/preview-assets/pdf/wasm/",
    disableAutoFetch: true,
    disableStream: true,
    maxImageSize: MAX_IMAGE_PIXELS,
    canvasMaxAreaInBytes: MAX_IMAGE_PIXELS * 4,
  })
}
