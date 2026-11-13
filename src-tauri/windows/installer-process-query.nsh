!macro IywClawWriteProcessQuery
  FileWriteUTF16LE $R0 `  $$script:useNativeProcesses = $$false$\r$\n`
  FileWriteUTF16LE $R0 `  function Get-InstallerProcesses([int]$$ProcessId = -1) {$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not $$script:useNativeProcesses) {$\r$\n`
  FileWriteUTF16LE $R0 `      try {$\r$\n`
  FileWriteUTF16LE $R0 `        $$query = @{ Namespace = 'root/cimv2'; ClassName = 'Win32_Process'; ErrorAction = 'Stop' }$\r$\n`
  FileWriteUTF16LE $R0 `        if ($$ProcessId -ge 0) { $$query.Filter = "ProcessId = $$ProcessId" }$\r$\n`
  FileWriteUTF16LE $R0 `        return @(Get-CimInstance @query)$\r$\n`
  FileWriteUTF16LE $R0 `      } catch {$\r$\n`
  FileWriteUTF16LE $R0 `        $$script:useNativeProcesses = $$true$\r$\n`
  FileWriteUTF16LE $R0 `        $$message = 'CIM process query failed; using native Windows process API: {0}' -f $$_.Exception.Message$\r$\n`
  FileWriteUTF16LE $R0 `        Write-Warning $$message$\r$\n`
  FileWriteUTF16LE $R0 `        try { $$message | Out-File -LiteralPath (Join-Path $$PSScriptRoot 'iyw-claw-process-control.log') -Append -Encoding UTF8 -ErrorAction Stop } catch { Write-Warning ('Unable to save process fallback diagnostics: {0}' -f $$_.Exception.Message) }$\r$\n`
  FileWriteUTF16LE $R0 `      }$\r$\n`
  FileWriteUTF16LE $R0 `    }$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$ProcessId -ge 0) {$\r$\n`
  FileWriteUTF16LE $R0 `      $$record = [IywNativeProcess]::Read($$ProcessId)$\r$\n`
  FileWriteUTF16LE $R0 `      if ($$null -ne $$record) { return $$record }$\r$\n`
  FileWriteUTF16LE $R0 `      return$\r$\n`
  FileWriteUTF16LE $R0 `    }$\r$\n`
  FileWriteUTF16LE $R0 `    return [IywNativeProcess]::List()$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
!macroend
