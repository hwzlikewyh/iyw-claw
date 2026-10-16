"use client"

import { useRef, useState, type RefObject } from "react"
import { Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { PreviewToolbar } from "./preview-toolbar"
import { PreviewFullscreen } from "./preview-fullscreen"
import { PdfPages, type PdfPagesHandle } from "./pdf-page"
import { PdfNavigation, PdfPassword } from "./pdf-controls"
import { usePreviewVisibility } from "./use-preview-resource"
import { usePdfDocument } from "./use-pdf-document"

interface PdfPreviewProps {
  src: string
  title: string
  compact?: boolean
}

interface PdfPreviewContentProps extends PdfPreviewProps {
  fullscreen: boolean
  onToggleFullscreen: () => void
}

interface PdfPreviewBodyProps {
  pdf: ReturnType<typeof usePdfDocument>
  visible: boolean
  page: number
  zoom: number
  pagesRef: RefObject<PdfPagesHandle | null>
  onPageChange: (page: number) => void
}

export function PdfPreview(props: PdfPreviewProps) {
  const [fullscreen, setFullscreen] = useState(false)
  return (
    <PreviewFullscreen
      open={fullscreen}
      onClose={() => setFullscreen(false)}
      title={props.title}
    >
      <PdfPreviewContent
        {...props}
        fullscreen={fullscreen}
        onToggleFullscreen={() => setFullscreen((value) => !value)}
      />
    </PreviewFullscreen>
  )
}

function PdfPreviewContent({
  src,
  title,
  compact,
  fullscreen,
  onToggleFullscreen,
}: PdfPreviewContentProps) {
  const { ref, visible } = usePreviewVisibility()
  const [page, setPage] = useState(1)
  const [zoom, setZoom] = useState(1)
  const pagesRef = useRef<PdfPagesHandle>(null)
  const pdf = usePdfDocument(src, visible)
  return (
    <div
      ref={ref}
      className="relative flex h-full min-h-0 flex-col bg-background"
      aria-label={title}
    >
      <PreviewToolbar
        zoom={zoom}
        onZoom={setZoom}
        compact={compact}
        isFullscreen={fullscreen}
        onFullscreen={onToggleFullscreen}
      >
        <PdfNavigation
          page={page}
          pages={pdf.document?.numPages ?? 0}
          onPage={(value) => pagesRef.current?.goToPage(value)}
        />
      </PreviewToolbar>
      <PdfPreviewBody
        pdf={pdf}
        visible={visible}
        page={page}
        zoom={zoom}
        pagesRef={pagesRef}
        onPageChange={setPage}
      />
    </div>
  )
}

function PdfPreviewBody({
  pdf,
  visible,
  page,
  zoom,
  pagesRef,
  onPageChange,
}: PdfPreviewBodyProps) {
  const messages = useTranslations("Folder.chat.workspaceFiles")
  return (
    <>
      {pdf.needsPassword && (
        <PdfPassword
          onSubmit={(password) => pdf.passwordCallback.current?.(password)}
        />
      )}
      {pdf.error ? (
        <p role="alert" className="p-4 text-sm text-destructive">
          {messages("previewError")}
        </p>
      ) : pdf.document && visible ? (
        <PdfPages
          ref={pagesRef}
          document={pdf.document}
          initialPage={page}
          zoom={zoom}
          onPageChange={onPageChange}
        />
      ) : (
        <div className="flex flex-1 items-center justify-center">
          <Loader2 className="size-4 animate-spin" />
        </div>
      )}
    </>
  )
}
