import type { App } from "@modelcontextprotocol/ext-apps"
import type { AssetClient } from "../asset-client.js"

export type HostBridge = { app: App; assets: AssetClient; modelNames?: Record<string, string> }

declare global { interface Window { __IYW_CANVAS_BRIDGE__?: HostBridge } }
