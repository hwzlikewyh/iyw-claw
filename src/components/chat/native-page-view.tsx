"use client"

import { NativePageToolbar } from "./native-page-toolbar"
import {
  NativePageFind,
  NativePageError,
  NativePageSurface,
  NativePageCapture,
} from "./native-page-parts"
import { useNativePage } from "./use-native-page"
import type { NativePageProps } from "./use-native-page-model"
import { NativePageDownloads } from "./native-page-downloads"

export function NativePageView(props: NativePageProps) {
  const { surface, ...view } = useNativePage(props)
  return (
    <div className="relative flex h-full min-h-0 flex-col">
      <NativePageToolbar
        key={view.url}
        state={view.state}
        url={view.url}
        busy={view.busy}
        picking={view.picking}
        mobile={view.mobile}
        zoom={view.zoom}
        native={view.native}
        onAction={view.action}
        onNavigate={view.navigate}
      />
      <NativePageFind view={view} />
      <NativePageError view={view} />
      <NativePageDownloads downloads={view.downloads} />
      <NativePageSurface view={view} surfaceRef={surface} />
      <NativePageCapture view={view} />
    </div>
  )
}
