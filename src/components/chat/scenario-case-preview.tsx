"use client"

import { useState } from "react"
import { PdfPreview } from "@/components/files/pdf-preview"
import { MediaPreview } from "@/components/files/media-preview"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import styles from "./scenario-cases.module.css"

export function ScenarioCasePreview({
  asset,
  onRetry,
}: {
  asset: ScenarioCaseAsset
  onRetry: () => void
}) {
  const [failed, setFailed] = useState(false)
  if (failed) return <CaseLoadError onRetry={onRetry} />
  if (asset.kind === "pdf") {
    return (
      <div className={styles.documentPreview}>
        <PdfPreview src={asset.url} title={asset.fileName} />
      </div>
    )
  }
  if (asset.kind === "video") {
    return (
      <div className={styles.documentPreview}>
        <MediaPreview src={asset.url} kind="video" title={asset.fileName} />
      </div>
    )
  }
  return (
    // eslint-disable-next-line @next/next/no-img-element
    <img
      src={asset.url}
      alt={asset.fileName}
      className={styles.previewImage}
      loading="lazy"
      onError={() => setFailed(true)}
    />
  )
}

export function CaseLoadError({ onRetry }: { onRetry: () => void }) {
  return (
    <div className={styles.loadState} role="alert">
      <p>案例暂时无法读取，请重试。</p>
      <button type="button" className={styles.viewButton} onClick={onRetry}>
        重新加载
      </button>
    </div>
  )
}
