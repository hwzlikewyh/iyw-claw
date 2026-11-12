import { StrictMode, useState } from "react"
import { createRoot } from "react-dom/client"
import { NextIntlClientProvider } from "next-intl"
import { PluginPanelFrame } from "../../src/components/layout/plugin-panel-frame"
import messages from "../../src/i18n/messages/zh-CN.json"
import type { PluginPanelTab } from "../../src/components/layout/use-plugin-panels"
import { requestPluginAppTeardown } from "../../src/lib/plugin-app-teardown"
import { AuxPanelSurface } from "../../src/components/layout/aux-panel-surface"
import { useIsMobile } from "../../src/hooks/use-mobile"

function AuthorPreview({ tab }: { tab: PluginPanelTab }) {
  const controls = usePreviewControls(tab.id)
  const { visible, setVisible, mounted } = controls
  const [desktopSlot, setDesktopSlot] = useState<HTMLDivElement | null>(null)
  const mobile = useIsMobile()
  return (
    <NextIntlClientProvider locale="zh-CN" messages={messages}>
      <header>
        <h1>{tab.label}作者预览</h1>
        <p>
          真实插件服务与现有面板组件。正式账号授权、聊天引用和云端安装需在原助理中验收。
        </p>
        <button type="button" onClick={() => setVisible(true)}>
          打开面板
        </button>
      </header>
      <main>
        {!mobile && (
          <div ref={setDesktopSlot} className="h-full min-h-0 w-full" />
        )}
      </main>
      <AuxPanelSurface
        desktopSlot={desktopSlot}
        mobile={mobile}
        open={visible}
        title={tab.label}
        onOpenChange={setVisible}
      >
        <PreviewControls controls={controls} />
        {mounted && <PluginPanelFrame tab={tab} visible={visible} />}
      </AuxPanelSurface>
    </NextIntlClientProvider>
  )
}

function usePreviewControls(id: string) {
  const [visible, setVisible] = useState(true),
    [mounted, setMounted] = useState(true)
  const [closing, setClosing] = useState(false),
    [error, setError] = useState(false)
  return {
    visible,
    setVisible,
    mounted,
    closing,
    error,
    close: async () => {
      setClosing(true)
      const closed = await requestPluginAppTeardown(id)
      setClosing(false)
      setError(!closed)
      if (closed) setMounted(false)
    },
    reopen: () => {
      setError(false)
      setMounted(true)
    },
  }
}

function PreviewControls({
  controls,
}: {
  controls: ReturnType<typeof usePreviewControls>
}) {
  return (
    <nav
      className="flex shrink-0 items-center gap-3 border-b px-2 py-1 text-xs"
      aria-label="作者预览控制"
    >
      <button type="button" onClick={() => controls.setVisible(false)}>
        收起面板
      </button>
      <button
        type="button"
        disabled={controls.closing}
        onClick={() => {
          if (controls.mounted) void controls.close()
          else controls.reopen()
        }}
      >
        {controls.mounted
          ? controls.closing
            ? "正在关闭面板"
            : "关闭面板"
          : "重新打开面板"}
      </button>
      {controls.error && <p role="alert">保存或关闭尚未完成，已保留页面。</p>}
    </nav>
  )
}

const tab: PluginPanelTab = await fetch("/bootstrap").then((response) =>
  response.json()
)
createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <AuthorPreview tab={tab} />
  </StrictMode>
)
