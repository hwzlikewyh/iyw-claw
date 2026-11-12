import { create } from "zustand"

export interface PluginPanelReference {
  id: string
  label: string
  pluginSlug: string
  appKey: string
  resourceUri: string
  pluginVersion: string
  conversationId: number
}

interface PluginPanelRegistryState {
  panels: PluginPanelReference[]
  setPanels: (panels: PluginPanelReference[]) => void
  registerPanel: (panel: PluginPanelReference) => void
  clear: () => void
}

export const usePluginPanelRegistry = create<PluginPanelRegistryState>(
  (set) => ({
    panels: [],
    setPanels: (panels) => set({ panels }),
    registerPanel: (panel) =>
      set((state) => ({
        panels: [...state.panels.filter((item) => item.id !== panel.id), panel],
      })),
    clear: () => set({ panels: [] }),
  })
)
