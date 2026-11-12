import React from "react"
import { createRoot } from "react-dom/client"
import { createMemoryRouter, Outlet, RouterProvider } from "react-router-dom"
import { App, Button, ConfigProvider } from "antd"
import { ProConfigProvider } from "@ant-design/pro-components"
import { QueryClient, QueryClientProvider } from "@tanstack/react-query"
import { BookOpen, ImagePlus, Images, Maximize2, Moon, Settings2, Sun, Video } from "lucide-react"
import { Link, useLocation } from "react-router-dom"
import { useTranslation } from "react-i18next"
import dayjs from "dayjs"
import zhCN from "antd/es/locale/zh_CN"
import enUS from "antd/es/locale/en_US"
import "antd/dist/reset.css"
import "streamdown/styles.css"
import "@/styles/globals.css"
import "dayjs/locale/zh-cn"
import "@/i18n"
import HomePage from "@/pages/home"
import ImagePage from "@/pages/image"
import VideoPage from "@/pages/video"
import AssetsPage from "@/pages/assets"
import PromptsPage from "@/pages/prompts"
import CanvasPage from "@/pages/canvas"
import CanvasProjectPage from "@/pages/canvas/project"
import ConfigPage from "@/pages/config"
import NotFound from "@/pages/not-found"
import { useThemeStore } from "@/stores/use-theme-store"
import { getAntThemeConfig } from "@/lib/app-theme"
import { PluginCanvasBridge } from "@/components/layout/plugin-canvas-bridge"
import type { ReactNode } from "react"

const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false, staleTime: 30_000, refetchOnWindowFocus: false } } })
const routes = createMemoryRouter([
  { element: <WorkbenchShell />, children: [
    { path: "/", element: <HomePage /> },
    { path: "/canvas", element: <CanvasPage /> },
    { path: "/canvas/:id", element: <CanvasProjectPage /> },
    { path: "/image", element: <ImagePage /> },
    { path: "/video", element: <VideoPage /> },
    { path: "/prompts", element: <PromptsPage /> },
    { path: "/assets", element: <AssetsPage /> },
    { path: "/config", element: <ConfigPage /> },
  ] },
  { path: "*", element: <NotFound /> },
], { initialEntries: ["/canvas"] })

function WorkbenchShell() {
  const { t } = useTranslation()
  const location = useLocation()
  const theme = useThemeStore((state) => state.theme)
  const setTheme = useThemeStore((state) => state.setTheme)
  const dark = theme === "dark"
  const projectOpen = /^\/canvas\/[^/]+/.test(location.pathname)
  const items = [
    ["canvas", Maximize2], ["image", ImagePlus], ["video", Video],
    ["prompts", BookOpen], ["assets", Images],
  ] as const
  return (
    <div className="flex h-dvh min-h-0 flex-col overflow-hidden bg-background text-foreground">
      {!projectOpen ? <header className="z-20 flex h-14 shrink-0 items-center gap-6 border-b border-stone-200 px-5 dark:border-stone-800">
        <Link to="/canvas" className="flex shrink-0 items-center gap-2 text-sm font-semibold text-stone-950 dark:text-stone-100">{t("meta.title")}</Link>
        <nav className="flex min-w-0 flex-1 items-center gap-5 overflow-x-auto">
          {items.map(([slug, Icon]) => <Link key={slug} to={`/${slug}`} className={`flex h-14 shrink-0 items-center gap-2 border-b text-sm ${location.pathname.startsWith(`/${slug}`) ? "border-current font-medium text-stone-950 dark:text-stone-100" : "border-transparent text-stone-500 hover:text-stone-900 dark:text-stone-400 dark:hover:text-stone-100"}`}>
            <Icon className="size-4" /><span>{t(`navigation.${slug}`)}</span>
          </Link>)}
        </nav>
        <Link to="/config" aria-label={t("navigation.config")} title={t("navigation.config")} className="flex size-8 shrink-0 items-center justify-center text-stone-600 hover:text-stone-950 dark:text-stone-300 dark:hover:text-white"><Settings2 className="size-4" /></Link>
        <Button type="text" shape="circle" aria-label={t(dark ? "canvasShell.lightTheme" : "canvasShell.darkTheme")} title={t(dark ? "canvasShell.lightTheme" : "canvasShell.darkTheme")} icon={dark ? <Sun className="size-4" /> : <Moon className="size-4" />} onClick={() => setTheme(dark ? "light" : "dark")} />
      </header> : null}
      <main className="min-h-0 flex-1 overflow-hidden"><Outlet /><PluginCanvasBridge /></main>
    </div>
  )
}

function WorkbenchProviders({ children }: { children: ReactNode }) {
  const theme = useThemeStore((state) => state.theme)
  const { i18n } = useTranslation()
  const locale = i18n.resolvedLanguage || "zh-CN"
  React.useEffect(() => {
    document.documentElement.lang = locale
    document.documentElement.classList.toggle("dark", theme === "dark")
    document.documentElement.style.colorScheme = theme
    document.title = "Infinite Canvas"
    dayjs.locale(locale === "zh-CN" ? "zh-cn" : "en")
  }, [locale, theme])
  return <ConfigProvider locale={locale === "zh-CN" ? zhCN : enUS} theme={getAntThemeConfig(theme === "dark")}><ProConfigProvider dark={theme === "dark"}><App><QueryClientProvider client={queryClient}>{children}</QueryClientProvider></App></ProConfigProvider></ConfigProvider>
}

const root = document.getElementById("root")
if (!root) throw new Error("Infinite Canvas root is missing")
createRoot(root).render(<React.StrictMode><WorkbenchProviders><RouterProvider router={routes} /></WorkbenchProviders></React.StrictMode>)
