import { forwardRef, type ReactNode } from "react"
import { PanelLeft, PanelRight, type LucideIcon } from "lucide-react"

import { cn } from "@/lib/utils"

interface SidebarToggleButtonProps {
  isOpen: boolean
  label: string
  onClick: () => void
  className?: string
}

export const SidebarToggleButton = forwardRef<
  HTMLButtonElement,
  SidebarToggleButtonProps
>(function SidebarToggleButton({ isOpen, label, onClick, className }, ref) {
  const Icon = isOpen ? PanelLeft : PanelRight

  return (
    <button
      ref={ref}
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-expanded={isOpen}
      className={cn(
        "flex h-7 w-7 items-center justify-center rounded-md outline-none",
        "text-muted-foreground transition-[background-color,color] duration-150",
        "hover:bg-sidebar-accent hover:text-sidebar-foreground",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
        className
      )}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
    </button>
  )
})

interface SidebarNavButtonProps {
  icon: LucideIcon
  label: string
  onClick: () => void
  active?: boolean
  trailing?: ReactNode
  tone?: "default" | "primary"
  className?: string
}

export function SidebarNavButton({
  icon: Icon,
  label,
  onClick,
  active,
  trailing,
  tone = "default",
  className,
}: SidebarNavButtonProps) {
  const isPrimary = tone === "primary"

  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-current={active ? "page" : undefined}
      className={cn(
        "group relative flex w-full items-center gap-2.5 rounded-md px-3",
        "text-[0.8125rem] font-medium outline-none",
        "transition-[background-color,color,box-shadow] duration-150",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
        isPrimary
          ? "h-10 bg-primary text-primary-foreground shadow-sm shadow-primary/15 hover:bg-primary/90"
          : "h-9 text-sidebar-foreground/70 hover:bg-sidebar-accent/70 hover:text-sidebar-foreground",
        active &&
          !isPrimary &&
          "bg-primary/[0.08] text-sidebar-foreground before:absolute before:top-2 before:bottom-2 before:left-0 before:w-0.5 before:bg-primary",
        className
      )}
    >
      <Icon
        className={cn(
          "h-[0.875rem] w-[0.875rem] shrink-0",
          isPrimary ? "text-primary-foreground" : "text-muted-foreground",
          active && !isPrimary && "text-primary"
        )}
      />
      <span className={cn("truncate", isPrimary && "font-medium")}>
        {label}
      </span>
      {trailing}
    </button>
  )
}

interface SidebarRailButtonProps {
  icon: LucideIcon
  label: string
  onClick: () => void
  active?: boolean
  tone?: "default" | "primary"
}

export function SidebarRailButton({
  icon: Icon,
  label,
  onClick,
  active,
  tone = "default",
}: SidebarRailButtonProps) {
  const isPrimary = tone === "primary"

  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex h-9 w-9 items-center justify-center rounded-md outline-none",
        "transition-[background-color,color,box-shadow] duration-150",
        "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
        isPrimary
          ? "bg-primary text-primary-foreground shadow-sm shadow-primary/15 hover:bg-primary/90"
          : "text-muted-foreground hover:bg-sidebar-accent hover:text-sidebar-foreground",
        active && !isPrimary && "bg-primary/10 text-primary"
      )}
    >
      <Icon className="h-3.5 w-3.5" aria-hidden="true" />
    </button>
  )
}
