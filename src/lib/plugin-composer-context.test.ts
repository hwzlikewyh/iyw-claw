import { beforeEach, describe, expect, it } from "vitest"
import {
  usePluginComposerContext,
  preparePluginDraft,
  selectComposerPlugin,
  clearComposerPlugin,
  pluginPrompt,
  pluginDraftVisible,
} from "./plugin-composer-context"
import type { PluginPanelChoice } from "./plugin-panel"

const app: PluginPanelChoice = {
  pluginId: "123",
  pluginSlug: "iyw-image-editor",
  appKey: "editor",
  version: "0.1.0",
  label: "PhotoCraft",
  state: "available",
}
beforeEach(() => usePluginComposerContext.setState({ entries: {} }))
describe("plugin composer context", () => {
  it("keeps a prepared empty conversation in welcome mode until its first message", () => {
    preparePluginDraft("draft", 42)
    expect(
      pluginDraftVisible({
        tabId: "draft",
        conversationId: 42,
        sent: false,
        hasTurns: false,
      })
    ).toBe(true)
    expect(
      pluginDraftVisible({
        tabId: "draft",
        conversationId: 42,
        sent: true,
        hasTurns: false,
      })
    ).toBe(false)
    expect(
      pluginDraftVisible({
        tabId: "draft",
        conversationId: 42,
        sent: false,
        hasTurns: true,
      })
    ).toBe(false)
    expect(
      pluginDraftVisible({
        tabId: "other",
        conversationId: 42,
        sent: false,
        hasTurns: false,
      })
    ).toBe(false)
  })
  it("carries the explicit plugin selection without losing text, files or scenario metadata", () => {
    selectComposerPlugin("draft", app)
    const draft = {
      blocks: [{ type: "text" as const, text: "画一张海报" }],
      displayText: "画一张海报",
      skillPackage: { id: "11", slug: "design", version: "1.0.0" },
    }
    const result = pluginPrompt("draft", draft)
    expect(result.blocks[0]).toMatchObject({
      type: "text",
      text: expect.stringContaining("iyw-image-editor"),
    })
    expect(result.blocks[1]).toEqual(draft.blocks[0])
    expect(result.skillPackage).toEqual(draft.skillPackage)
    expect(result.displayText).toBe(draft.displayText)
    expect(pluginPrompt("other", draft)).toBe(draft)
    clearComposerPlugin("draft")
    expect(pluginPrompt("draft", draft)).toBe(draft)
  })
})
