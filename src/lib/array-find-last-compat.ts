// Tiptap 3.26 的事务分发使用 findLast；部分旧 WebView 尚未实现它。
export function hasArrayFindLast(): boolean {
  return typeof Reflect.get(Array.prototype, "findLast") === "function"
}

function findLast<T>(
  this: ArrayLike<T>,
  predicate: (value: T, index: number, array: ArrayLike<T>) => unknown,
  thisArg?: unknown
): T | undefined {
  if (this == null) {
    throw new TypeError("Array.prototype.findLast called on null or undefined")
  }
  const values = Object(this) as ArrayLike<T>
  const length = Math.min(
    Math.max(Math.trunc(+values.length) || 0, 0),
    Number.MAX_SAFE_INTEGER
  )
  if (typeof predicate !== "function") {
    throw new TypeError("Array.prototype.findLast predicate must be a function")
  }
  // 与原生实现一致：固定初始长度，从后往前访问，空槽也参与判断。
  for (let index = length - 1; index >= 0; index -= 1) {
    const value = values[index]
    if (Reflect.apply(predicate, thisArg, [value, index, values])) return value
  }
  return undefined
}

export function ensureArrayFindLast(): void {
  if (hasArrayFindLast()) return

  Object.defineProperty(Array.prototype, "findLast", {
    configurable: true,
    writable: true,
    value: findLast,
  })
  const unscopables = Array.prototype[Symbol.unscopables]
  if (unscopables) Reflect.set(unscopables, "findLast", true)

  console.warn("[composer] enabled Array.prototype.findLast compatibility")
}
