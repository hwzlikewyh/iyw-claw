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
    StrCmp $IywClawEnvironmentPrepareCode "31" 0 +2
    StrCpy $IywClawEnvironmentError "组件文件访问被拒绝，自动重试后仍未恢复。请关闭占用文件的程序，检查安全软件的拦截记录及目录权限后重试。"
    StrCmp $IywClawEnvironmentPrepareCode "20" 0 +2
    StrCpy $IywClawEnvironmentError "组件下载失败，请检查网络或代理后重试。已校验的下载缓存会继续复用。"
    StrCmp $IywClawEnvironmentPrepareCode "21" 0 +2
    StrCpy $IywClawEnvironmentError "组件完整性校验失败，请重试下载。若反复失败，请联系支持并提供安装日志。"
    ReadINIStr $R0 "$PLUGINSDIR\environment.status.ini" "result" "Detail"
    StrCmp $R0 "" +2 0
    StrCpy $IywClawEnvironmentError "$IywClawEnvironmentError$\r$\n原因：$R0"
    Push "initialization failed: code=$IywClawEnvironmentPrepareCode"
    Call IywClawAppendInstallerLog
    IfSilent environment_cancelled 0
    MessageBox MB_RETRYCANCEL|MB_ICONSTOP "$IywClawEnvironmentError$\r$\n日志：$IywClawRoot\logs\installer-initialization.log" IDRETRY environment_restart
    Goto environment_cancelled
  environment_restart:
    Call IywClawLaunchEnvironmentWorker
    Goto environment_retry
  environment_cancelled:
    Push "0"
FunctionEnd
