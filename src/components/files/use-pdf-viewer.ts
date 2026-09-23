"use client"

import { useCallback, useEffect, useRef, useState } from "react"
import {
  AnnotationEditorType,
  AnnotationMode,
  type PDFDocumentProxy,
} from "pdfjs-dist"
import { EventBus, PDFViewer } from "pdfjs-dist/web/pdf_viewer.mjs"

const MAX_CANVAS_PIXELS = 8_000_000
const PAGE_PADDING = 40

interface PdfViewerOptions {
  document: PDFDocumentProxy
  initialPage: number
  zoom: number
  onPageChange: (pageNumber: number) => void
}

export function usePdfViewer(options: PdfViewerOptions) {
  const { document, initialPage, zoom, onPageChange } = options
  const containerRef = useRef<HTMLDivElement>(null)
  const viewerRef = useRef<PDFViewer | null>(null)
  const zoomRef = useRef(zoom)
  const initialPageRef = useRef(initialPage)
  const [error, setError] = useState(false)
  useEffect(() => {
    zoomRef.current = zoom
    if (viewerRef.current) resizePdfViewer(viewerRef.current, zoom)
  }, [zoom])
  useEffect(() => {
    const container = containerRef.current
    if (!container) return
    const session = createPdfViewer(container)
    const { viewer, eventBus, signal } = session
    viewerRef.current = viewer
    const resize = () => resizePdfViewer(viewer, zoomRef.current)
    const onReady = () => {
      resize()
      viewer.currentPageNumber = Math.min(
        initialPageRef.current,
        document.numPages
      )
    }
    const onError = (reason: unknown) => {
      if (signal.aborted) return
      setError(true)
      console.warn("[preview] PDF render failed", {
        pageNumber: viewer.currentPageNumber,
        totalPages: document.numPages,
        errorType: reason instanceof Error ? reason.name : "unknown",
      })
    }
    bindPdfEvents({ eventBus, signal, onReady, onPageChange, onError })
    const observer = new ResizeObserver(resize)
    observer.observe(container)
    viewer.setDocument(document)
    void viewer.pagesPromise.catch(onError)
    return () => {
      observer.disconnect()
      viewerRef.current = null
      session.dispose()
    }
  }, [document, onPageChange])
  const goToPage = useCallback((pageNumber: number) => {
    viewerRef.current?.scrollPageIntoView({ pageNumber })
  }, [])
  return { containerRef, error, goToPage }
}

function createPdfViewer(container: HTMLDivElement) {
  const controller = new AbortController()
  const eventBus = new EventBus()
  const options = {
    container,
    eventBus,
    abortSignal: controller.signal,
    maxCanvasPixels: MAX_CANVAS_PIXELS,
    annotationMode: AnnotationMode.DISABLE,
    annotationEditorMode: AnnotationEditorType.DISABLE,
    enableAutoLinking: false,
    supportsPinchToZoom: false,
  }
  const viewer = new PDFViewer(options)
  return {
    viewer,
    eventBus,
    signal: controller.signal,
    dispose: () => {
      controller.abort()
      // @ts-expect-error PDF.js supports null for teardown; its types omit it.
      viewer.setDocument(null)
      void viewer.l10n?.destroy()
    },
  }
}

function resizePdfViewer(viewer: PDFViewer, zoom: number) {
  const firstPage = viewer.getPageView(0)
  const width = viewer.container.clientWidth - PAGE_PADDING
  if (!firstPage || width <= 0) return
  viewer.currentScale = (width / firstPage.width) * firstPage.scale * zoom
  viewer.update()
}

interface PdfViewerEvents {
  eventBus: EventBus
  signal: AbortSignal
  onReady: () => void
  onPageChange: (pageNumber: number) => void
  onError: (reason: unknown) => void
}

function bindPdfEvents({
  eventBus,
  signal,
  onReady,
  onPageChange,
  onError,
}: PdfViewerEvents) {
  eventBus.on("pagesinit", onReady, { signal })
  eventBus.on(
    "pagechanging",
    ({ pageNumber }: { pageNumber: number }) => onPageChange(pageNumber),
    { signal }
  )
  eventBus.on(
    "pagerendered",
    ({ error }: { error?: unknown }) => {
      if (error) onError(error)
    },
    { signal }
  )
}
