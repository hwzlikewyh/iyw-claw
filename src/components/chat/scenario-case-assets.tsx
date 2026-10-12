"use client"

import { useState, type KeyboardEvent } from "react"
import type { ScenarioCaseAsset } from "@/lib/scenario-cases"
import { ScenarioCasePreview } from "./scenario-case-preview"
import styles from "./scenario-case-assets.module.css"

interface AssetChoice {
  name: string
  assets: ScenarioCaseAsset[]
}

function groupAssets(assets: ScenarioCaseAsset[], field: "step" | "label") {
  const result: AssetChoice[] = []
  for (const asset of assets) {
    const name = asset[field] || asset.fileName
    const group = result.find((item) => item.name === name)
    if (group) group.assets.push(asset)
    else result.push({ name, assets: [asset] })
  }
  return result
}

function shouldShowFiles(
  files: AssetChoice[],
  assets: ScenarioCaseAsset[],
  hasSteps: boolean
) {
  return (
    files.length > 1 ||
    (!hasSteps &&
      assets.some(
        (item) => item.label || item.kind === "pdf" || item.kind === "markdown"
      ))
  )
}

export function ScenarioCaseAssets({
  assets,
  onRetry,
}: {
  assets: ScenarioCaseAsset[]
  onRetry: () => void
}) {
  const [stepIndex, setStepIndex] = useState(0)
  const [fileIndex, setFileIndex] = useState(0)
  const hasSteps = assets.some((item) => item.step)
  const steps = hasSteps
    ? groupAssets(assets, "step")
    : [{ name: "案例成果", assets }]
  const step = steps[stepIndex] ?? steps[0]
  const files = groupAssets(step?.assets ?? [], "label")
  const file = files[fileIndex] ?? files[0]
  const showFiles = shouldShowFiles(files, assets, hasSteps)
  if (!file) return null
  return (
    <div className={styles.assets}>
      {hasSteps && (
        <AssetChoices
          choices={steps}
          steps
          selected={stepIndex}
          onSelect={(index) => {
            setStepIndex(index)
            setFileIndex(0)
          }}
        />
      )}
      {showFiles && (
        <AssetChoices
          choices={files}
          selected={fileIndex}
          onSelect={setFileIndex}
        />
      )}
      <div className={styles.viewport} role="tabpanel" aria-label={file.name}>
        {file.assets.map((asset) => (
          <ScenarioCasePreview
            key={asset.url}
            asset={asset}
            onRetry={onRetry}
          />
        ))}
      </div>
    </div>
  )
}

function AssetChoices({
  choices,
  steps = false,
  selected,
  onSelect,
}: {
  choices: AssetChoice[]
  steps?: boolean
  selected: number
  onSelect: (index: number) => void
}) {
  return (
    <div
      className={steps ? styles.steps : styles.files}
      role="tablist"
      aria-label={steps ? "案例步骤" : "案例成果"}
    >
      {choices.map((choice, index) => (
        <AssetTab
          key={choice.name}
          choice={choice}
          index={index}
          steps={steps}
          selected={index === selected}
          onSelect={() => onSelect(index)}
        />
      ))}
    </div>
  )
}

function AssetTab({
  choice,
  index,
  steps,
  selected,
  onSelect,
}: {
  choice: AssetChoice
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
          <img
            src={
              choice.assets[0]?.kind === "markdown"
                ? "/scenario-cases/markdown-file.svg"
                : "/scenario-cases/file.svg"
            }
            alt=""
          />
        )}
        <span>{choice.name}</span>
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
