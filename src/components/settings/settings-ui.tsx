/**
 * Shared layout primitives for settings pages.
 *
 * Usage pattern:
 *   <SettingsPageLayout>
 *     <SettingsPageHeader icon={Palette} title="Appearance" description="..." />
 *     <SettingSection icon={Sun} title="Theme" description="...">
 *       <SettingRow title="Mode" description="...">
 *         <Switch />
 *       </SettingRow>
 *       <SettingRow title="Color">
 *         <Select />
 *       </SettingRow>
 *     </SettingSection>
 *     <SettingSection title="Advanced">
 *       <SettingSectionBody>
 *         free-form content
 *       </SettingSectionBody>
 *     </SettingSection>
 *   </SettingsPageLayout>
 */

import type { ComponentType, ReactNode } from "react"
import { ScrollArea } from "@/components/ui/scroll-area"
import { cn } from "@/lib/utils"

// ─── Page-level layout ───────────────────────────────────────────────────────

/** Scroll container with a centred, max-width-constrained content column. */
export function SettingsPageLayout({ children }: { children: ReactNode }) {
  return (
    <ScrollArea className="h-full">
      <div className="mx-auto w-full max-w-2xl space-y-5 px-5 py-5">
        {children}
      </div>
    </ScrollArea>
  )
}

/** Page-level heading block placed at the top of each settings page. */
export function SettingsPageHeader({
  icon: Icon,
  title,
  description,
  action,
}: {
  icon?: ComponentType<{ className?: string }>
  title: string
  description?: string
  /** Optional right-side element (e.g. a "Refresh" button). */
  action?: ReactNode
}) {
  return (
    <div className="flex items-start justify-between gap-4 pb-1">
      <div className="min-w-0">
        <div className="flex items-center gap-2.5">
          {Icon && (
            <div className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md bg-muted">
              <Icon className="h-4 w-4 text-muted-foreground" />
            </div>
          )}
          <h1 className="text-base font-semibold tracking-tight">{title}</h1>
        </div>
        {description && (
          <p className="mt-1.5 text-sm leading-relaxed text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {action && <div className="shrink-0">{action}</div>}
    </div>
  )
}

// ─── Section ─────────────────────────────────────────────────────────────────

/**
 * Card section with an icon-and-title header.
 *
 * For row-based content, put `<SettingRow>` children directly — they
 * contribute their own `px-4 py-3` padding and a bottom divider.
 *
 * For free-form content (inputs, buttons, etc.), wrap in `<SettingSectionBody>`.
 */
export function SettingSection({
  icon: Icon,
  title,
  description,
  children,
  variant = "default",
}: {
  icon?: ComponentType<{ className?: string }>
  title: string
  description?: string
  children: ReactNode
  variant?: "default" | "destructive"
}) {
  return (
    <section
      className={cn(
        "overflow-hidden rounded-xl border bg-card",
        variant === "destructive" &&
          "border-destructive/30 [border-left-width:3px] border-l-destructive"
      )}
    >
      <div className="border-b px-4 py-3">
        <div className="flex items-center gap-2">
          {Icon && <Icon className="h-4 w-4 text-muted-foreground" />}
          <h2 className="text-sm font-semibold">{title}</h2>
        </div>
        {description && (
          <p className="mt-1 text-xs leading-5 text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      <div className="divide-y divide-border/60">{children}</div>
    </section>
  )
}

// ─── Row ─────────────────────────────────────────────────────────────────────

/**
 * A single setting row: label + optional description on the left,
 * control on the right.
 */
export function SettingRow({
  title,
  description,
  children,
  className,
}: {
  title: string
  description?: string
  children?: ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "flex items-center justify-between gap-4 px-4 py-3",
        className
      )}
    >
      <div className="min-w-0 flex-1">
        <div className="text-sm font-medium leading-5">{title}</div>
        {description && (
          <p className="mt-0.5 text-xs leading-5 text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {children && <div className="shrink-0">{children}</div>}
    </div>
  )
}

// ─── Free-form section body ───────────────────────────────────────────────────

/**
 * Padded wrapper for non-row content inside a SettingSection.
 * Provides the same visual padding as a SettingRow.
 */
export function SettingSectionBody({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return <div className={cn("space-y-4 p-4", className)}>{children}</div>
}
