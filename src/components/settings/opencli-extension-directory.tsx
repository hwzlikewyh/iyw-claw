import { useState } from "react"
import { Check, Copy, FolderOpen } from "lucide-react"
import { useTranslations } from "next-intl"
import { Button } from "@/components/ui/button"
import { openPath } from "@/lib/platform"
import { toErrorMessage } from "@/lib/app-error"

function useDirectoryActions(directory: string) {
  const [copied, setCopied] = useState(false)
  const [error, setError] = useState("")
  const act = async (operation: () => Promise<void>) => {
    setError("")
    try {
      await operation()
    } catch (cause) {
      setError(toErrorMessage(cause))
    }
  }
  const copy = async () => {
    await navigator.clipboard.writeText(directory)
    setCopied(true)
  }
  return { copied, error, act, copy }
}

export function OpencliExtensionDirectory({
  directory,
}: {
  directory: string
}) {
  const t = useTranslations("BrowserSetup")
  const { copied, error, act, copy } = useDirectoryActions(directory)
  return (
    <div className="space-y-2">
      <div className="flex min-w-0 items-start gap-2">
        <code className="min-w-0 flex-1 select-all break-all rounded border bg-muted/30 p-2 text-xs">
          {directory}
        </code>
        <Button
          variant="outline"
          size="icon"
          className="size-8 shrink-0"
          title={t("copyPath")}
          aria-label={t("copyPath")}
          onClick={() => void act(copy)}
        >
          {copied ? (
            <Check className="size-3.5" />
          ) : (
            <Copy className="size-3.5" />
          )}
        </Button>
        <Button
          variant="outline"
          size="icon"
          className="size-8 shrink-0"
          title={t("openFolder")}
          aria-label={t("openFolder")}
          onClick={() => void act(() => openPath(directory))}
        >
          <FolderOpen className="size-3.5" />
        </Button>
      </div>
      {error && (
        <p role="alert" className="break-words text-xs text-destructive">
          {error}
        </p>
      )}
    </div>
  )
}
