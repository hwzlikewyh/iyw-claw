"use client"

import { Dialog as DialogPrimitive } from "radix-ui"
import { ArrowLeft } from "lucide-react"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogOverlay,
  DialogPortal,
  DialogTitle,
  DialogTrigger,
} from "@/components/ui/dialog"
import {
  useCaseController,
  type CaseController,
  type ScenarioCasesProps,
} from "./scenario-case-state"
import { CaseEntryThumbnails, ScenarioCaseCover } from "./scenario-case-cover"
import { CaseLoadError } from "./scenario-case-error"
import { ScenarioCaseAssets } from "./scenario-case-assets"
import styles from "./scenario-cases.module.css"

export function ScenarioCases(props: ScenarioCasesProps) {
  const model = useCaseController(props)
  if (!props.category || model.items.length === 0) return null
  return (
    <Dialog open={model.open} onOpenChange={model.changeOpen}>
      <DialogTrigger asChild>
        <button type="button" className={styles.entry}>
          <CaseEntryThumbnails />
          <span>查看相关案例</span>
          {/* eslint-disable-next-line @next/next/no-img-element */}
          <img
            src="/scenario-cases/chevron.svg"
            alt=""
            className={styles.chevron}
          />
        </button>
      </DialogTrigger>
      <CaseDialog model={model} />
    </Dialog>
  )
}

function CaseDialog({ model }: { model: CaseController }) {
  return (
    <DialogPortal>
      <DialogOverlay className="bg-black/30 supports-backdrop-filter:backdrop-blur-none" />
      <DialogPrimitive.Content
        className={styles.dialog}
        aria-describedby={
          model.selected?.summary || model.detail?.summary
            ? "scenario-case-summary"
            : undefined
        }
        onCloseAutoFocus={model.closeAutoFocus}
      >
        <DialogClose asChild>
          <button type="button" className={styles.close} aria-label="关闭案例">
            {/* eslint-disable-next-line @next/next/no-img-element */}
            <img src="/scenario-cases/close.svg" alt="" />
          </button>
        </DialogClose>
        <CaseList model={model} hidden={Boolean(model.selected)} />
        {model.selected && <CaseDetailContent model={model} />}
        {model.selected && <MakeSameButton model={model} />}
      </DialogPrimitive.Content>
    </DialogPortal>
  )
}

function CaseList({
  model,
  hidden,
}: {
  model: CaseController
  hidden: boolean
}) {
  return (
    <div className={hidden ? "hidden" : "contents"}>
      {!hidden && (
        <DialogTitle className={styles.listTitle}>{model.title}</DialogTitle>
      )}
      <div className={styles.grid}>
        {model.items.map((item) => (
          <ScenarioCaseCover
            key={item.id}
            item={item}
            onSelect={() => model.select(item)}
          />
        ))}
      </div>
    </div>
  )
}

function MakeSameButton({ model }: { model: CaseController }) {
  return (
    <button
      type="button"
      className={styles.makeSame}
      disabled={!model.detail}
      onClick={model.makeSame}
    >
      {/* eslint-disable-next-line @next/next/no-img-element */}
      <img src="/scenario-cases/copy.svg" alt="" />
      一键做同款
    </button>
  )
}

function CaseDetailContent({ model }: { model: CaseController }) {
  const item = model.detail ?? model.selected
  if (!item) return null
  return (
    <>
      <CaseDetailHeader model={model} />
      <div
        className={styles.preview}
        data-selectors={Boolean(
          model.detail?.assets.some(
            (asset) => asset.label || asset.step || asset.kind === "pdf"
          ) || (model.detail?.assets.length ?? 0) > 1
        )}
      >
        {model.failed ? (
          <CaseLoadError onRetry={model.retry} phase="detail" />
        ) : model.detail ? (
          <ScenarioCaseAssets
            key={model.detail.id}
            assets={model.detail.assets}
            onRetry={model.retry}
          />
        ) : (
          <p className={styles.loadState} role="status">
            正在加载案例…
          </p>
        )}
      </div>
    </>
  )
}

function CaseDetailHeader({ model }: { model: CaseController }) {
  const item = model.detail ?? model.selected
  if (!item) return null
  return (
    <header
      className={styles.detailHeader}
      data-summary={Boolean(item.summary)}
    >
      <div className="flex items-start justify-between gap-4">
        <DialogTitle className={styles.detailTitle}>{item.title}</DialogTitle>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="-mt-1 h-8 shrink-0 gap-1 px-2 text-xs text-[#2656f6]"
          onClick={model.back}
        >
          <ArrowLeft className="size-3" />
          返回案例列表
        </Button>
      </div>
      {item.summary && (
        <p id="scenario-case-summary" className={styles.summary}>
          {item.summary}
        </p>
      )}
      <span className={styles.detailTag}>{item.tag}</span>
    </header>
  )
}
