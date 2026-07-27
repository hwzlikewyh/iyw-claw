import { Suspense } from "react"
import { SettingsShell } from "@/components/settings/settings-shell"
import { RemoteConnectionGate } from "@/contexts/remote-connection-context"
import { UpdateProvider } from "@/components/providers/update-provider"
import { SidebarViewOptionsProvider } from "@/contexts/sidebar-view-options-context"

export default function SettingsLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <Suspense>
      <RemoteConnectionGate>
        <UpdateProvider>
          <SidebarViewOptionsProvider>
            <SettingsShell>{children}</SettingsShell>
          </SidebarViewOptionsProvider>
        </UpdateProvider>
      </RemoteConnectionGate>
    </Suspense>
  )
}
