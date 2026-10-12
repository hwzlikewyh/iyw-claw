import type { Scenario } from "@/lib/types"
import type { ComposerInjectContent } from "./message-input"

export function scenarioContent(
  scenario: Scenario,
  prompt: string
): ComposerInjectContent {
  const content: ComposerInjectContent = {
    text: prompt,
    scenario: { variables: scenario.variables ?? [] },
  }
  if (scenario.skillPackageId !== "0" && scenario.skillPackageSlug) {
    content.skill = {
      id: scenario.skillPackageSlug,
      label: scenario.displayName,
      hiddenFromDisplay: scenario.hideSkillReference,
      package: {
        id: scenario.skillPackageId,
        slug: scenario.skillPackageSlug,
        version: scenario.skillPackageVersion,
      },
    }
  }
  return content
}
