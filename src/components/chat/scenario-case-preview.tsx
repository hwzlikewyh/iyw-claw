"use client"

import { useState } from "react"
import { PdfPreview } from "@/components/files/pdf-preview"
import { MediaPreview } from "@/components/files/media-preview"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import styles from "./scenario-cases.module.css"
import { ScenarioCaseMarkdown } from "./scenario-case-markdown"
import { CaseLoadError } from "./scenario-case-error"
import { ScenarioCaseSource } from "./scenario-case-source"

export function ScenarioCasePreview({
  asset,
  onRetry,
}: {
  asset: ScenarioCaseAsset
  onRetry: () => void
}) {
  const [failed, setFailed] = useState(false)
  if (failed)
    return (
      <>
        <CaseLoadError onRetry={onRetry} />
        <ScenarioCaseSource asset={asset} />
      </>
    )
  if (asset.kind === "markdown")
    return <ScenarioCaseMarkdown asset={asset} onRetry={onRetry} />
  if (asset.kind === "file") return <ScenarioCaseSource asset={asset} />
  if (asset.kind !== "image") {
    return (
      <>
        <div className={styles.documentPreview}>
          <CaseDocument asset={asset} onRetry={onRetry} />
        </div>
        <ScenarioCaseSource asset={asset} />
      </>
    )
  }
  return <CaseImage asset={asset} onError={() => setFailed(true)} />
}

function CaseDocument({
  asset,
  onRetry,
}: {
  asset: ScenarioCaseAsset
  onRetry: () => void
}) {
  if (asset.kind === "text")
    return <ScenarioCaseMarkdown asset={asset} onRetry={onRetry} />
  if (asset.kind === "pdf")
    return <PdfPreview src={asset.url} title={asset.fileName} compact />
  if (asset.kind === "audio" || asset.kind === "video")
    return (
      <MediaPreview src={asset.url} kind={asset.kind} title={asset.fileName} />
    )
  return null
}

function CaseImage({
  asset,
  onError,
}: {
  asset: ScenarioCaseAsset
  onError: () => void
}) {
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={asset.url}
      alt={asset.fileName}
      className={styles.previewImage}
      loading="lazy"
      onError={() => {
        console.warn("[scenario-case] image preview failed", {
          kind: asset.kind,
        })
        onError()
      }}
    />
  )
}
