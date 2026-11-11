import { AppWindow } from "lucide-react"
import { cn } from "@/lib/utils"

export function PluginContextBadge({
  label,
  className,
  onClick,
}: {
  label: string
  className?: string
  onClick?: () => void
}) {
  const content = (
    <>
      <AppWindow className="size-3.5 shrink-0 text-violet-600 dark:text-violet-300" />
      <span className="truncate">{label}</span>
    </>
  )
  const classes = cn(
    "inline-flex max-w-full items-center gap-1.5 rounded-md border border-violet-500/25 bg-violet-500/10 px-2 py-1 text-xs font-medium text-violet-700 dark:text-violet-300",
    onClick &&
      "cursor-pointer transition-colors hover:border-violet-500/40 hover:bg-violet-500/15 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-violet-500/50",
    className
  )

  if (onClick) {
    return (
      <button
        type="button"
        data-plugin-context-badge
        title={label}
        aria-label={label}
        className={classes}
        onClick={onClick}
      >
        {content}
      </button>
    )
  }

  return (
    <span data-plugin-context-badge title={label} className={classes}>
      {content}
    </span>
  )
}
