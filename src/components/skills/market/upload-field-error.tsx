import type { SkillMarketTranslator } from "@/lib/skill-market"
import type { SkillMarketUploadFieldError } from "@/components/skills/skill-market-upload-validation"

export function UploadFieldError({
  t,
  error,
}: {
  t: SkillMarketTranslator
  error?: SkillMarketUploadFieldError
}) {
  if (!error) return null
  const key =
    error.code === "dependencies" && error.line
      ? t("upload.errors.dependenciesLine", { line: error.line })
      : t(`upload.errors.${error.code}`)
  return <p className="text-xs text-destructive">{key}</p>
}
