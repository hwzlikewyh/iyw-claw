import type {
  SelectedSkillMarketFolder,
  SkillMarketAudience,
  SkillMarketCategory,
  SkillMarketTranslator,
} from "@/lib/skill-market"
import type {
  SkillMarketAddVersionRequestV2,
  SkillMarketPublishRequestV2,
} from "@/lib/skill-market-source"

export type SkillMarketUploadMode = "publish" | "addVersion"

export interface PublishForm {
  displayName: string
  summary: string
  category: string
  tags: string
  iconUrl: string
  audience: SkillMarketAudience
  version: string
  changelog: string
  dependencies: string
}

export const EMPTY_PUBLISH: PublishForm = {
  displayName: "",
  summary: "",
  category: "",
  tags: "",
  iconUrl: "",
  audience: "organization",
  version: "1.0.0",
  changelog: "",
  dependencies: "",
}

export interface SkillMarketUploadDialogProps {
  packageType?: "skill" | "plugin"
  open: boolean
  mode: SkillMarketUploadMode
  categories: SkillMarketCategory[]
  targetSkillId: string | null
  busy: boolean
  onOpenChange: (open: boolean) => void
  onPublish: (request: SkillMarketPublishRequestV2) => Promise<void>
  onAddVersion: (request: SkillMarketAddVersionRequestV2) => Promise<void>
}

export interface UploadDetailsProps {
  t: SkillMarketTranslator
  busy: boolean
  isPublish: boolean
  form: PublishForm
  errors: import("@/components/skills/skill-market-upload-validation").SkillMarketUploadErrors
  nameCheck: import("./use-skill-publication").UseSkillPublicationResult
  categories: SkillMarketCategory[]
  update: (field: keyof PublishForm, value: string) => void
}

export function splitTags(value: string): string[] {
  return value
    .split(",")
    .map((tag) => tag.trim())
    .filter(Boolean)
}

export function pluginVersion(
  folder: SelectedSkillMarketFolder
): string | null {
  const manifest = folder.files.find((file) => file.path === ".iyw-plugin.json")
  if (!manifest) return null
  const bytes = Uint8Array.from(atob(manifest.contentBase64), (char) =>
    char.charCodeAt(0)
  )
  const value: unknown = JSON.parse(new TextDecoder().decode(bytes))
  if (
    value &&
    typeof value === "object" &&
    "version" in value &&
    typeof value.version === "string"
  )
    return value.version
  return null
}
