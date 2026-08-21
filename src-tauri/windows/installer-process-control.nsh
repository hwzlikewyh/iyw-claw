!define IYW_CLAW_PROCESS_WAIT_ATTEMPTS 10
!define IYW_CLAW_PROCESS_WAIT_MS 500

Var IywClawProcessError

!macro IywClawDefineProcessFunctions Prefix
Function ${Prefix}IywClawIssueKnownProcessKills
  Push "kill"
  Call ${Prefix}IywClawRunKnownProcessCommand
  Pop $R0
  StrCmp $R0 "0" known_process_kill_ready 0
  StrCpy $IywClawProcessError "当前安装目录的进程查询或终止失败"
  Push "1"
  Return

  known_process_kill_ready:
  Push "0"
FunctionEnd

Function ${Prefix}IywClawRunKnownProcessCommand
  Pop $R8
  Push "$INSTDIR"
  Push "$R8"
  Call ${Prefix}IywClawRunKnownProcessCommandAt
FunctionEnd

Function ${Prefix}IywClawRunKnownProcessCommandAt
  Pop $R8
  Pop $R9
  Call ${Prefix}IywClawWriteKnownProcessScript
  Pop $R0
  StrCmp $R0 "0" known_process_script_ready known_process_command_failed

  known_process_script_ready:
  nsExec::ExecToLog '"$SYSDIR\WindowsPowerShell\v1.0\powershell.exe" -NoLogo -NoProfile -NonInteractive -ExecutionPolicy Bypass -File "$PLUGINSDIR\iyw-claw-process-control.ps1" -Action "$R8" -InstallDir "$R9"'
  Pop $R0
  Delete "$PLUGINSDIR\iyw-claw-process-control.ps1"
  StrCmp $R0 "0" known_process_command_result_ready 0
  StrCmp $R0 "1" known_process_command_result_ready 0
  StrCmp $R0 "2" known_process_command_result_ready 0
  StrCpy $R0 "2"

  known_process_command_result_ready:
  Push $R0
  Return

  known_process_command_failed:
    Delete "$PLUGINSDIR\iyw-claw-process-control.ps1"
    Push "2"
FunctionEnd

Function ${Prefix}IywClawWriteKnownProcessScript
  InitPluginsDir
  ClearErrors
  FileOpen $R0 "$PLUGINSDIR\iyw-claw-process-control.ps1" w
  IfErrors known_process_script_write_failed 0
  FileWriteUTF16LE /BOM $R0 `param([ValidateSet('kill', 'check', 'check-main', 'check-legacy-files')][string]$$Action, [Parameter(Mandatory = $$true)][string]$$InstallDir)$\r$\n`
  FileWriteUTF16LE $R0 `try {$\r$\n`
  FileWriteUTF16LE $R0 `  $$ErrorActionPreference = 'Stop'$\r$\n`
  FileWriteUTF16LE $R0 `  function Normalize-Directory([string]$$Path) {$\r$\n`
  FileWriteUTF16LE $R0 `    $$full = [IO.Path]::GetFullPath($$Path)$\r$\n`
  FileWriteUTF16LE $R0 `    $$root = [IO.Path]::GetPathRoot($$full)$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::Equals($$full, $$root, [StringComparison]::OrdinalIgnoreCase)) { return $$root }$\r$\n`
  FileWriteUTF16LE $R0 `    return $$full.TrimEnd([IO.Path]::DirectorySeparatorChar, [IO.Path]::AltDirectorySeparatorChar)$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  function Get-SafeCanonicalPath([string]$$Path) {$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$Path)) { throw 'Path is empty' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$full = [IO.Path]::GetFullPath($$Path)$\r$\n`
  FileWriteUTF16LE $R0 `    $$cursor = $$full$\r$\n`
  FileWriteUTF16LE $R0 `    while ($$true) {$\r$\n`
  FileWriteUTF16LE $R0 `      $$item = Get-Item -LiteralPath $$cursor -Force -ErrorAction Stop$\r$\n`
  FileWriteUTF16LE $R0 `      if (($$item.Attributes -band [IO.FileAttributes]::ReparsePoint) -ne 0) { throw "Path contains a symbolic link or junction: $$cursor" }$\r$\n`
  FileWriteUTF16LE $R0 `      $$parentPath = [IO.Path]::GetDirectoryName($$cursor)$\r$\n`
  FileWriteUTF16LE $R0 `      if ([string]::IsNullOrWhiteSpace($$parentPath) -or $$parentPath -eq $$cursor) { break }$\r$\n`
  FileWriteUTF16LE $R0 `      $$cursor = $$parentPath$\r$\n`
  FileWriteUTF16LE $R0 `    }$\r$\n`
  FileWriteUTF16LE $R0 `    return (Get-Item -LiteralPath $$full -Force -ErrorAction Stop).FullName$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  function Get-ProcessIdentity([object]$$CimProcess) {$\r$\n`
  FileWriteUTF16LE $R0 `    $$owner = Invoke-CimMethod -InputObject $$CimProcess -MethodName GetOwnerSid -ErrorAction Stop$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$owner.ReturnValue -ne 0 -or [string]::IsNullOrWhiteSpace($$owner.Sid)) { throw 'Unable to resolve process owner SID' }$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$CimProcess.ExecutablePath)) { throw 'Process executable path is unavailable' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$processId = [int]$$CimProcess.ProcessId$\r$\n`
  FileWriteUTF16LE $R0 `    $$runtime = Get-Process -Id $$processId -ErrorAction Stop$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$runtime.Path)) { throw 'Runtime executable path is unavailable' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$startTicks = $$runtime.StartTime.ToUniversalTime().Ticks$\r$\n`
  FileWriteUTF16LE $R0 `    $$cimPath = Get-SafeCanonicalPath $$CimProcess.ExecutablePath$\r$\n`
  FileWriteUTF16LE $R0 `    $$runtimePath = Get-SafeCanonicalPath $$runtime.Path$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not [string]::Equals($$cimPath, $$runtimePath, [StringComparison]::OrdinalIgnoreCase)) { throw 'CIM and runtime executable identities differ' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$parentPath = [IO.Path]::GetDirectoryName($$cimPath)$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$parentPath)) { throw 'Process executable parent is unavailable' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$parent = Normalize-Directory (Get-SafeCanonicalPath $$parentPath)$\r$\n`
  FileWriteUTF16LE $R0 `    $$leaf = [IO.Path]::GetFileName($$cimPath)$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not [string]::Equals($$leaf, $$CimProcess.Name, [StringComparison]::OrdinalIgnoreCase)) { throw 'Process executable name mismatch' }$\r$\n`
  FileWriteUTF16LE $R0 `    return [PSCustomObject]@{ ProcessId = $$processId; StartTimeUtcTicks = $$startTicks; OwnerSid = $$owner.Sid; ExecutablePath = $$cimPath; ParentPath = $$parent; Name = $$CimProcess.Name; RuntimeProcess = $$runtime }$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  function Confirm-ProcessIdentity([object]$$Expected, [string]$$Target, [string]$$CurrentSid) {$\r$\n`
  FileWriteUTF16LE $R0 `    $$processId = [int]$$Expected.ProcessId$\r$\n`
  FileWriteUTF16LE $R0 `    $$records = @(Get-CimInstance Win32_Process -Filter "ProcessId = $$processId" -ErrorAction Stop)$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$records.Count -eq 0) { return $$null }$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$records.Count -ne 1) { throw "PID $$processId returned multiple process records" }$\r$\n`
  FileWriteUTF16LE $R0 `    try { $$actual = Get-ProcessIdentity $$records[0] } catch { $$stillThere = @(Get-CimInstance Win32_Process -Filter "ProcessId = $$processId" -ErrorAction Stop); if ($$stillThere.Count -eq 0) { return $$null }; throw }$\r$\n`
  FileWriteUTF16LE $R0 `    $$changed = $$actual.ProcessId -ne $$Expected.ProcessId -or $$actual.StartTimeUtcTicks -ne $$Expected.StartTimeUtcTicks -or -not [string]::Equals($$actual.OwnerSid, $$Expected.OwnerSid, [StringComparison]::OrdinalIgnoreCase) -or -not [string]::Equals($$actual.ExecutablePath, $$Expected.ExecutablePath, [StringComparison]::OrdinalIgnoreCase) -or -not [string]::Equals($$actual.ParentPath, $$Expected.ParentPath, [StringComparison]::OrdinalIgnoreCase) -or -not [string]::Equals($$actual.Name, $$Expected.Name, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$changed) { throw "Process identity changed before stop for PID $$processId" }$\r$\n`
  FileWriteUTF16LE $R0 `    $$sameOwner = [string]::Equals($$actual.OwnerSid, $$CurrentSid, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    $$sameParent = [string]::Equals($$actual.ParentPath, $$Target, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not ($$sameOwner -and $$sameParent)) { throw "Process scope changed before stop for PID $$processId" }$\r$\n`
  FileWriteUTF16LE $R0 `    return $$actual.RuntimeProcess$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  $$pattern = '(?i)^iyw-claw-mcp(?:-(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:0|[1-9][0-9]*|[0-9]*[A-Za-z-][0-9A-Za-z-]*))*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?)?\.exe$$'$\r$\n`
  FileWriteUTF16LE $R0 `  if ($$Action -eq 'check-legacy-files') {$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not (Test-Path -LiteralPath $$InstallDir)) { exit 0 }$\r$\n`
  FileWriteUTF16LE $R0 `    $$fileRoot = Get-Item -LiteralPath (Get-SafeCanonicalPath $$InstallDir) -Force -ErrorAction Stop; if (-not $$fileRoot.PSIsContainer) { throw 'Legacy-file check path is not a directory' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$legacy = @(Get-ChildItem -LiteralPath $$fileRoot.FullName -File -Force -ErrorAction Stop | Where-Object { $$_.Name -match $$pattern })$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$legacy.Count -gt 0) { exit 1 }$\r$\n`
  FileWriteUTF16LE $R0 `    exit 0$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  $$targetItem = Get-Item -LiteralPath $$InstallDir -Force -ErrorAction Stop; if (-not $$targetItem.PSIsContainer) { throw 'Install path is not a directory' }$\r$\n`
  FileWriteUTF16LE $R0 `  $$target = Normalize-Directory (Get-SafeCanonicalPath $$InstallDir)$\r$\n`
  FileWriteUTF16LE $R0 `  $$currentSid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value; if ([string]::IsNullOrWhiteSpace($$currentSid)) { throw 'Current user SID is unavailable' }$\r$\n`
  FileWriteUTF16LE $R0 `  $$isCandidate = { param([string]$$Name) if ($$Action -eq 'check-main') { return $$Name -ieq 'iyw-claw.exe' }; return $$Name -ieq 'iyw-claw.exe' -or $$Name -ieq 'agent-browser.exe' -or $$Name -match $$pattern }$\r$\n`
  FileWriteUTF16LE $R0 `  $$processSnapshots = @()$\r$\n`
  FileWriteUTF16LE $R0 `  Get-CimInstance Win32_Process -ErrorAction Stop | ForEach-Object {$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not (& $$isCandidate $$_.Name)) { return }$\r$\n`
  FileWriteUTF16LE $R0 `    $$candidatePid = [int]$$_.ProcessId$\r$\n`
  FileWriteUTF16LE $R0 `    try { $$identity = Get-ProcessIdentity $$_ } catch { $$stillThere = @(Get-CimInstance Win32_Process -Filter "ProcessId = $$candidatePid" -ErrorAction Stop); if ($$stillThere.Count -eq 0) { return }; throw }$\r$\n`
  FileWriteUTF16LE $R0 `    $$sameOwner = [string]::Equals($$identity.OwnerSid, $$currentSid, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    $$sameParent = [string]::Equals($$identity.ParentPath, $$target, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$sameOwner -and $$sameParent) { $$processSnapshots += $$identity }$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  if ($$Action -eq 'kill') {$\r$\n`
  FileWriteUTF16LE $R0 `    foreach ($$snapshot in $$processSnapshots) {$\r$\n`
  FileWriteUTF16LE $R0 `      $$liveProcess = Confirm-ProcessIdentity $$snapshot $$target $$currentSid$\r$\n`
  FileWriteUTF16LE $R0 `      if ($$null -ne $$liveProcess) { $$snapshotPid = [int]$$snapshot.ProcessId; try { Stop-Process -InputObject $$liveProcess -Force -ErrorAction Stop } catch { $$stillThere = @(Get-CimInstance Win32_Process -Filter "ProcessId = $$snapshotPid" -ErrorAction Stop); if ($$stillThere.Count -gt 0) { throw } } }$\r$\n`
  FileWriteUTF16LE $R0 `    }$\r$\n`
  FileWriteUTF16LE $R0 `    exit 0$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  if ($$processSnapshots.Count -gt 0) { exit 1 }$\r$\n`
  FileWriteUTF16LE $R0 `  exit 0$\r$\n`
  FileWriteUTF16LE $R0 `} catch {$\r$\n`
  FileWriteUTF16LE $R0 `  Write-Error $$_ -ErrorAction Continue$\r$\n`
  FileWriteUTF16LE $R0 `  exit 2$\r$\n`
  FileWriteUTF16LE $R0 `}$\r$\n`
  FileClose $R0
  IfErrors known_process_script_write_failed 0
  Push "0"
  Return
  known_process_script_write_failed:
    Delete "$PLUGINSDIR\iyw-claw-process-control.ps1"
    Push "1"
FunctionEnd

Function ${Prefix}IywClawAnyKnownProcessRunning
  Push "check"
  Call ${Prefix}IywClawRunKnownProcessCommand
  Pop $R0
  StrCmp $R0 "0" no_known_process 0
  StrCmp $R0 "1" known_process_running process_check_failed

  known_process_running:
    StrCpy $IywClawProcessError "当前安装目录仍有后台进程"
    Push "1"
    Return

  process_check_failed:
    StrCpy $IywClawProcessError "进程状态检查失败"
    Push "2"
    Return

  no_known_process:
    Push "0"
FunctionEnd

Function ${Prefix}IywClawStopKnownProcesses
  StrCpy $IywClawProcessError ""
  DetailPrint "正在停止当前用户且属于本安装目录的 iyw-claw 后台进程..."
  Call ${Prefix}IywClawIssueKnownProcessKills
  Pop $R5
  StrCmp $R5 "0" issue_process_kills_ready stop_processes_failed

  issue_process_kills_ready:
  StrCpy $R4 0

  wait_for_processes:
    Sleep ${IYW_CLAW_PROCESS_WAIT_MS}
    Call ${Prefix}IywClawAnyKnownProcessRunning
    Pop $R5
    StrCmp $R5 "0" stop_processes_done 0
    StrCmp $R5 "2" stop_processes_failed 0
    IntOp $R4 $R4 + 1
    IntCmp $R4 ${IYW_CLAW_PROCESS_WAIT_ATTEMPTS} stop_processes_timeout retry_process_kill stop_processes_timeout

  retry_process_kill:
    Call ${Prefix}IywClawIssueKnownProcessKills
    Pop $R5
    StrCmp $R5 "0" wait_for_processes stop_processes_failed

  stop_processes_timeout:
    DetailPrint "等待进程退出超时：$IywClawProcessError"
    Push "1"
    Return

  stop_processes_failed:
    StrCmp $IywClawProcessError "" 0 +2
      StrCpy $IywClawProcessError "进程查询或终止失败"
    DetailPrint "无法安全停止进程：$IywClawProcessError"
    Push "1"
    Return

  stop_processes_done:
    DetailPrint "当前用户的 iyw-claw 后台进程已停止。"
    Push "0"
FunctionEnd
!macroend

!insertmacro IywClawDefineProcessFunctions ""
!insertmacro IywClawDefineProcessFunctions "un."
