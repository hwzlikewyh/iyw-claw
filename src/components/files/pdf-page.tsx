"use client"

import { useImperativeHandle, type Ref } from "react"
import { useTranslations } from "next-intl"
import type { PDFDocumentProxy } from "pdfjs-dist"
import { usePdfViewer } from "./use-pdf-viewer"
import "pdfjs-dist/web/pdf_viewer.css"

export interface PdfPagesHandle {
  goToPage: (pageNumber: number) => void
}

export function PdfPages({
  ref,
  document,
  initialPage,
  zoom,
  onPageChange,
}: {
  ref: Ref<PdfPagesHandle>
  document: PDFDocumentProxy
  initialPage: number
  zoom: number
  onPageChange: (pageNumber: number) => void
}) {
  const { containerRef, error, goToPage } = usePdfViewer({
    document,
    initialPage,
    zoom,
    onPageChange,
  })
  useImperativeHandle(ref, () => ({ goToPage }), [goToPage])
  const t = useTranslations("Folder.chat.workspaceFiles")
  return (
    <div className="flex min-h-0 flex-1 flex-col bg-muted/30">
      {error && (
        <p role="alert" className="shrink-0 p-4 text-sm text-destructive">
          {t("previewError")}
        </p>
      )}
      <div className="relative min-h-0 flex-1">
        <div
          ref={containerRef}
          tabIndex={0}
          className="absolute inset-0 overflow-auto overscroll-contain [overflow-anchor:none]"
        >
          <div className="pdfViewer py-4 [&_.page]:box-content" />
        </div>
      </div>
    </div>
  )
}
