!macro IywClawWriteNodeProcessScope
  FileWriteUTF16LE $R0 `  Add-Type -TypeDefinition @'$\r$\n`
  FileWriteUTF16LE $R0 `using System;$\r$\n`
  FileWriteUTF16LE $R0 `using System.ComponentModel;$\r$\n`
  FileWriteUTF16LE $R0 `using System.Runtime.InteropServices;$\r$\n`
  FileWriteUTF16LE $R0 `public static class IywClawCommandLine {$\r$\n`
  FileWriteUTF16LE $R0 `  [DllImport("shell32.dll", CharSet = CharSet.Unicode, SetLastError = true)]$\r$\n`
  FileWriteUTF16LE $R0 `  private static extern IntPtr CommandLineToArgvW(string command, out int count);$\r$\n`
  FileWriteUTF16LE $R0 `  [DllImport("kernel32.dll")] private static extern IntPtr LocalFree(IntPtr value);$\r$\n`
  FileWriteUTF16LE $R0 `  public static string[] Split(string command) {$\r$\n`
  FileWriteUTF16LE $R0 `    int count;$\r$\n`
  FileWriteUTF16LE $R0 `    IntPtr argv = CommandLineToArgvW(command, out count);$\r$\n`
  FileWriteUTF16LE $R0 `    if (argv == IntPtr.Zero) { throw new Win32Exception(Marshal.GetLastWin32Error()); }$\r$\n`
  FileWriteUTF16LE $R0 `    try {$\r$\n`
  FileWriteUTF16LE $R0 `      string[] arguments = new string[count];$\r$\n`
  FileWriteUTF16LE $R0 `      for (int index = 0; index < count; index++) {$\r$\n`
  FileWriteUTF16LE $R0 `        arguments[index] = Marshal.PtrToStringUni(Marshal.ReadIntPtr(argv, index * IntPtr.Size));$\r$\n`
  FileWriteUTF16LE $R0 `      }$\r$\n`
  FileWriteUTF16LE $R0 `      return arguments;$\r$\n`
  FileWriteUTF16LE $R0 `    } finally { LocalFree(argv); }$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `}$\r$\n`
  FileWriteUTF16LE $R0 `'@$\r$\n`
  FileWriteUTF16LE $R0 `  function Get-NodeEntryPoint([string]$$CommandLine) {$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$CommandLine)) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$arguments = [IywClawCommandLine]::Split($$CommandLine)$\r$\n`
  FileWriteUTF16LE $R0 `    $$index = 1$\r$\n`
  FileWriteUTF16LE $R0 `    while ($$index -lt $$arguments.Length) {$\r$\n`
  FileWriteUTF16LE $R0 `      $$argument = $$arguments[$$index]$\r$\n`
  FileWriteUTF16LE $R0 `      if ($$argument -ceq '--') { $$index++; break }$\r$\n`
  FileWriteUTF16LE $R0 `      if (-not $$argument.StartsWith('-')) { break }$\r$\n`
  FileWriteUTF16LE $R0 `      if ($$argument -cnotmatch '^--(?:no-warnings|no-deprecation|enable-source-maps|trace-warnings|trace-deprecation)$$' -and $$argument -cnotmatch '^--(?:max-old-space-size|stack-size)=[0-9]+$$') { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `      $$index++$\r$\n`
  FileWriteUTF16LE $R0 `    }$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$index -ge $$arguments.Length) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    return $$arguments[$$index].Replace('/', '\')$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
  FileWriteUTF16LE $R0 `  function Get-ManagedNodeScript([object]$$Process) {$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$Action -eq 'check-main' -or $$Process.Name -ine 'node.exe') { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$script = Get-NodeEntryPoint $$Process.CommandLine$\r$\n`
  FileWriteUTF16LE $R0 `    if ([string]::IsNullOrWhiteSpace($$script)) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$root = [IO.Path]::GetPathRoot($$script)$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$root.Length -lt 3 -or -not $$root.EndsWith('\')) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    if ([IO.Path]::GetExtension($$script) -notin @('.js', '.cjs', '.mjs')) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$full = [IO.Path]::GetFullPath($$script)$\r$\n`
  FileWriteUTF16LE $R0 `    $$inApp = $$full.StartsWith($$target + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)$\r$\n`
  FileWriteUTF16LE $R0 `    if (-not ($$inApp -or (Test-ManagedProcessPath $$full))) { return '' }$\r$\n`
  FileWriteUTF16LE $R0 `    $$item = Get-Item -LiteralPath (Get-SafeCanonicalPath $$full) -Force -ErrorAction Stop$\r$\n`
  FileWriteUTF16LE $R0 `    if ($$item.PSIsContainer) { throw 'Node entry point is not a file' }$\r$\n`
  FileWriteUTF16LE $R0 `    return $$item.FullName$\r$\n`
  FileWriteUTF16LE $R0 `  }$\r$\n`
!macroend
