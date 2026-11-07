import { stateDriver } from "./driver.mjs"
import { presetDatabase } from "./indexed.mjs"
import { managedPreferences } from "./preferences.mjs"
import { restoreInterface } from "./bootstrap.mjs"
import { attachEditor } from "./connection.mjs"
import { installCompactEditor } from "./compact.mjs"

export async function installManagedStorage(call, options = {}) {
  const driver = await stateDriver(call)
  const preferences = managedPreferences(driver, options.preferenceKey)
  const presets = options.presets === false ? null : presetDatabase(driver)
  let compact
  Object.defineProperty(window, "localStorage", {
    value: preferences.storage,
    configurable: true,
  })
  if (presets)
    Object.defineProperty(window, "indexedDB", {
      value: presets.factory,
      configurable: true,
    })
  return {
    startCompact: () => {
      if (options.compact !== false) compact = installCompactEditor(call)
    },
    attach: (status, isClosed) => attachEditor(call, status, isClosed),
    async ready(wasm) {
      if (options.restoreInterface !== false)
        await restoreInterface(wasm, preferences.saved)
      await preferences.resume()
    },
    async flush() {
      await compact?.pause()
      try {
        await presets?.flush()
        await preferences.flush()
        await driver.flush()
      } catch (error) {
        compact?.resume()
        throw error
      }
    },
  }
}
