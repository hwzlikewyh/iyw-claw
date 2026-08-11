"use client"

import { PackageOpen } from "lucide-react"
import { useTranslations } from "next-intl"

import { cn } from "@/lib/utils"

export function EmptyTaskArtifactPreview({
  className,
}: {
  className?: string
}) {
  const t = useTranslations("Folder.taskArtifacts")
  return (
    <div
      className={cn(
        "flex min-h-0 flex-1 flex-col items-center justify-center gap-2 px-6 text-center text-muted-foreground",
        className
      )}
    >
      <PackageOpen className="size-6" />
      <p className="text-sm">{t("selectArtifact")}</p>
    </div>
  )
}
