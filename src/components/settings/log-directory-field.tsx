"use client"

import { Copy } from "lucide-react"
import { useTranslations } from "next-intl"
import { toast } from "sonner"

import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { copyTextToClipboard } from "@/lib/utils"

export function LogDirectoryField({ path }: { path: string }) {
  const t = useTranslations("LogsSettings")

  const copyPath = async () => {
    if (await copyTextToClipboard(path)) {
      toast.success(t("pathCopied"))
    } else {
      toast.error(t("copyPathFailed"))
    }
  }

  return (
    <div className="grid gap-1.5">
      <Label htmlFor="logs-directory" className="text-xs">
        {t("logsPathLabel")}
      </Label>
      <div className="flex min-w-0 gap-2">
        <Input
          id="logs-directory"
          value={path}
          readOnly
          className="min-w-0 font-mono text-xs"
        />
        <Button
          type="button"
          variant="outline"
          size="icon"
          className="h-9 w-9 shrink-0"
          aria-label={t("copyPath")}
          title={t("copyPath")}
          disabled={!path}
          onClick={() => void copyPath()}
        >
          <Copy className="h-4 w-4" />
        </Button>
      </div>
    </div>
  )
}
