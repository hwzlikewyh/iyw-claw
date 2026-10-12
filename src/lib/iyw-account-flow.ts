import { getTransport } from "@/lib/transport"
import type { IywAccountProfile } from "@/lib/types"

export interface IywWorkspace {
  org_code: string
  name: string
  logo_url: string | null
  org_flag: number
  enabled: boolean
  is_own_create: boolean
}

export interface IywAccountSessionDetails {
  org_code?: string | null
  org_list?: IywWorkspace[]
  points_refresh_failures?: number
  points_stale?: boolean
  points_requests_paused?: boolean
  workspace_pending?: boolean
  business_credentials_pending?: boolean
}

export type IywAuthAction =
  | "password"
  | "phone_captcha"
  | "phone"
  | "register"
  | "reset_password"
  | "wechat_poll"
  | "bind_phone"
  | "select_workspace"
  | "create_personal"
  | "set_identity"
  | "cancel"
  | "switch_workspace"
  | "recover_workspace"

export interface IywLoginResult {
  status:
    | "pending"
    | "authenticated"
    | "phone_binding_required"
    | "identity_required"
    | "workspace_required"
    | "no_workspace"
    | "captcha"
    | "password_reset"
    | "cancelled"
  profile: IywAccountProfile | null
  flow_id: string | null
  workspaces: IywWorkspace[]
  message: string | null
  captcha_key: string | null
  security_verification_required: boolean
}

export function iywAccountAuth(
  action: IywAuthAction,
  data: Record<string, unknown> = {}
): Promise<IywLoginResult> {
  return getTransport().call("iyw_account_auth", { request: { action, data } })
}
