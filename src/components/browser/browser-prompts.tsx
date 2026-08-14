"use client"

import { useEffect, useRef, useState } from "react"
import { useTranslations } from "next-intl"
import { useBrowser } from "@/contexts/browser-context"
import { browserApi } from "@/lib/browser-api"
import type {
  BrowserDialogSnapshot,
  BrowserFileChooserSnapshot,
} from "@/lib/browser-types"
import { openFileDialog } from "@/lib/platform"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"

export function BrowserPrompts({
  dialog,
  chooser,
}: {
  dialog?: BrowserDialogSnapshot
  chooser?: BrowserFileChooserSnapshot
}) {
  const t = useTranslations("Browser")
  const { run } = useBrowser()
  const [prompt, setPrompt] = useState(dialog?.defaultPrompt ?? "")
  const choosingRef = useRef<string | null>(null)

  useEffect(() => {
    if (!chooser || choosingRef.current === chooser.chooserId) return
    choosingRef.current = chooser.chooserId
    void (async () => {
      let selection: string | string[] | null = null
      try {
        selection = await openFileDialog({
          multiple: chooser.mode === "select_multiple",
          title: t("chooseFiles"),
        })
      } catch {
        selection = null
      }
      try {
        const paths = !selection
          ? []
          : Array.isArray(selection)
            ? selection
            : [selection]
        await run(() =>
          browserApi.chooseFiles(chooser.chooserId, chooser.generations, paths)
        )
      } catch {}
    })().finally(() => {
      choosingRef.current = null
    })
  }, [chooser, run, t])

  const answer = (accept: boolean) => {
    if (!dialog) return
    void run(() =>
      browserApi.answerDialog(
        dialog.dialogId,
        dialog.generations,
        accept,
        dialog.kind === "prompt" ? prompt : undefined
      )
    ).catch(() => {})
  }

  return (
    <Dialog open={Boolean(dialog)}>
      <DialogContent
        className="max-w-md"
        onEscapeKeyDown={(event) => {
          event.preventDefault()
          answer(false)
        }}
        onPointerDownOutside={(event) => event.preventDefault()}
      >
        <DialogHeader>
          <DialogTitle>{t(`dialog.${dialog?.kind ?? "alert"}`)}</DialogTitle>
          <DialogDescription className="whitespace-pre-wrap break-words">
            {dialog?.message}
          </DialogDescription>
        </DialogHeader>
        {dialog?.kind === "prompt" ? (
          <Input
            value={prompt}
            onChange={(event) => setPrompt(event.target.value)}
            autoFocus
          />
        ) : null}
        <DialogFooter>
          {dialog?.kind !== "alert" ? (
            <Button variant="outline" onClick={() => answer(false)}>
              {t("cancel")}
            </Button>
          ) : null}
          <Button onClick={() => answer(true)}>{t("confirm")}</Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  )
}
