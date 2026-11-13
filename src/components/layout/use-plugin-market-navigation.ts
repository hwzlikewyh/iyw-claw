import { useAuxPanelContext } from "@/contexts/aux-panel-context"
import { useWorkbenchRoute } from "@/contexts/workbench-route-context"

export function usePluginMarketNavigation() {
  const { setOpen } = useAuxPanelContext()
  const route = useWorkbenchRoute()
  return {
    openPluginMarket: () => {
      setOpen(false)
      route.openPluginMarket()
    },
    openSkillMarket: (slug: string) => {
      setOpen(false)
      route.openSkillMarket(slug, "plugin")
    },
    openPlugin: (slug: string) => {
      setOpen(false)
      route.openSkillMarket(slug, "plugin")
    },
  }
}
