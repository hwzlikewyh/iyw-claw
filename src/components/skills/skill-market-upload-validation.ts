import {
  isValidSemVer,
  validateSkillDependencies,
} from "@/components/skills/skill-market-semver"

export type SkillMarketUploadField =
  | "slug"
  | "displayName"
  | "summary"
  | "category"
  | "version"
  | "dependencies"

export type SkillMarketUploadFieldError = {
  code: "required" | "slug" | "version" | "dependencies"
  line?: number | null
}

export type SkillMarketUploadErrors = Partial<
  Record<SkillMarketUploadField, SkillMarketUploadFieldError>
>

type PublishForm = {
  slug: string
  displayName: string
  summary: string
  category: string
  version: string
  dependencies: string
}

type VersionForm = Pick<PublishForm, "version" | "dependencies">

const SLUG_PATTERN = /^[a-z0-9]+(?:-[a-z0-9]+)*$/

function requiredError(value: string): SkillMarketUploadFieldError | null {
  return value.trim() ? null : { code: "required" }
}

function dependencyError(value: string): SkillMarketUploadFieldError | null {
  const result = validateSkillDependencies(value)
  if (result.valid) return null
  return { code: "dependencies", line: result.line }
}

export function validateSkillMarketPublishForm(
  form: PublishForm
): SkillMarketUploadErrors {
  const errors: SkillMarketUploadErrors = {}
  const slugRequired = requiredError(form.slug)
  const displayNameRequired = requiredError(form.displayName)
  const categoryRequired = requiredError(form.category)

  if (slugRequired) errors.slug = slugRequired
  else if (!SLUG_PATTERN.test(form.slug.trim())) errors.slug = { code: "slug" }
  if (displayNameRequired) errors.displayName = displayNameRequired
  if (categoryRequired) errors.category = categoryRequired
  if (!isValidSemVer(form.version)) errors.version = { code: "version" }

  const dependencies = dependencyError(form.dependencies)
  if (dependencies) errors.dependencies = dependencies
  return errors
}

export function validateSkillMarketVersionForm(
  form: VersionForm
): SkillMarketUploadErrors {
  const errors: SkillMarketUploadErrors = {}
  if (!isValidSemVer(form.version)) errors.version = { code: "version" }
  const dependencies = dependencyError(form.dependencies)
  if (dependencies) errors.dependencies = dependencies
  return errors
}

export function hasSkillMarketUploadErrors(
  errors: SkillMarketUploadErrors
): boolean {
  return Object.keys(errors).length > 0
}
