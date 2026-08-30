import { annotationPlugin } from "./annotation/register.js"
import { htmlPlugin } from "./html/register.js"
import { slidesPlugin } from "./slides/register.js"
import { markdownPlugin } from "./markdown/register.js"
import { svgPlugin } from "./svg/register.js"

export const builtinPlugins = [annotationPlugin, htmlPlugin, markdownPlugin, slidesPlugin, svgPlugin] as const

export function registerBuiltinPlugins(): ReadonlyMap<string, (typeof builtinPlugins)[number]> {
  return new Map(builtinPlugins.map((plugin) => [plugin.type, plugin]))
}
