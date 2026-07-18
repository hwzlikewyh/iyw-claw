export interface SidebarPresentation {
  renderExpanded: boolean
  renderRail: boolean
  expandedInteractive: boolean
  railInteractive: boolean
}

export function focusSidebarToggleAfterCollapse(
  expandedLayer: HTMLElement | null,
  toggleButton: HTMLButtonElement | null
): boolean {
  const activeElement = document.activeElement
  if (
    !expandedLayer ||
    !toggleButton ||
    !(activeElement instanceof HTMLElement) ||
    !expandedLayer.contains(activeElement)
  ) {
    return false
  }

  toggleButton.focus()
  return document.activeElement === toggleButton
}

export function resolveSidebarPresentation(
  isOpen: boolean,
  isMobile: boolean
): SidebarPresentation {
  if (isMobile) {
    return {
      renderExpanded: true,
      renderRail: false,
      expandedInteractive: isOpen,
      railInteractive: false,
    }
  }

  return {
    renderExpanded: true,
    renderRail: true,
    expandedInteractive: isOpen,
    railInteractive: !isOpen,
  }
}
