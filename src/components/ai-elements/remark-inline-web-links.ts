interface MarkdownNode {
  type: string
  value?: string
  children?: MarkdownNode[]
}

// 只转换独立的完整网址，代码块和包含命令/表达式的行内代码保持原样。
export function remarkInlineWebLinks() {
  return (tree: MarkdownNode) => {
    const visit = (node: MarkdownNode) => {
      if (
        !node.children ||
        node.type === "link" ||
        node.type === "linkReference"
      )
        return
      node.children = node.children.map((child) => {
        if (child.type !== "inlineCode" || !child.value) {
          visit(child)
          return child
        }
        const value = child.value
        if (!/^https?:\/\/[^\s<>]+$/i.test(value)) return child
        try {
          const url = new URL(value)
          if (!url.hostname || url.username || url.password) return child
          return {
            type: "link",
            url: value,
            children: [{ type: "text", value }],
          }
        } catch {
          return child
        }
      })
    }
    visit(tree)
  }
}
