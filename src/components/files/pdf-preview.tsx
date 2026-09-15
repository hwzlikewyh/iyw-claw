"use client"

import { useState } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { PreviewToolbar } from "./preview-toolbar"
import { PdfPage } from "./pdf-page"
import { PdfNavigation, PdfPassword } from "./pdf-controls"
import { usePreviewVisibility } from "./use-preview-resource"
import { usePdfDocument } from "./use-pdf-document"

export function PdfPreview({ src, title }: { src: string; title: string }) {
  const { ref, visible } = usePreviewVisibility()
  const [page, setPage] = useState(1)
  const [zoom, setZoom] = useState(1)
  const { document, error, needsPassword, passwordCallback } = usePdfDocument(
    src,
    visible
  )
  const messages = useTranslations("Folder.chat.workspaceFiles")
  return (
    <div ref={ref} className="flex h-full min-h-0 flex-col" aria-label={title}>
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        onFullscreen={() =>
          void ref.current?.requestFullscreen().catch(() => {})
        }
      >
        <PdfNavigation
          page={page}
          pages={document?.numPages ?? 0}
          onPage={setPage}
        />
      </PreviewToolbar>
      {needsPassword && (
        <PdfPassword
          onSubmit={(password) => passwordCallback.current?.(password)}
        />
      )}
      {error ? (
        <p role="alert" className="p-4 text-sm text-destructive">
          {messages("previewError")}
        </p>
      ) : document && visible ? (
        <PdfPage key={page} document={document} pageNumber={page} zoom={zoom} />
      ) : (
        <div className="flex flex-1 items-center justify-center">
          <Loader2 className="size-4 animate-spin" />
        </div>
      )}
    </div>
  )
}
