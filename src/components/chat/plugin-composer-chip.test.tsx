import { act, fireEvent, render, screen } from "@testing-library/react"
import { beforeEach, expect, it, vi } from "vitest"
import { PluginComposerChip } from "./plugin-composer-chip"
import {
  selectComposerPlugin,
  clearPluginComposerContext,
  usePluginComposerContext,
} from "@/lib/plugin-composer-context"

vi.mock("next-intl", () => ({
  useTranslations: () => (_key: string, values: { name: string }) =>
    `移除本次选择的 ${values.name}`,
}))
beforeEach(() => clearPluginComposerContext())
it("shows the current tab's plugin and removes only its explicit selection", () => {
  const view = render(<PluginComposerChip tabId="draft" />)
  act(() => selectComposerPlugin("other", { label: "Other" } as never))
  expect(screen.queryByText("Other")).toBeNull()
  act(() =>
    selectComposerPlugin("draft", {
      label: "PhotoCraft",
      pluginSlug: "iyw-image-editor",
    } as never)
  )
  expect(screen.getByText("PhotoCraft")).toBeTruthy()
  fireEvent.click(
    screen.getByRole("button", { name: "移除本次选择的 PhotoCraft" })
  )
  expect(screen.queryByText("PhotoCraft")).toBeNull()
  expect(
    usePluginComposerContext.getState().entries.other?.selected?.label
  ).toBe("Other")
  view.unmount()
})
