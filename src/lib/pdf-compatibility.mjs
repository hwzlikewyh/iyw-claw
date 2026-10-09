// PDF.js 6 的 legacy 包仍调用该接口；页面与独立 worker 使用同一兼容实现。
if (typeof Promise.withResolvers !== "function") {
  Object.defineProperty(Promise, "withResolvers", {
    configurable: true,
    writable: true,
    value: function withResolvers() {
      let resolve
      let reject
      const promise = new this((onResolve, onReject) => {
        resolve = onResolve
        reject = onReject
      })
      return { promise, resolve, reject }
    },
  })
}

// Chromium 109 的 ReadableStream 尚未提供异步迭代，PDF 文本层会使用此接口。
if (typeof ReadableStream !== "undefined" && !ReadableStream.prototype.values) {
  Object.defineProperty(ReadableStream.prototype, "values", {
    configurable: true,
    writable: true,
    value: async function* values({ preventCancel = false } = {}) {
      const reader = this.getReader()
      let finished = false
      try {
        while (true) {
          const result = await reader.read()
          if (result.done) {
            finished = true
            return
          }
          yield result.value
        }
      } finally {
        try {
          if (!finished && !preventCancel) await reader.cancel()
        } finally {
          reader.releaseLock()
        }
      }
    },
  })
}

if (
  typeof ReadableStream !== "undefined" &&
  !ReadableStream.prototype[Symbol.asyncIterator]
) {
  Object.defineProperty(ReadableStream.prototype, Symbol.asyncIterator, {
    configurable: true,
    writable: true,
    value: ReadableStream.prototype.values,
  })
}
