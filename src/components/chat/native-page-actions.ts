import {
  nativePageCommand,
  captureNativePage,
  normalizePageUrl,
} from "@/lib/native-page-api"
import { openUrl, isDesktop } from "@/lib/platform"
import type { NativePageModel, NativePageProps } from "./use-native-page-model"
import type { useNativePageHandoff } from "./use-native-page-handoff"
import type { PageTool } from "./native-page-toolbar"

type Options = {
  props: NativePageProps
  model: NativePageModel
  handoff: ReturnType<typeof useNativePageHandoff>
  noErrors: string
  invalidUrl: string
}
const ZOOM_STEP = 0.1
const ZOOM_MIN = 0.5
const ZOOM_MAX = 2

export function nativePageActions(options: Options) {
  const { props, model, handoff } = options
  const url = model.state?.url ?? props.url
  const operate = pageOperator(props.id, model)
  const local: Partial<Record<PageTool, () => void>> = {
    device: () => model.setMobile(!model.mobile),
    find: () => {
      model.setFind(model.find === null ? "" : null)
      if (model.find !== null) void operate("find", { query: "" })
    },
    share: () => handoff.sendText(`${model.state?.title ?? ""}\n${url}`),
    console: () =>
      handoff.sendText(
        `${url}\n${model.errors.join("\n") || options.noErrors}`
      ),
    external: () => {
      void openUrl(url).catch((error) => model.setError(String(error)))
    },
    copy: () => {
      void navigator.clipboard
        .writeText(url)
        .catch((error) => model.setError(String(error)))
    },
    capture: () => capturePage(options, false, true),
    region: () => capturePage(options, true, false),
    markup: () => capturePage(options, false, false),
    pick: () => {
      model.setPicking(!model.picking)
      void operate(model.picking ? "cancel-pick" : "pick")
    },
    "zoom-in": () => zoomPage(model, operate, ZOOM_STEP),
    "zoom-out": () => zoomPage(model, operate, -ZOOM_STEP),
  }
  const action = (tool: PageTool) => {
    if (model.busy) return
    if (local[tool]) local[tool]!()
    else void operate(tool)
  }
  const navigate = (value: string) => {
    const url = normalizePageUrl(value)
    if (!url) {
      model.setError(options.invalidUrl)
      return
    }
    if (isDesktop()) void operate("navigate", { url })
    else props.onNewTab(url)
  }
  return { action, navigate, operate }
}

function capturePage(
  { props, model, handoff }: Options,
  crop: boolean,
  immediately: boolean
) {
  model.setBusy(true)
  model.setError(null)
  void captureNativePage(props.id)
    .then((image) => {
      if (immediately) handoff.sendCapture(image, "", props.tabId)
      else model.setCapture({ image, crop, tabId: props.tabId })
    })
    .catch((error) => model.setError(String(error)))
    .finally(() => model.setBusy(false))
}

function zoomPage(
  model: NativePageModel,
  operate: (action: string, values?: Record<string, unknown>) => Promise<void>,
  step: number
) {
  const zoom = Math.max(ZOOM_MIN, Math.min(ZOOM_MAX, model.zoom + step))
  model.setZoom(zoom)
  void operate("zoom", { zoom })
}

function pageOperator(id: string, model: NativePageModel) {
  const operate = async (action: string, values?: Record<string, unknown>) => {
    model.setBusy(true)
    model.setError(null)
    try {
      model.setState(await nativePageCommand(id, action, values))
    } catch (error) {
      model.setError(String(error))
      if (action === "pick") model.setPicking(false)
    } finally {
      model.setBusy(false)
    }
  }
  return operate
}
