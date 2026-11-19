"use client"

import { X } from "lucide-react"
import { MarketItemIcon } from "@/components/skills/market/market-item-icon"
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from "@/components/ui/tooltip"
import { cn } from "@/lib/utils"

type SourceKind = "plugin" | "skill" | "connector"

export function ComposerSourceIcon(props: {
  kind: SourceKind
  label: string
  src?: string | null
}) {
  return (
    <MarketItemIcon
      kind={props.kind}
      name={props.label}
      src={props.src}
      className="size-4 rounded-sm [&_img]:object-contain"
    />
  )
}

export function ComposerSourceChip({
  kind,
  label,
  src,
  onOpen,
  onRemove,
  removeLabel,
  className,
}: {
  kind: SourceKind
  label: string
  src?: string | null
  className?: string
  onOpen?: () => void
  onRemove?: () => void
  removeLabel?: string
}) {
  const Trigger = onOpen ? "button" : "span"
  return (
    <span
      data-composer-source={kind}
      data-plugin-context-badge={kind === "plugin" ? "" : undefined}
      className={cn(
        "inline-flex h-7 max-w-full min-w-0 items-center rounded-md border border-border/70 bg-muted/45 align-middle text-xs font-medium text-foreground/90",
        className
      )}
    >
      <TooltipProvider delayDuration={250}>
        <Tooltip>
          <TooltipTrigger asChild>
            <Trigger
              type={onOpen ? "button" : undefined}
              tabIndex={0}
              aria-label={label}
              onClick={onOpen}
              className={cn(
                "inline-flex h-full min-w-0 items-center gap-1.5 rounded px-2 focus-visible:outline-2 focus-visible:outline-ring",
                onOpen && "hover:bg-muted"
              )}
            >
              <ComposerSourceIcon kind={kind} label={label} src={src} />
              <span className="max-w-24 truncate">{label}</span>
            </Trigger>
          </TooltipTrigger>
          <TooltipContent
            side="top"
            sideOffset={4}
            className="max-w-[min(18rem,90vw)] break-all rounded-md"
          >
            {label}
          </TooltipContent>
        </Tooltip>
      </TooltipProvider>
      {onRemove && (
        <button
          type="button"
          aria-label={removeLabel}
          title={removeLabel}
          onClick={onRemove}
          className="mr-0.5 grid size-6 shrink-0 place-items-center rounded text-muted-foreground hover:bg-muted hover:text-foreground focus-visible:outline-2 focus-visible:outline-ring"
        >
          <X className="size-3" />
        </button>
      )}
    </span>
  )
}
