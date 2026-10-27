Var IywClawElevationReady

; 提权必须先于 WebView2、进程停止及应用替换；静默安装也经过此入口。
Section -IywClawElevation
  Call IywClawConfigureInstallerMode
  Call IywClawValidateElevationIdentity
  Call IywClawEnsureInstallElevation
SectionEnd

Function IywClawEnsureInstallElevation
  StrCmp $IywClawElevationReady "1" elevation_ready
  StrCmp $IywClawInstallerTestMode "1" elevation_ready
  StrCmp $IywClawInstallerTestMode "invalid" elevation_invalid_mode
  StrCpy $IywClawPermissionAction "identity"
  Call IywClawRunPermissionCheck
  StrCmp $IywClawPermissionResult "0" 0 elevation_failed
  StrCmp $IywClawElevated "1" elevation_verified
  StrCmp $IywClawOriginalSid "" 0 elevation_not_granted
  Call IywClawSelectInstallRoot
  Call IywClawNormalizeSelectedInstallRoot
  Call IywClawLaunchElevatedInstaller
  StrCmp $IywClawPermissionLaunched "1" elevation_finished
  StrCmp $IywClawPermissionResult "1223" 0 elevation_failed
  StrCpy $IywClawPermissionError "已取消管理员权限授权，安装尚未修改应用或环境。"
  Goto elevation_failed
  elevation_invalid_mode:
    StrCpy $IywClawPermissionResult "2"
    StrCpy $IywClawPermissionError "测试模式参数无效，安装已取消。"
    Goto elevation_failed
  elevation_not_granted:
    StrCpy $IywClawPermissionResult "1"
    StrCpy $IywClawPermissionError "安装器未取得管理员权限，安装已停止。"
  elevation_failed:
    DetailPrint "$IywClawPermissionError"
    IfSilent elevation_finished 0
    MessageBox MB_OK|MB_ICONSTOP "$IywClawPermissionError"
  elevation_finished:
    SetErrorLevel $IywClawPermissionResult
    Quit
  elevation_verified:
    StrCpy $IywClawElevationReady "1"
    DetailPrint "已取得管理员权限，仅用于本次安装；原助理将以普通用户权限启动。"
  elevation_ready:
FunctionEnd

Function IywClawLaunchElevatedInstaller
  ${GetParameters} $R0
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ARGUMENTS", w R0)'
  StrCpy $R0 "$EXEPATH"
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXECUTABLE", w R0)'
  StrCpy $IywClawPermissionAction "elevate"
  DetailPrint "正在申请管理员权限..."
  Call IywClawRunPermissionCheck
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_ARGUMENTS", p 0)'
  System::Call 'kernel32::SetEnvironmentVariableW(w "IYW_INSTALL_EXECUTABLE", p 0)'
FunctionEnd
