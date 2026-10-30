// 已完成的回复通常包含数百条工具记录；流式更新只需重算发生变化的那一组。
export function createMessageGroupCache<T extends object>() {
  const entries = new WeakMap<
    T,
    { members: T[]; streaming: boolean; result: T }
  >()
  return {
    get(members: T[], streaming: boolean): T | undefined {
      const first = members[0]
      const cached = first && entries.get(first)
      return cached &&
        cached.streaming === streaming &&
        cached.members.length === members.length &&
        members.every((member, index) => member === cached.members[index])
        ? cached.result
        : undefined
    },
    set(members: T[], streaming: boolean, result: T): void {
      if (members[0]) entries.set(members[0], { members, streaming, result })
    },
  }
}
