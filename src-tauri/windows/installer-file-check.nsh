Var IywClawFileCheckPath
Var IywClawFileCheckReason
Var IywClawFileCheckAttempts
Var IywClawFileCheckErrorCode

!define IYW_CLAW_FILE_CHECK_ATTEMPTS 31
!define IYW_CLAW_FILE_CHECK_WAIT_MS 500
!define IYW_CLAW_GENERIC_READ 0x80000000
!define IYW_CLAW_SHARE_ALL 7
!define IYW_CLAW_OPEN_EXISTING 3
!define IYW_CLAW_FILE_ATTRIBUTE_NORMAL 0x80

Function IywClawCheckFileOnce
  Push $1
  Push $2
  ; 共享只读打开，避免校验自身与扫描程序冲突；立即捕获原始 Win32 错误。
  System::Call 'kernel32::CreateFileW(w "$IywClawFileCheckPath", i ${IYW_CLAW_GENERIC_READ}, i ${IYW_CLAW_SHARE_ALL}, p 0, i ${IYW_CLAW_OPEN_EXISTING}, i ${IYW_CLAW_FILE_ATTRIBUTE_NORMAL}, p 0) p.r1 ?e'
  Pop $IywClawFileCheckErrorCode
  StrCmp $1 "-1" check_file_open_failed 0
  System::Call 'kernel32::GetFileSizeEx(p r1, *l .r2) i.r0 ?e'
  Pop $IywClawFileCheckErrorCode
  System::Call 'kernel32::CloseHandle(p r1)'
  StrCmp $0 "0" check_file_size_failed 0
  StrCpy $IywClawFileCheckErrorCode "0"
  System::Int64Op $2 > 0
  Pop $0
  StrCmp $0 "0" check_file_empty 0
  StrCpy $IywClawFileCheckReason "ok"
  Goto check_file_done
  check_file_open_failed:
    StrCpy $IywClawFileCheckReason "文件无法读取"
    StrCmp $IywClawFileCheckErrorCode "2" check_file_missing 0
    StrCmp $IywClawFileCheckErrorCode "3" check_file_missing check_file_unreadable
  check_file_missing:
    StrCpy $IywClawFileCheckReason "文件不存在"
    StrCpy $0 "3"
    Goto check_file_done
  check_file_size_failed:
    StrCpy $IywClawFileCheckReason "无法读取文件大小"
  check_file_unreadable:
    StrCpy $0 "2"
    Goto check_file_done
  check_file_empty:
    StrCpy $IywClawFileCheckReason "文件为空（0 字节）"
  check_file_done:
    Pop $2
    Pop $1
    Push $0
FunctionEnd

Function IywClawIsNonEmptyFile
  Exch $0
  Push $1
  Push $2
  Push $3
  StrCpy $IywClawFileCheckPath "$0"
  StrCpy $2 "0"
  StrCpy $3 "1"
  non_empty_file_retry:
    StrCpy $IywClawFileCheckAttempts "$3"
    Call IywClawCheckFileOnce
    Pop $1
    StrCmp $1 "1" non_empty_file_success 0
    StrCmp $1 "2" non_empty_file_retryable non_empty_file_done
  non_empty_file_retryable:
    StrCmp $IywClawFileCheckErrorCode "32" non_empty_file_retry_wait 0
    StrCmp $IywClawFileCheckErrorCode "33" non_empty_file_retry_wait non_empty_file_done
  non_empty_file_retry_wait:
    IntCmp $3 ${IYW_CLAW_FILE_CHECK_ATTEMPTS} non_empty_file_done non_empty_file_wait non_empty_file_done
  non_empty_file_wait:
    StrCmp $3 "1" 0 non_empty_file_sleep
    DetailPrint "文件暂时被占用，正在等待释放：$IywClawFileCheckPath"
  non_empty_file_sleep:
    Sleep ${IYW_CLAW_FILE_CHECK_WAIT_MS}
    IntOp $3 $3 + 1
    Goto non_empty_file_retry
  non_empty_file_success:
    StrCpy $2 "1"
  non_empty_file_done:
    StrCmp $2 "1" non_empty_file_return 0
    Push "file-check: path=$IywClawFileCheckPath; reason=$IywClawFileCheckReason; win32_error=$IywClawFileCheckErrorCode; attempts=$IywClawFileCheckAttempts"
    Call IywClawAppendInstallerLog
  non_empty_file_return:
    StrCpy $0 $2
    Pop $3
    Pop $2
    Pop $1
    Exch $0
FunctionEnd
