import {
  normalizeSettingsSection,
  settingsPathToSection,
  settingsSectionToNavPath,
  settingsSectionToPath,
} from "./settings-navigation"

describe("settings skill-pack navigation", () => {
  it("uses the unified skills route as the canonical entry", () => {
    expect(normalizeSettingsSection("skills")).toBe("skills")
    expect(settingsSectionToPath("skills")).toBe("/settings/skills")
  })

  it.each(["experts", "office-tools", "internet-tools"] as const)(
    "keeps the %s compatibility route",
    (section) => {
      expect(settingsSectionToPath(section)).toBe(`/settings/${section}`)
      expect(settingsSectionToNavPath(section)).toBe("/settings/skills")
    }
  )

  it.each(["experts", "office-tools", "internet-tools"] as const)(
    "preserves the %s legacy route for initial category selection",
    (section) => {
      expect(settingsPathToSection(`/settings/${section}`)).toBe(section)
    }
  )
})
