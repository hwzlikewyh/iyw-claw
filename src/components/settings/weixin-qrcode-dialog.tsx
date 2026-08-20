"use client"

import { ChatChannelQrcodeDialog } from "./chat-channel-qrcode-dialog"

interface WeixinQrcodeDialogProps {
  open: boolean
  channelId: number
  onOpenChange: (open: boolean) => void
  onAuthSuccess: (channelId: number) => void
}

export function WeixinQrcodeDialog(props: WeixinQrcodeDialogProps) {
  return <ChatChannelQrcodeDialog {...props} channelType="weixin" />
}
