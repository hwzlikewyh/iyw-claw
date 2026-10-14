export const hasOwn = (value: object, key: PropertyKey): boolean =>
  Object.prototype.hasOwnProperty.call(value, key)
