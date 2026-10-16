"use client"

import { useId, useState } from "react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { ControlField } from "./control-field"
import { Button } from "@/components/ui/button"
import { copyTextToClipboard } from "@/lib/utils"
import { toErrorMessage } from "@/lib/app-error"
import {
  readAction,
  type ResolvedNode,
} from "@/lib/intelligent-ui/engine/block"
import { useUiRender, text } from "./context"
import { Ask } from "./ask"

export { Ask } from "./ask"

export function Control({ node }: { node: ResolvedNode }) {
  const { state, stateNames } = useUiRender()
  const t = useTranslations("IntelligentUI")
  const p = node.props,
    bind = node.bind ?? "",
    value = state[bind]
  const id = useId()
  if (node.component === "button") return <ActionButton node={node} />
  if (!bind || !stateNames.has(bind))
    return (
      <p role="status" className="text-xs text-destructive">
        {t("invalidBinding", {
          name: text(p.label ?? (bind || node.component)),
        })}
      </p>
    )
  return (
    <div className="min-w-0 space-y-1.5">
      <label
        htmlFor={id}
        className="flex items-center justify-between gap-2 text-xs text-muted-foreground"
      >
        <span>{text(p.label ?? bind)}</span>
        {node.component === "slider" && (
          <span>
            {text(value)} {text(p.unit ?? "")}
          </span>
        )}
      </label>
      <ControlField node={node} value={value} id={id} />
      <span className="sr-only">{t("input")}</span>
    </div>
  )
}

function ActionButton({ node }: { node: ResolvedNode }) {
  const { stateNames, update } = useUiRender()
  const t = useTranslations("IntelligentUI")
  const action = readAction(node.props, stateNames)
  if (action?.kind === "ask") return <Ask suggestion={action.text} />
  if (action?.kind === "copy")
    return (
      <CopyButton
        value={action.text}
        label={text(node.props.label ?? t("copy"))}
      />
    )
  return (
    <div className="space-y-1.5">
      <Button
        type="button"
        size="sm"
        variant="outline"
        disabled={!action}
        onClick={() => {
          if (action?.kind === "set") update(action.patch)
        }}
      >
        {text(node.props.label ?? t("copy"))}
      </Button>
      {!action && (
        <p role="status" className="text-xs text-muted-foreground">
          {t("actionUnavailable")}
        </p>
      )}
    </div>
  )
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const t = useTranslations("Folder.conversation")
  const [copying, setCopying] = useState(false)
  async function copy() {
    if (copying) return
    setCopying(true)
    try {
      if (await copyTextToClipboard(value)) toast.success(t("copyTextSuccess"))
      else toast.error(t("copyTextFailed"))
    } catch (error) {
      console.error("[intelligent-ui] copy failed", {
        error: toErrorMessage(error),
      })
      toast.error(t("copyTextFailed"), { description: toErrorMessage(error) })
    } finally {
      setCopying(false)
    }
  }
  return (
    <Button
      type="button"
      size="sm"
      variant="outline"
      disabled={copying}
      aria-busy={copying}
      onClick={() => void copy()}
    >
      {label}
    </Button>
  )
}
