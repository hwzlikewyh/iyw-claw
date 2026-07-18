import { describe, expect, it } from "vitest"

import {
  focusSidebarToggleAfterCollapse,
  resolveSidebarPresentation,
} from "./sidebar-presentation"

describe("resolveSidebarPresentation", () => {
  it("keeps both desktop layers mounted while only the expanded layer is interactive", () => {
    expect(resolveSidebarPresentation(true, false)).toEqual({
      renderExpanded: true,
      renderRail: true,
      expandedInteractive: true,
      railInteractive: false,
    })
  })

  it("keeps both desktop layers mounted while only the rail is interactive", () => {
    expect(resolveSidebarPresentation(false, false)).toEqual({
      renderExpanded: true,
      renderRail: true,
      expandedInteractive: false,
      railInteractive: true,
    })
  })

  it("keeps mobile content mounted but inert while the sheet exits", () => {
    expect(resolveSidebarPresentation(false, true)).toEqual({
      renderExpanded: true,
      renderRail: false,
      expandedInteractive: false,
      railInteractive: false,
    })
  })

  it("moves focus from the expanded layer to the persistent toggle", () => {
    const expandedLayer = document.createElement("div")
    const expandedButton = document.createElement("button")
    const toggleButton = document.createElement("button")
    expandedLayer.append(expandedButton)
    document.body.append(expandedLayer, toggleButton)
    expandedButton.focus()

    expect(focusSidebarToggleAfterCollapse(expandedLayer, toggleButton)).toBe(
      true
    )
    expect(document.activeElement).toBe(toggleButton)

    expandedLayer.remove()
    toggleButton.remove()
  })
})
