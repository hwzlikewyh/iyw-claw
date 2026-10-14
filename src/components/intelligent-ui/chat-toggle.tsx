"use client"

import { useState } from "react"
import { LayoutDashboard, Loader2 } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"
import { Button } from "@/components/ui/button"
import {
  Tooltip,
  TooltipContent,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import {
  reloadUiSettings,
  updateUiSettings,
  useUiSettings,
} from "@/lib/intelligent-ui/settings"
import { toErrorMessage } from "@/lib/app-error"
import { cn } from "@/lib/utils"

export function IntelligentUiChatToggle() {
  const t = useTranslations("IntelligentUI")
  const settings = useUiSettings()
  const { saving, change } = useToggleAction(settings)
  const label = settings.error
    ? t("loadFailed")
    : t(settings.enabled ? "disable" : "enabled")
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="xs"
          role="switch"
          aria-checked={settings.enabled}
          aria-label={label}
          disabled={!settings.loaded || saving}
          onClick={() => void change()}
          className={cn(
            "shrink-0 gap-1 px-1.5",
            settings.enabled
              ? "bg-primary/10 text-primary hover:bg-primary/15"
              : "text-muted-foreground",
            settings.error && "text-destructive"
          )}
        >
          {saving ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <LayoutDashboard className="size-3.5" />
          )}
          <span className="hidden @[30rem]:inline">{t("chatLabel")}</span>
        </Button>
      </TooltipTrigger>
      <TooltipContent side="top">
        <p>{label}</p>
        <p>{t("enabledDescription")}</p>
      </TooltipContent>
    </Tooltip>
  )
}

function useToggleAction(settings: ReturnType<typeof useUiSettings>) {
  const t = useTranslations("IntelligentUI")
  const [saving, setSaving] = useState(false)
  async function change() {
    if (saving) return
    setSaving(true)
    try {
      if (settings.error) await reloadUiSettings()
      else await updateUiSettings(!settings.enabled)
    } catch (error) {
      toast.error(t("saveFailed"), { description: toErrorMessage(error) })
    } finally {
      setSaving(false)
    }
  }
  return { saving, change }
}
