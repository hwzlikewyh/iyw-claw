"use client"
import type { ComponentProps, ReactNode } from "react"
import type { LucideIcon } from "lucide-react"
import { AlertCircle, Loader2 } from "lucide-react"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { cn } from "@/lib/utils"
type HeadingProps = {
  icon?: LucideIcon
  title: ReactNode
  description?: ReactNode
  control?: ReactNode
  htmlFor?: string
  children?: ReactNode
  className?: string
}
export function SettingCard({ className, ...props }: ComponentProps<"div">) {
  return (
    <div
      className={cn("divide-y rounded-lg border bg-muted/30", className)}
      {...props}
    />
  )
}
function SettingHeading({
  icon: Icon,
  title,
  description,
  control,
  htmlFor,
}: HeadingProps) {
  return (
    <div className="flex items-start justify-between gap-3">
      <div className="min-w-0 space-y-1">
        <Label htmlFor={htmlFor} className="text-sm">
          {Icon && <Icon className="size-4 shrink-0" aria-hidden="true" />}
          {title}
        </Label>
        {description && (
          <p className="text-xs leading-5 text-muted-foreground">
            {description}
          </p>
        )}
      </div>
      {control && <div className="shrink-0">{control}</div>}
    </div>
  )
}
export function SettingRow({ children, className, ...heading }: HeadingProps) {
  return (
    <div className={cn("space-y-2 p-3", className)}>
      <SettingHeading {...heading} />
      {children}
    </div>
  )
}
export function SettingsSection({
  children,
  className,
  ...heading
}: HeadingProps) {
  return (
    <section
      className={cn("space-y-3 rounded-lg border bg-card p-4", className)}
    >
      <SettingHeading {...heading} />
      {children}
    </section>
  )
}
export function SettingNote({
  icon: Icon,
  children,
  className,
}: {
  icon?: LucideIcon
  children: ReactNode
  className?: string
}) {
  return (
    <div
      className={cn(
        "flex gap-2 rounded-lg border bg-muted/30 p-3 text-xs leading-5",
        className
      )}
    >
      {Icon && <Icon className="size-4 shrink-0" aria-hidden="true" />}
      <div>{children}</div>
    </div>
  )
}
export function SettingsError({
  children,
  className,
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <div
      role="alert"
      className={cn(
        "flex gap-2 rounded-lg border border-destructive/30 p-3 text-xs text-destructive",
        className
      )}
    >
      <AlertCircle className="size-4 shrink-0" aria-hidden="true" />
      <span>{children}</span>
    </div>
  )
}
export function SettingsSaveBar({
  onSave,
  saving,
  disabled,
  label,
  savingLabel,
  className,
}: {
  onSave: () => void
  saving: boolean
  disabled?: boolean
  label: ReactNode
  savingLabel: ReactNode
  className?: string
}) {
  return (
    <div className={cn("flex justify-end", className)}>
      <Button
        size="sm"
        type="button"
        onClick={onSave}
        disabled={disabled || saving}
      >
        {saving && (
          <Loader2 className="size-4 animate-spin" aria-hidden="true" />
        )}
        {saving ? savingLabel : label}
      </Button>
    </div>
  )
}
