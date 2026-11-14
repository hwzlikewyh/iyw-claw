"use client"

import { Suspense } from "react"
import { RemoteConnectionGate } from "@/contexts/remote-connection-context"
import { DetachedPluginPanel } from "@/components/layout/detached-plugin-panel"

export default function PluginWindowPage() {
  return (
    <Suspense fallback={null}>
      <RemoteConnectionGate>
        <DetachedPluginPanel />
      </RemoteConnectionGate>
    </Suspense>
  )
}
