"use client"

import type { ComponentProps, ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import {
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
} from "@/components/ui/context-menu"
import { cn } from "@/lib/utils"

export const MENU_CONTENT_CLASS = cn(
  "min-w-56 max-w-[calc(100vw-1rem)] rounded-lg p-1.5 ring-border/70",
  "max-h-(--radix-context-menu-content-available-height) overflow-y-auto",
  "shadow-lg shadow-black/10 dark:shadow-black/25 [&_svg]:stroke-[1.75]"
)
const ITEM_CLASS = cn(
  "min-h-8 gap-2.5 rounded-md px-2.5 py-1.5 text-[0.8125rem] leading-5",
  "data-[highlighted]:bg-accent data-[state=open]:bg-accent",
  "data-[variant=destructive]:data-[highlighted]:bg-destructive/10"
)

export function ConversationMenuItem({
  icon: Icon,
  iconClassName,
  ...props
}: ComponentProps<typeof ContextMenuItem> & {
  icon?: LucideIcon
  iconClassName?: string
}) {
  return (
    <ContextMenuItem {...props} className={cn(ITEM_CLASS, props.className)}>
      {Icon && (
        <Icon
          aria-hidden
          className={cn(
            "size-4",
            props.variant !== "destructive" && "text-muted-foreground",
            iconClassName
          )}
        />
      )}
      {props.children}
    </ContextMenuItem>
  )
}

export function ConversationMenuSub(props: {
  icon: LucideIcon
  label: string
  children: ReactNode
  value?: string
  disabled?: boolean
}) {
  const Icon = props.icon
  return (
    <ContextMenuSub>
      <ContextMenuSubTrigger
        disabled={props.disabled}
        className={cn(
          ITEM_CLASS,
          "[&>svg:last-child]:size-3.5 [&>svg:last-child]:text-muted-foreground",
          props.value && "[&>svg:last-child]:ml-0"
        )}
      >
        <Icon aria-hidden className="size-4 text-muted-foreground" />
        {props.label}
        {props.value && (
          <span className="ml-auto text-xs text-muted-foreground">
            {props.value}
          </span>
        )}
      </ContextMenuSubTrigger>
      <ContextMenuSubContent className={cn(MENU_CONTENT_CLASS, "min-w-44")}>
        {props.children}
      </ContextMenuSubContent>
    </ContextMenuSub>
  )
}

export function ConversationMenuSeparator() {
  return <ContextMenuSeparator className="mx-2 my-1.5 bg-border/60" />
}
