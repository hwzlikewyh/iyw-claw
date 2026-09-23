Var IywClawEnvironmentStarted
Var IywClawEnvironmentState
Var IywClawEnvironmentCode
Var IywClawEnvironmentWait
Var IywClawEnvironmentHandle
!define IYW_CLAW_ENVIRONMENT_SOURCE "${__FILEDIR__}"
!define IYW_CLAW_ENVIRONMENT_POLL_MS 250
!define IYW_CLAW_ENVIRONMENT_WAIT_POLLS 10000
!define IYW_CLAW_ENVIRONMENT_CANCEL_POLLS 80
!include "${__FILEDIR__}\installer-progress.nsh"

!macro IywClawStartEnvironmentPreparation
  StrCpy $IywClawEnvironmentVersion "${VERSION}"
  StrCmp $IywClawInstallerTestMode "1" iyw_environment_background_ready 0
  InitPluginsDir
  SetDetailsPrint none
  !if "${ARCH}" == "arm64"
    File /oname=$PLUGINSDIR\iyw-environment.exe "${IYW_CLAW_ENVIRONMENT_SOURCE}\..\binaries\iyw-environment-aarch64-pc-windows-msvc.exe"
  !else
    File /oname=$PLUGINSDIR\iyw-environment.exe "${IYW_CLAW_ENVIRONMENT_SOURCE}\..\binaries\iyw-environment-x86_64-pc-windows-msvc.exe"
  !endif
  File /oname=$PLUGINSDIR\installer-worker.ps1 "${IYW_CLAW_ENVIRONMENT_SOURCE}\installer-worker.ps1"
  File /oname=$PLUGINSDIR\installer-worker-native.cs "${IYW_CLAW_ENVIRONMENT_SOURCE}\installer-worker-native.cs"
  File /oname=$PLUGINSDIR\installer-worker-launch.ps1 "${IYW_CLAW_ENVIRONMENT_SOURCE}\installer-worker-launch.ps1"
  Call IywClawLaunchEnvironmentWorker
  iyw_environment_background_ready:
!macroend

Function IywClawLaunchEnvironmentWorker
  SetDetailsPrint none
  StrCmp $IywClawOriginalProgress "" 0 +2
    StrCpy $IywClawOriginalProgress "0"
  StrCmp $IywClawProgressBar "" 0 +2
    StrCpy $IywClawProgressBar "0"
  StrCmp $IywClawProgressText "" 0 +2
    StrCpy $IywClawProgressText "0"
  Delete "$PLUGINSDIR\environment.status.ini"
  Delete "$PLUGINSDIR\environment.progress.ini"
  Delete "$PLUGINSDIR\environment.commit"
  Delete "$PLUGINSDIR\environment.cancel"
  System::Call 'kernel32::GetCurrentProcessId() i.R0'
  nsExec::ExecToStack /TIMEOUT=10000 '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "$PLUGINSDIR\installer-worker-launch.ps1" -Directory "$PLUGINSDIR" -AppVersion "$IywClawEnvironmentVersion" -InstallerPid $R0 -OriginalBar "$IywClawOriginalProgress" -ProgressBar "$IywClawProgressBar" -StatusText "$IywClawProgressText"'
  Pop $R0
  Pop $R1
  StrCmp $R0 "0" 0 environment_worker_launch_failed
  System::Call 'kernel32::OpenProcess(i 0x101000, i 0, i R1) p.R0'
  StrCpy $IywClawEnvironmentHandle $R0
  StrCmp $R0 "0" environment_worker_launch_failed 0
  StrCpy $IywClawEnvironmentStarted "1"
  Return
  environment_worker_launch_failed:
    FileOpen $R2 "$PLUGINSDIR\environment.cancel" w
    FileClose $R2
    Push "initialization worker launch failed: code=$R0; $R1"
    Call IywClawAppendInstallerLog
    StrCpy $IywClawEnvironmentStarted "0"
    StrCpy $IywClawEnvironmentError "无法启动初始化任务，请检查系统权限后重试。"
FunctionEnd

Function IywClawWaitEnvironment
  StrCpy $IywClawEnvironmentWait 0
  environment_worker_wait:
    StrCpy $IywClawEnvironmentState ""
    ReadINIStr $IywClawEnvironmentState "$PLUGINSDIR\environment.status.ini" "result" "State"
    StrCmp $IywClawEnvironmentState "committed" environment_worker_success 0
    StrCmp $IywClawEnvironmentState "failed" environment_worker_failed 0
    StrCmp $IywClawEnvironmentState "cancelled" environment_worker_failed 0
    System::Call 'kernel32::WaitForSingleObject(p $IywClawEnvironmentHandle, i 0) i.R0'
    StrCmp $R0 "258" 0 environment_worker_exit_failed
    IntOp $IywClawEnvironmentWait $IywClawEnvironmentWait + 1
    IntCmp $IywClawEnvironmentWait ${IYW_CLAW_ENVIRONMENT_WAIT_POLLS} environment_worker_timeout 0 environment_worker_timeout
    Sleep ${IYW_CLAW_ENVIRONMENT_POLL_MS}
    Goto environment_worker_wait
  environment_worker_exit_failed:
    ReadINIStr $IywClawEnvironmentState "$PLUGINSDIR\environment.status.ini" "result" "State"
    StrCmp $IywClawEnvironmentState "committed" environment_worker_success 0
    StrCmp $IywClawEnvironmentState "failed" environment_worker_failed 0
    StrCpy $IywClawEnvironmentCode "worker-exited"
    Push "0"
    Return
  environment_worker_timeout:
    StrCpy $IywClawEnvironmentCode "timeout"
    Push "0"
    Return
  environment_worker_failed:
    ReadINIStr $IywClawEnvironmentCode "$PLUGINSDIR\environment.status.ini" "result" "Code"
    Push "0"
    Return
  environment_worker_success:
    StrCpy $IywClawEnvironmentCode "0"
    Push "1"
FunctionEnd

Function IywClawStopEnvironmentWorker
  StrCmp $IywClawEnvironmentStarted "1" 0 environment_worker_stopped
  FileOpen $R0 "$PLUGINSDIR\environment.cancel" w
  FileClose $R0
  StrCpy $IywClawEnvironmentWait 0
  environment_worker_cancel_wait:
    System::Call 'kernel32::WaitForSingleObject(p $IywClawEnvironmentHandle, i 0) i.R0'
    StrCmp $R0 "0" environment_worker_stopped 0
    IntOp $IywClawEnvironmentWait $IywClawEnvironmentWait + 1
    IntCmp $IywClawEnvironmentWait ${IYW_CLAW_ENVIRONMENT_CANCEL_POLLS} environment_worker_stop_failed 0 environment_worker_stop_failed
    Sleep ${IYW_CLAW_ENVIRONMENT_POLL_MS}
    Goto environment_worker_cancel_wait
  environment_worker_stop_failed:
    StrCpy $IywClawTransactionError "初始化任务尚未停止，已保留应用和备份，请等待任务退出后重试安装。"
    Push "0"
    Return
  environment_worker_stopped:
    StrCmp $IywClawEnvironmentHandle "" environment_worker_handle_closed 0
    StrCmp $IywClawEnvironmentHandle "0" environment_worker_handle_closed 0
    System::Call 'kernel32::CloseHandle(p $IywClawEnvironmentHandle)'
    StrCpy $IywClawEnvironmentHandle "0"
  environment_worker_handle_closed:
    StrCpy $IywClawEnvironmentStarted "0"
    IfFileExists "$PLUGINSDIR\environment-worker.log" 0 environment_worker_log_done
    CreateDirectory "$IywClawRoot\logs"
    CopyFiles /SILENT "$PLUGINSDIR\environment-worker.log" "$IywClawRoot\logs\installer-initialization.log"
  environment_worker_log_done:
    Push "1"
FunctionEnd
