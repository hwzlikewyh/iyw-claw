using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Principal;
using System.Text;
using Microsoft.Win32.SafeHandles;

public sealed class IywNativeProcessRecord
{
    public int ProcessId;
    public string Name;
    public string ExecutablePath;
    public DateTime CreationDate;
    public string OwnerSid;
    public string CommandLine;
    public string QueryError;
}

public static class IywNativeProcess
{
    const uint QueryLimitedInformation = 0x1000;
    const uint TokenQuery = 0x0008;
    const int InvalidParameter = 87;
    const int MaximumPathCharacters = 32768;

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern SafeProcessHandle OpenProcess(uint access, bool inherit, int id);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool QueryFullProcessImageNameW(SafeProcessHandle process,
        uint flags, StringBuilder path, ref int length);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool GetProcessTimes(SafeProcessHandle process, out long created,
        out long exited, out long kernel, out long user);

    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool OpenProcessToken(SafeProcessHandle process, uint access,
        out IntPtr token);

    [DllImport("kernel32.dll")]
    static extern bool CloseHandle(IntPtr handle);

    public static IywNativeProcessRecord[] List()
    {
        var records = new List<IywNativeProcessRecord>();
        foreach (var process in Process.GetProcesses())
        {
            using (process)
            {
                var record = ReadProcess(process);
                if (record != null) records.Add(record);
            }
        }
        return records.ToArray();
    }

    public static IywNativeProcessRecord Read(int processId)
    {
        try
        {
            using (var process = Process.GetProcessById(processId))
                return ReadProcess(process);
        }
        catch (ArgumentException) { return null; }
    }

    static IywNativeProcessRecord ReadProcess(Process process)
    {
        var record = new IywNativeProcessRecord { ProcessId = process.Id };
        try
        {
            record.Name = process.ProcessName + ".exe";
            using (var handle = OpenProcess(QueryLimitedInformation, false, process.Id))
            {
                if (handle.IsInvalid)
                {
                    var error = Marshal.GetLastWin32Error();
                    if (error == InvalidParameter) return null;
                    throw new Win32Exception(error);
                }
                var path = new StringBuilder(MaximumPathCharacters);
                var length = path.Capacity;
                if (!QueryFullProcessImageNameW(handle, 0, path, ref length))
                    throw new Win32Exception(Marshal.GetLastWin32Error());
                record.ExecutablePath = path.ToString();
                record.Name = Path.GetFileName(record.ExecutablePath);
                record.CreationDate = ReadCreationTime(handle);
                record.OwnerSid = ReadOwner(handle);
                if (String.Equals(record.Name, "node.exe", StringComparison.OrdinalIgnoreCase))
                    ReadCommandLine(record);
            }
        }
        catch (Exception error) { record.QueryError = error.Message; }
        return record;
    }

    static void ReadCommandLine(IywNativeProcessRecord record)
    {
        using (var identity = WindowsIdentity.GetCurrent())
        {
            if (String.Equals(record.OwnerSid, identity.User.Value, StringComparison.Ordinal))
                record.CommandLine = IywNativeCommandLine.Read(record.ProcessId);
        }
    }

    static DateTime ReadCreationTime(SafeProcessHandle process)
    {
        long created, exited, kernel, user;
        if (!GetProcessTimes(process, out created, out exited, out kernel, out user))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        return DateTime.FromFileTimeUtc(created);
    }

    static string ReadOwner(SafeProcessHandle process)
    {
        IntPtr token;
        if (!OpenProcessToken(process, TokenQuery, out token))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        try
        {
            using (var identity = new WindowsIdentity(token)) return identity.User.Value;
        }
        finally { CloseHandle(token); }
    }
}

// 仅为 Node 入口归属读取命令行；内部结构不可用时保留错误，不猜测入口。
static class IywNativeCommandLine
{
    const uint QueryInformationAndRead = 0x0410;
    const int ProcessBasicInformation = 0;
    const int ProcessWow64Information = 26;
    const int PebParameters32 = 0x10;
    const int PebParameters64 = 0x20;
    const int CommandLine32 = 0x40;
    const int CommandLine64 = 0x70;
    const int UnicodeHeader32 = 8;
    const int UnicodeHeader64 = 16;
    const int MaximumCommandLineBytes = 65534;

    [UnmanagedFunctionPointer(CallingConvention.Winapi)]
    delegate int QueryInformation(SafeProcessHandle process, int kind,
        IntPtr buffer, int length, out int written);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern SafeProcessHandle OpenProcess(uint access, bool inherit, int id);

    [DllImport("kernel32.dll", SetLastError = true)]
    static extern bool ReadProcessMemory(SafeProcessHandle process, IntPtr address,
        byte[] buffer, IntPtr length, out IntPtr read);

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr GetModuleHandleW(string name);

    [DllImport("kernel32.dll", CharSet = CharSet.Ansi, ExactSpelling = true,
        SetLastError = true)]
    static extern IntPtr GetProcAddress(IntPtr module, string name);

    public static string Read(int processId)
    {
        using (var process = OpenProcess(QueryInformationAndRead, false, processId))
        {
            if (process.IsInvalid) throw new Win32Exception(Marshal.GetLastWin32Error());
            var module = GetModuleHandleW("ntdll.dll");
            var address = GetProcAddress(module, "NtQueryInformationProcess");
            if (address == IntPtr.Zero) throw new Win32Exception(Marshal.GetLastWin32Error());
            var query = (QueryInformation)Marshal.GetDelegateForFunctionPointer(
                address, typeof(QueryInformation));
            var pointerSize = IntPtr.Size;
            var peb = FindPeb(process, query, ref pointerSize);
            var parametersOffset = pointerSize == 8 ? PebParameters64 : PebParameters32;
            var parameters = ReadPointer(ReadBytes(process,
                IntPtr.Add(peb, parametersOffset), pointerSize), 0, pointerSize);
            var commandOffset = pointerSize == 8 ? CommandLine64 : CommandLine32;
            var headerSize = pointerSize == 8 ? UnicodeHeader64 : UnicodeHeader32;
            var header = ReadBytes(process, IntPtr.Add(parameters, commandOffset), headerSize);
            var length = BitConverter.ToUInt16(header, 0);
            if (length == 0) return String.Empty;
            if (length > MaximumCommandLineBytes || length % sizeof(char) != 0)
                throw new InvalidOperationException("Invalid Node command-line length");
            var text = ReadPointer(header, pointerSize, pointerSize);
            return Encoding.Unicode.GetString(ReadBytes(process, text, length));
        }
    }

    static IntPtr FindPeb(SafeProcessHandle process, QueryInformation query,
        ref int pointerSize)
    {
        // 安装器在 64 位系统使用原生 PowerShell，允许查询 32/64 位 Node。
        if (Environment.Is64BitOperatingSystem && IntPtr.Size != 8)
            throw new InvalidOperationException("Native PowerShell is required for Node inspection");
        var buffer = Marshal.AllocHGlobal(IntPtr.Size * 6);
        try
        {
            int written;
            if (IntPtr.Size == 8)
            {
                CheckStatus(query(process, ProcessWow64Information, buffer, IntPtr.Size, out written));
                var wowPeb = Marshal.ReadIntPtr(buffer);
                if (wowPeb != IntPtr.Zero) { pointerSize = 4; return wowPeb; }
            }
            CheckStatus(query(process, ProcessBasicInformation, buffer, IntPtr.Size * 6, out written));
            return Marshal.ReadIntPtr(buffer, IntPtr.Size);
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    static byte[] ReadBytes(SafeProcessHandle process, IntPtr address, int length)
    {
        var buffer = new byte[length];
        IntPtr read;
        if (!ReadProcessMemory(process, address, buffer, new IntPtr(length), out read))
            throw new Win32Exception(Marshal.GetLastWin32Error());
        if (read.ToInt64() != length) throw new InvalidOperationException("Incomplete process memory read");
        return buffer;
    }

    static IntPtr ReadPointer(byte[] bytes, int offset, int pointerSize)
    {
        return pointerSize == 8 ? new IntPtr(BitConverter.ToInt64(bytes, offset))
            : new IntPtr((long)BitConverter.ToUInt32(bytes, offset));
    }

    static void CheckStatus(int status)
    {
        if (status < 0) throw new InvalidOperationException(
            "NtQueryInformationProcess failed: 0x" + status.ToString("X8"));
    }
}
