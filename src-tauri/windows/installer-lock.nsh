Var IywClawInstallLock

!macro IywClawDefineInstallLock Prefix
Function ${Prefix}IywClawAcquireInstallLock
  StrCmp $IywClawInstallLock "" install_lock_create 0
  StrCmp $IywClawInstallLock "0" install_lock_create install_lock_ready
  install_lock_create:
    ; 环境目录按用户共享，保守地串行所有原助理安装/卸载事务。
    System::Call 'kernel32::CreateMutexW(p 0, i 0, w "Global\IywClawInstallerTransactionV1") p.R2 ?e'
    Pop $R0
    StrCpy $IywClawInstallLock $R2
    StrCmp $IywClawInstallLock "0" install_lock_failed 0
    StrCmp $R0 "183" install_lock_busy install_lock_ready
  install_lock_busy:
    Call ${Prefix}IywClawReleaseInstallLock
    StrCpy $R1 "另一个原助理安装或卸载正在进行，请等待完成后重试。"
    Goto install_lock_abort
  install_lock_failed:
    StrCpy $R1 "无法取得安装互斥锁（Windows 错误 $R0），可能有其他用户正在安装。"
  install_lock_abort:
    DetailPrint "$R1"
    IfSilent install_lock_quit 0
    MessageBox MB_OK|MB_ICONEXCLAMATION "$R1"
  install_lock_quit:
    SetErrorLevel 1618
    Quit
  install_lock_ready:
FunctionEnd

Function ${Prefix}IywClawReleaseInstallLock
  StrCmp $IywClawInstallLock "" install_lock_released 0
  StrCmp $IywClawInstallLock "0" install_lock_released 0
  System::Call 'kernel32::CloseHandle(p $IywClawInstallLock)'
  StrCpy $IywClawInstallLock "0"
  install_lock_released:
FunctionEnd
!macroend

!insertmacro IywClawDefineInstallLock ""
!insertmacro IywClawDefineInstallLock "un."
