"use client"

import dynamic from "next/dynamic"
import { useState } from "react"
import { useTranslations } from "next-intl"
import { Loader2 } from "lucide-react"
import type { BinaryPreviewKind } from "@/lib/binary-preview"
import {
  usePreviewResource,
  usePreviewVisibility,
} from "./use-preview-resource"
import { MediaPreview } from "./media-preview"
import { Button } from "@/components/ui/button"

const PdfPreview = dynamic(
  () => import("./pdf-preview").then((mod) => mod.PdfPreview),
  { ssr: false }
)
const ModelPreview = dynamic(
  () => import("./model-preview").then((mod) => mod.ModelPreview),
  { ssr: false }
)

function BinaryRenderer({
  src,
  title,
  kind,
}: {
  src: string
  title: string
  kind: BinaryPreviewKind
}) {
  if (kind === "pdf") return <PdfPreview src={src} title={title} />
  if (kind === "model") return <ModelPreview src={src} title={title} />
  return <MediaPreview src={src} kind={kind} title={title} />
}

export function BinaryFilePreview(props: {
  rootPath: string
  path: string
  kind: BinaryPreviewKind
}) {
  const [attempt, setAttempt] = useState(0)
  return (
    <BinaryResourcePreview
      key={attempt}
      {...props}
      onRetry={() => setAttempt((value) => value + 1)}
    />
  )
}

function BinaryResourcePreview({
  rootPath,
  path,
  kind,
  onRetry,
}: {
  rootPath: string
  path: string
  kind: BinaryPreviewKind
  onRetry: () => void
}) {
  const { ref, visible } = usePreviewVisibility()
  const state = usePreviewResource(rootPath, path, visible)
  const t = useTranslations("Folder.chat.workspaceFiles")
  const labels = useTranslations("Folder.previewControls")
  return (
    <div ref={ref} className="h-full min-h-0">
      {state?.error ? (
        <div className="p-4">
          <p role="alert" className="mb-3 text-sm text-destructive">
            {t("previewError")}
          </p>
          <Button size="sm" variant="outline" onClick={onRetry}>
            {labels("retry")}
          </Button>
        </div>
      ) : state?.resource && visible ? (
        <BinaryRenderer
          key={state.resource.url}
          src={state.resource.url}
          title={path}
          kind={kind}
        />
      ) : (
        <div className="flex h-full items-center justify-center gap-2 text-xs text-muted-foreground">
          <Loader2 className="size-4 animate-spin" />
          {t("loadingPreview")}
        </div>
      )}
    </div>
  )
}
