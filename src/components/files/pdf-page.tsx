"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import {
  TextLayer,
  type PDFDocumentProxy,
  type PDFPageProxy,
  type RenderTask,
} from "pdfjs-dist"
import "pdfjs-dist/web/pdf_viewer.css"

const MAX_CANVAS_PIXELS = 8_000_000
const PAGE_PADDING = 32

export function PdfPage({
  document,
  pageNumber,
  zoom,
}: {
  document: PDFDocumentProxy
  pageNumber: number
  zoom: number
}) {
  const viewportRef = useRef<HTMLDivElement>(null)
  const canvasRef = useRef<HTMLCanvasElement>(null)
  const textRef = useRef<HTMLDivElement>(null)
  const [width, setWidth] = useState(0)
  const [error, setError] = useState(false)
  const t = useTranslations("Folder.chat.workspaceFiles")
  useEffect(() => {
    const element = viewportRef.current
    if (!element) return
    const observer = new ResizeObserver(([entry]) =>
      setWidth(Math.floor(entry.contentRect.width))
    )
    observer.observe(element)
    return () => observer.disconnect()
  }, [])
  useEffect(() => {
    const canvas = canvasRef.current,
      text = textRef.current
    if (!canvas || !text || width <= PAGE_PADDING) return
    return renderPage({
      document,
      pageNumber,
      zoom,
      width,
      canvas,
      text,
      onError: () => setError(true),
    })
  }, [document, pageNumber, width, zoom])
  return (
    <div
      ref={viewportRef}
      className="min-h-0 flex-1 overflow-auto bg-muted/30 p-4"
    >
      {error && (
        <p role="alert" className="text-sm text-destructive">
          {t("previewError")}
        </p>
      )}
      <div className="relative mx-auto w-fit bg-white shadow-sm">
        <canvas ref={canvasRef} />
        <div ref={textRef} className="textLayer" />
      </div>
    </div>
  )
}

interface PageRenderOptions {
  document: PDFDocumentProxy
  pageNumber: number
  zoom: number
  width: number
  canvas: HTMLCanvasElement
  text: HTMLDivElement
  onError: () => void
}

function renderPage(options: PageRenderOptions) {
  const { document, pageNumber, canvas, text } = options
  let disposed = false,
    page: PDFPageProxy | undefined
  let render: RenderTask | undefined, layer: TextLayer | undefined
  void document
    .getPage(pageNumber)
    .then(async (value) => {
      if (disposed) return
      page = value
      const viewport = sizeCanvas(page, options)
      render = page.render({
        canvas,
        viewport: viewport.view,
        transform: viewport.transform,
      })
      layer = new TextLayer({
        textContentSource: page.streamTextContent(),
        container: text,
        viewport: viewport.view,
      })
      await Promise.all([render.promise, layer.render()])
    })
    .catch((error: unknown) => {
      if (!disposed) {
        options.onError()
        console.warn("[preview] PDF render failed", {
          errorType: error instanceof Error ? error.name : "unknown",
        })
      }
    })
  return () => {
    disposed = true
    render?.cancel()
    layer?.cancel()
    text.replaceChildren()
    canvas.width = 0
    canvas.height = 0
    page?.cleanup()
  }
}

function sizeCanvas(page: PDFPageProxy, options: PageRenderOptions) {
  const { canvas, text, width, zoom } = options
  const natural = page.getViewport({ scale: 1 })
  const view = page.getViewport({
    scale: ((width - PAGE_PADDING) / natural.width) * zoom,
  })
  const ratio = Math.min(
    window.devicePixelRatio || 1,
    Math.sqrt(MAX_CANVAS_PIXELS / (view.width * view.height))
  )
  canvas.width = Math.max(1, Math.floor(view.width * ratio))
  canvas.height = Math.max(1, Math.floor(view.height * ratio))
  canvas.style.width = `${view.width}px`
  canvas.style.height = `${view.height}px`
  text.style.setProperty("--scale-factor", String(view.scale))
  text.style.setProperty("--total-scale-factor", String(view.scale))
  return { view, transform: [ratio, 0, 0, ratio, 0, 0] }
}
