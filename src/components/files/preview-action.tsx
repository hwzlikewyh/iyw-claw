import type { ComponentProps } from "react"
import { Button } from "@/components/ui/button"

export function PreviewAction({
  label,
  children,
  ...props
}: ComponentProps<typeof Button> & { label: string }) {
  return (
    <Button
      {...props}
      variant="ghost"
      size="icon-sm"
      title={label}
      aria-label={label}
    >
      {children}
    </Button>
  )
}
