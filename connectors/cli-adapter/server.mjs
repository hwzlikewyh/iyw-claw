import { McpServer } from "@modelcontextprotocol/sdk/server/mcp.js"
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js"
import { z } from "zod/v4"
import { execute, stopChildren } from "./process.mjs"
import { ready, resolveCommand } from "./runtime.mjs"
import { bundledHelp } from "./help.mjs"

const MAX_ARGS = 64
const MAX_ARG_BYTES = 4096

async function callTool(context, tool, { args, signal }) {
  try {
    if (tool.helpFiles?.length && args.length === 1 && args[0] === "--help")
      return await bundledHelp(context, tool)
    const command = await resolveCommand(context, tool.command)
    const result = await execute(
      context,
      { ...command, args: [...command.args, ...args] },
      { signal }
    )
    return {
      isError: result.code !== 0,
      content: [
        {
          type: "text",
          text: JSON.stringify({
            exitCode: result.code,
            stdout: result.stdout,
            stderr: result.stderr,
          }),
        },
      ],
    }
  } catch (error) {
    return { isError: true, content: [{ type: "text", text: error.message }] }
  }
}

function registerTool(server, context, tool) {
  server.registerTool(
    tool.name,
    {
      description: `${context.manifest.name}：${tool.description}。参数使用数组，不包含程序名；先用 --help 核对用法。写操作须符合用户请求。`,
      inputSchema: {
        args: z
          .array(
            z
              .string()
              .max(MAX_ARG_BYTES)
              .refine((arg) => !arg.includes("\0"))
          )
          .max(MAX_ARGS),
      },
    },
    ({ args }, { signal }) => callTool(context, tool, { args, signal })
  )
}

export async function serve(context) {
  if (!(await ready(context)))
    throw new Error("连接器运行环境尚未就绪，请先完成登录")
  if (context.manifest.mcp) {
    const result = await execute(
      context,
      await resolveCommand(context, context.manifest.mcp),
      { inherit: true, timeoutMs: null }
    )
    process.exitCode = result.code
    return
  }
  const server = new McpServer({ name: context.manifest.id, version: "1.0.1" })
  for (const tool of context.manifest.tools) registerTool(server, context, tool)
  server.server.onclose = stopChildren
  await server.connect(new StdioServerTransport())
}
