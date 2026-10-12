"use client"

import { useState, type KeyboardEvent } from "react"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import { ScenarioCasePreview } from "./scenario-case-preview"
import styles from "./scenario-case-assets.module.css"

export function ScenarioCaseAssets({
  assets,
  onRetry,
}: {
  assets: ScenarioCaseAsset[]
  onRetry: () => void
}) {
  const [selected, setSelected] = useState(0)
  const asset = assets[selected] ?? assets[0]
  const steps = assets.some((item) => item.step)
  const showFiles =
    assets.length > 1 ||
    assets.some((item) => item.kind === "pdf" || item.label || item.step)
  if (!asset) return null
  return (
    <div className={styles.assets}>
      {showFiles && (
        <div
          className={steps ? styles.steps : styles.files}
          role="tablist"
          aria-label="案例成果"
        >
          {assets.map((item, index) => (
            <AssetTab
              key={index}
              item={item}
              index={index}
              steps={steps}
              selected={index === selected}
              onSelect={() => setSelected(index)}
            />
          ))}
        </div>
      )}
      <div
        className={styles.viewport}
        role="tabpanel"
        aria-label={asset.step || asset.label || asset.fileName}
      >
        <ScenarioCasePreview key={asset.url} asset={asset} onRetry={onRetry} />
      </div>
    </div>
  )
}

function AssetTab({
  item,
  index,
  steps,
  selected,
  onSelect,
}: {
  item: ScenarioCaseAsset
  index: number
  steps: boolean
  selected: boolean
  onSelect: () => void
}) {
  return (
    <>
      {steps && index > 0 && (
        <span className={styles.arrow} aria-hidden="true">
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img src="/scenario-cases/step-arrow.svg" alt="" />
        </span>
      )}
      <button
        type="button"
        role="tab"
        aria-selected={selected}
        tabIndex={selected ? 0 : -1}
        onKeyDown={navigateAssets}
        className={steps ? styles.step : styles.file}
        onClick={onSelect}
      >
        {steps ? (
          <span className={styles.number}>{index + 1}</span>
        ) : (
          // eslint-disable-next-line @next/next/no-img-element
          <img src="/scenario-cases/file.svg" alt="" />
        )}
        <span>{item.step || item.label || item.fileName}</span>
      </button>
    </>
  )
}

function navigateAssets(event: KeyboardEvent<HTMLButtonElement>) {
  const list = event.currentTarget.closest('[role="tablist"]')
  const tabs = Array.from(
    list?.querySelectorAll<HTMLButtonElement>('[role="tab"]') ?? []
  )
  const current = tabs.indexOf(event.currentTarget)
  const next =
    event.key === "ArrowRight"
      ? (current + 1) % tabs.length
      : event.key === "ArrowLeft"
        ? (current + tabs.length - 1) % tabs.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? tabs.length - 1
            : null
  if (next === null || !tabs[next]) return
  event.preventDefault()
  tabs[next].focus()
  tabs[next].click()
}
