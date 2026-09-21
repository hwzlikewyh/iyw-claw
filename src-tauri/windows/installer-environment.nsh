Var IywClawEnvironmentError

Function IywClawInstallEnvironment
  StrCpy $IywClawEnvironmentError ""
  StrCmp $IywClawInstallerTestMode "1" environment_test_mode
  IfFileExists "$IywClawAppDir\iyw-environment.exe" 0 environment_helper_missing

  DetailPrint "正在从 Fusion 准备用户运行环境..."
  ExecWait '"$IywClawAppDir\iyw-environment.exe" install --phase prepare --app-version "${VERSION}" --json' $R0
  StrCmp $R0 "0" environment_prepared 0
  StrCpy $IywClawEnvironmentError "环境下载或校验失败（退出码=$R0）"
  Goto environment_failed

  environment_prepared:
    DetailPrint "正在提交用户运行环境..."
    ExecWait '"$IywClawAppDir\iyw-environment.exe" install --phase commit --json' $R0
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
    DetailPrint "$IywClawEnvironmentError"
    Push "environment: $IywClawEnvironmentError"
    Call IywClawAppendInstallerLog
    Push "0"
FunctionEnd
