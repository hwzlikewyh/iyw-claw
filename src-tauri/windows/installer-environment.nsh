Var IywClawEnvironmentError
Var IywClawEnvironmentVersion
Var IywClawEnvironmentPrepareCode
!include "${__FILEDIR__}\installer-environment-worker.nsh"

Function IywClawInstallEnvironment
  StrCpy $IywClawEnvironmentError ""
  StrCmp $IywClawInstallerTestMode "1" environment_ready 0
  environment_retry:
    StrCmp $IywClawEnvironmentStarted "1" 0 environment_failed
    ClearErrors
    FileOpen $R0 "$PLUGINSDIR\environment.commit" w
    IfErrors environment_failed 0
    FileClose $R0
    Call IywClawWaitEnvironment
    Pop $R5
    StrCpy $IywClawEnvironmentPrepareCode $IywClawEnvironmentCode
    Call IywClawStopEnvironmentWorker
    Pop $R0
    StrCmp $R0 "1" 0 environment_cancelled
    StrCmp $R5 "1" environment_ready environment_failed
  environment_ready:
    Push "1"
    Return
  environment_failed:
    StrCpy $IywClawEnvironmentError "初始化未完成，请检查网络连接、磁盘空间和目录权限后重试。详细原因已写入安装日志。"
    Push "initialization failed: code=$IywClawEnvironmentPrepareCode"
    Call IywClawAppendInstallerLog
    StrCmp $IywClawEnvironmentPrepareCode "31" 0 environment_show_error
    StrCpy $IywClawPermissionError "$IywClawEnvironmentError"
    Call IywClawTryElevatedInstall
    StrCmp $IywClawElevationRolledBack "failed" environment_cancelled 0
    StrCmp $IywClawElevationRolledBack "1" environment_cancelled 0
  environment_show_error:
    IfSilent environment_cancelled 0
    MessageBox MB_RETRYCANCEL|MB_ICONSTOP "$IywClawEnvironmentError$\r$\n日志：$IywClawRoot\logs\installer-initialization.log" IDRETRY environment_restart
    Goto environment_cancelled
  environment_restart:
    Call IywClawLaunchEnvironmentWorker
    Goto environment_retry
  environment_cancelled:
    Push "0"
FunctionEnd
