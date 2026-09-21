Var IywClawEnvironmentError

Function IywClawValidateNewApp
  Call IywClawValidateTransactionPaths
  Pop $R0
  StrCmp $R0 "1" 0 validate_new_app_failed
  Push "$IywClawAppDir"
  Call IywClawIsAppComplete
  Pop $R0
  StrCmp $R0 "1" app_install_valid 0
  StrCpy $IywClawTransactionError "新 app 校验失败：$IywClawAppCheckError"
  Goto validate_new_app_failed

  app_install_valid:
    Push "$IywClawAppDir\iyw-environment.exe"
    Call IywClawIsNonEmptyFile
    Pop $R0
    StrCmp $R0 "1" environment_helper_complete
    StrCpy $IywClawTransactionError "新 app 缺少完整的环境修复程序"
    Goto validate_new_app_failed

  environment_helper_complete:
    Push "$IywClawAppDir"
    Push "check-legacy-files"
    Call IywClawRunKnownProcessCommandAt
    Pop $R0
  StrCmp $R0 "0" app_legacy_check_complete 0
  StrCmp $R0 "1" app_legacy_found app_legacy_check_failed

  app_legacy_found:
    StrCpy $IywClawTransactionError "新 app 仍包含旧 iyw-claw-mcp 文件"
    Goto validate_new_app_failed
  app_legacy_check_failed:
    StrCpy $IywClawTransactionError "无法复核新 app 的旧 MCP 文件"
    Goto validate_new_app_failed

  app_legacy_check_complete:
    Push "1"
    Return

  validate_new_app_failed:
    Push "0"
FunctionEnd

Function IywClawInstallEnvironment
  StrCpy $IywClawEnvironmentError ""
  StrCmp $IywClawInstallerTestMode "1" environment_test_mode
  IfFileExists "$IywClawAppDir\iyw-environment.exe" 0 environment_helper_missing

  environment_retry:
  Delete "$PROFILE\.iyw-claw\logs\environment\last-error.txt"
  DetailPrint "正在从 Fusion 准备用户运行环境..."
  nsExec::ExecToLog /TIMEOUT=1200000 '"$IywClawAppDir\iyw-environment.exe" install --phase prepare --app-version "${VERSION}"'
  Pop $R0
  StrCmp $R0 "0" environment_prepared 0
  StrCpy $IywClawEnvironmentError "环境下载或校验失败（退出码=$R0）"
  Goto environment_failed

  environment_prepared:
    DetailPrint "正在提交用户运行环境..."
    nsExec::ExecToLog /TIMEOUT=1200000 '"$IywClawAppDir\iyw-environment.exe" install --phase commit'
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
    StrCmp $IywClawInstallerTestMode "1" environment_success
    CreateShortCut "$DESKTOP\原助理环境修复.lnk" "$PROFILE\.iyw-claw\maintenance\repair.cmd"
  environment_success:
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
    IfSilent environment_cancelled
    StrCmp $PassiveMode "1" environment_cancelled
    MessageBox MB_RETRYCANCEL|MB_ICONSTOP "$IywClawEnvironmentError$\r$\n$\r$\n下载已自动重试。修复网络、磁盘或权限问题后可重试。$\r$\n日志：$PROFILE\.iyw-claw\logs\environment\last-error.log" IDRETRY environment_retry
  environment_cancelled:
    Push "0"
FunctionEnd
