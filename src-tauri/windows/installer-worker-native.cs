using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class IywInstallerNative {
    const uint MessageTimeoutMs = 500;
    [DllImport("user32.dll", SetLastError=true)]
    static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", CharSet=CharSet.Unicode)]
    static extern bool SetWindowTextW(IntPtr window, string text);
    [DllImport("user32.dll", SetLastError=true)]
    static extern bool PostMessageW(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("kernel32.dll", CharSet=CharSet.Unicode)]
    static extern uint GetPrivateProfileStringW(string section, string key, string fallback, StringBuilder value, uint size, string path);
    public static uint Message(long window, uint message, uint value) {
        UIntPtr result;
        SendMessageTimeoutW(new IntPtr(window), message, new UIntPtr(value), IntPtr.Zero, 2, MessageTimeoutMs, out result);
        return (uint)result.ToUInt64();
    }
    public static string Read(string path, string key) {
        var value = new StringBuilder(256);
        GetPrivateProfileStringW("progress", key, "", value, (uint)value.Capacity, path);
        return value.ToString();
    }
    public static void Show(long bar, long label, int value) {
        if (bar == 0) return;
        if (!PostMessageW(new IntPtr(bar), 0x402, new UIntPtr((uint)value), IntPtr.Zero))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        SetWindowTextW(new IntPtr(label), (value / 100) + "%");
    }
}

public sealed class IywInstallerChild : IDisposable {
    const uint KillOnJobClose = 0x2000;
    [StructLayout(LayoutKind.Sequential)] struct Limits {
        public long ProcessTime, JobTime;
        public uint Flags;
        public UIntPtr MinWorkingSet, MaxWorkingSet;
        public uint ProcessLimit;
        public UIntPtr Affinity;
        public uint Priority, Scheduling;
    }
    [StructLayout(LayoutKind.Sequential)] struct IoCounters {
        public ulong ReadOperations, WriteOperations, OtherOperations, ReadBytes, WriteBytes, OtherBytes;
    }
    [StructLayout(LayoutKind.Sequential)] struct ExtendedLimits {
        public Limits Basic;
        public IoCounters Io;
        public UIntPtr ProcessMemory, JobMemory, PeakProcessMemory, PeakJobMemory;
    }
    [DllImport("kernel32.dll", SetLastError=true)] static extern IntPtr CreateJobObjectW(IntPtr security, IntPtr name);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool SetInformationJobObject(IntPtr job, int kind, ref ExtendedLimits limits, uint size);
    [DllImport("kernel32.dll", SetLastError=true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr handle);
    readonly StreamWriter log;
    readonly object gate = new object();
    readonly Process process;
    bool disposed;
    IntPtr job;
    public IywInstallerChild(string executable, string arguments, string logPath) {
        log = new StreamWriter(logPath, true, new UTF8Encoding(false));
        log.AutoFlush = true;
        process = new Process();
        try { Start(executable, arguments); }
        catch { Dispose(); throw; }
    }
    void Start(string executable, string arguments) {
        job = CreateJobObjectW(IntPtr.Zero, IntPtr.Zero);
        var limits = new ExtendedLimits();
        limits.Basic.Flags = KillOnJobClose;
        if (job == IntPtr.Zero || !SetInformationJobObject(job, 9, ref limits, (uint)Marshal.SizeOf(limits)))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        process.StartInfo = new ProcessStartInfo(executable, arguments) {
            UseShellExecute = false, CreateNoWindow = true,
            RedirectStandardOutput = true, RedirectStandardError = true,
            WorkingDirectory = Path.GetDirectoryName(executable)
        };
        process.OutputDataReceived += OnOutput;
        process.ErrorDataReceived += OnOutput;
        process.Start();
        if (!AssignProcessToJobObject(job, process.Handle) && !process.HasExited) {
            int error = Marshal.GetLastWin32Error();
            process.Kill();
            throw new System.ComponentModel.Win32Exception(error);
        }
        process.BeginOutputReadLine();
        process.BeginErrorReadLine();
    }
    void OnOutput(object sender, DataReceivedEventArgs args) {
        if (args.Data == null) return;
        lock (gate) { if (!disposed) log.WriteLine(args.Data); }
    }
    public bool HasExited { get { return process.HasExited; } }
    public int Finish() { process.WaitForExit(); return process.ExitCode; }
    public void Dispose() {
        if (job != IntPtr.Zero) { CloseHandle(job); job = IntPtr.Zero; }
        try { if (!process.HasExited) process.WaitForExit(10000); } catch (InvalidOperationException) { }
        process.Dispose();
        lock (gate) { disposed = true; log.Dispose(); }
    }
}
