Var IywClawEnvironmentError
Var IywClawEnvironmentVersion
Var IywClawEnvironmentPrepareCode

Function IywClawInstallEnvironment
  StrCpy $IywClawEnvironmentError ""
  StrCpy $IywClawEnvironmentPrepareCode ""
  StrCmp $IywClawInstallerTestMode "1" environment_test_mode
  IfFileExists "$IywClawAppDir\iyw-environment.exe" 0 environment_helper_missing

  environment_retry:
  Delete "$PROFILE\.iyw-claw\logs\environment\last-error.txt"
  DetailPrint "正在从 Fusion 准备用户运行环境..."
  nsExec::ExecToLog /TIMEOUT=1200000 '"$IywClawAppDir\iyw-environment.exe" install --phase prepare --app-version "$IywClawEnvironmentVersion" --json'
  Pop $R0
  StrCpy $IywClawEnvironmentPrepareCode $R0
  StrCmp $R0 "0" environment_prepared 0
  StrCpy $IywClawEnvironmentError "环境下载或校验失败（退出码=$R0）"
  Goto environment_failed

  environment_prepared:
    DetailPrint "正在提交用户运行环境..."
    nsExec::ExecToLog /TIMEOUT=1200000 '"$IywClawAppDir\iyw-environment.exe" install --phase commit --json'
    Pop $R0
    StrCmp $R0 "0" environment_ready 0
    StrCpy $IywClawEnvironmentError "环境提交失败（退出码=$R0）"
    Goto environment_failed

  environment_helper_missing:
    StrCpy $IywClawEnvironmentError "安装包缺少环境修复程序"
    Goto environment_failed

  environment_test_mode:
    DetailPrint "安装器测试模式：跳过真实环境网络下载。"

  environment_ready:
    Push "1"
    Return

  environment_failed:
    ClearErrors
    FileOpen $R1 "$PROFILE\.iyw-claw\logs\environment\last-error.txt" r
    IfErrors environment_error_ready
    FileReadUTF16LE $R1 $R2
    FileClose $R1
    StrCmp $R2 "" environment_error_ready
    StrCpy $IywClawEnvironmentError "$IywClawEnvironmentError$\r$\n$R2"
  environment_error_ready:
    DetailPrint "$IywClawEnvironmentError"
    Push "environment: $IywClawEnvironmentError"
    Call IywClawAppendInstallerLog
    StrCmp $IywClawEnvironmentPrepareCode "31" 0 environment_show_error
    StrCpy $IywClawPermissionError "$IywClawEnvironmentError"
    Call IywClawTryElevatedInstall
    StrCpy $IywClawEnvironmentError "$IywClawEnvironmentError$\r$\n$IywClawPermissionError"
    ; 提权前可能已恢复旧 app；此时不能从旧 helper 继续当前版本的安装。
    StrCmp $IywClawElevationRolledBack "failed" environment_cancelled 0
    StrCmp $IywClawElevationRolledBack "1" environment_cancelled environment_show_error
  environment_show_error:
    IfSilent environment_cancelled
    StrCmp $PassiveMode "1" environment_cancelled
    MessageBox MB_RETRYCANCEL|MB_ICONSTOP "$IywClawEnvironmentError$\r$\n$\r$\n修复网络、磁盘或权限问题后可重试。$\r$\n日志：$PROFILE\.iyw-claw\logs\environment\last-error.log" IDRETRY environment_retry
  environment_cancelled:
    Push "0"
FunctionEnd
