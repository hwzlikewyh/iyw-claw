Var IywClawApplicationPhaseStarted
Var IywClawWebViewPending
Var IywClawWebViewCode
Var IywClawWebViewError
!define IYW_CLAW_WEBVIEW_GUID "{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}"

; Tauri 在 hooks 后定义模式变量，函数在安装页宏展开时声明。
!macro IywClawDefineWebViewFeedback
Function IywClawShowWebViewPreparation
  Push $R0
  StrCpy $IywClawWebViewPending "0"
  StrCmp $IywClawInstallerTestMode "1" webview_feedback_done 0
  StrCmp $UpdateMode "1" webview_feedback_done 0
  ${If} ${RunningX64}
    ReadRegStr $R0 HKLM "SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\${IYW_CLAW_WEBVIEW_GUID}" "pv"
  ${Else}
    ReadRegStr $R0 HKLM "SOFTWARE\Microsoft\EdgeUpdate\Clients\${IYW_CLAW_WEBVIEW_GUID}" "pv"
  ${EndIf}
  StrCmp $R0 "" 0 webview_feedback_done
  ReadRegStr $R0 HKCU "SOFTWARE\Microsoft\EdgeUpdate\Clients\${IYW_CLAW_WEBVIEW_GUID}" "pv"
  StrCmp $R0 "" 0 webview_feedback_done
  StrCpy $IywClawWebViewPending "1"
  SendMessage $IywClawProgressStage ${WM_SETTEXT} 0 "STR:1 / 3   安装 WebView2 运行库"
  Push "正在准备应用界面所需的 WebView2 运行库。$\r$\n下载和安装可能需要几分钟，请保持网络连接和安装窗口打开。"
  Call IywClawSetInstallActivity
  webview_feedback_done:
    Pop $R0
FunctionEnd

Function IywClawReportWebViewFailure
  ; Tauri 的 WebView2 Section 在 PREINSTALL 之前执行，$1 保留 ExecWait 返回码。
  StrCpy $IywClawWebViewCode $1
  StrCmp $IywClawWebViewCode "" webview_code_unknown 0
  StrCmp $IywClawWebViewCode "0" webview_code_unknown 0
  IntFmt $IywClawWebViewCode "0x%08X" $IywClawWebViewCode
  Goto webview_code_ready
  webview_code_unknown:
    StrCpy $IywClawWebViewCode "未返回，请查看微软安装日志"
  webview_code_ready:
  StrCpy $IywClawWebViewError "WebView2 运行库安装未完成（返回码：$IywClawWebViewCode）。应用文件尚未替换。"
  ClearErrors
  FileOpen $R0 "$TEMP\iyw-claw-webview2-install.log" a
  IfErrors webview_failure_log_done 0
  FileSeek $R0 0 END
  FileWrite $R0 "WebView2 installation failed: code=$IywClawWebViewCode$\r$\n"
  FileClose $R0
  webview_failure_log_done:
  Push "WebView2 安装未完成（$IywClawWebViewCode）。$\r$\n请安装微软官方运行库后重试，详细原因见错误提示。"
  Call IywClawFailProgress
  IfSilent webview_failure_done 0
  StrCmp $PassiveMode "1" webview_failure_done 0
  MessageBox MB_OK|MB_ICONSTOP \
    "$IywClawWebViewError$\r$\n$\r$\n请从微软官方页面下载与系统架构匹配的 Evergreen Standalone Installer，安装后重新运行本安装包。$\r$\nhttps://developer.microsoft.com/microsoft-edge/webview2/$\r$\n$\r$\n安装记录：$TEMP\iyw-claw-webview2-install.log$\r$\n微软日志常见位置：$TEMP\msedge_installer.log；$LOCALAPPDATA\Microsoft\EdgeUpdate\Log\MicrosoftEdgeUpdate.log$\r$\n若仍失败，请提供返回码和日志。请勿直接删除 Microsoft 目录或 EdgeUpdate 注册表项。"
  webview_failure_done:
FunctionEnd
!macroend
