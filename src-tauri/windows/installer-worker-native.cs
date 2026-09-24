using System;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class IywInstallerNative {
    const uint MessageTimeoutMs = 500;
    const uint SetTextMessage = 0x000C;
    const uint SetProgressMessage = 0x0402;
    const uint AbortIfHung = 0x0002;
    const int StepLabelId = 2400;
    const int StepBarId = 2410;
    static readonly string[] StageNames = { "\u51C6\u5907\u5B89\u88C5", "\u521D\u59CB\u5316\u7EC4\u4EF6", "\u5B8C\u6210\u8BBE\u7F6E" };
    [DllImport("user32.dll", SetLastError=true)]
    static extern IntPtr SendMessageTimeoutW(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", EntryPoint="SendMessageTimeoutW", CharSet=CharSet.Unicode, SetLastError=true)]
    static extern IntPtr SendTextTimeout(IntPtr window, uint message, UIntPtr wparam, string text, uint flags, uint timeout, out UIntPtr result);
    [DllImport("user32.dll", SetLastError=true)]
    static extern bool PostMessageW(IntPtr window, uint message, UIntPtr wparam, IntPtr lparam);
    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr window, out uint processId);
    [DllImport("user32.dll")] static extern IntPtr GetParent(IntPtr window);
    [DllImport("user32.dll")] static extern IntPtr GetDlgItem(IntPtr window, int id);
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
    public static bool Text(long window, string text) {
        if (window == 0) return true;
        UIntPtr result;
        return SendTextTimeout(new IntPtr(window), SetTextMessage, UIntPtr.Zero, text,
            AbortIfHung, MessageTimeoutMs, out result) != IntPtr.Zero && result != UIntPtr.Zero;
    }
    public static bool Show(long bar, long label, int value) {
        if (bar == 0) return true;
        value = Math.Max(0, Math.Min(10000, value));
        // WM_SETTEXT marshals child-control text across process boundaries.
        if (!Text(label, (value / 100) + "%")) return false;
        return PostMessageW(new IntPtr(bar), SetProgressMessage, new UIntPtr((uint)value), IntPtr.Zero);
    }
    public static bool Stage(long window, string phase) {
        if (window == 0) return true;
        int current = phase == "complete" ? 3 : phase == "initialize" ? 2 : 1;
        bool updated = Text(window, current + " / 3   " + StageNames[current - 1]);
        IntPtr parent = GetParent(new IntPtr(window));
        for (int step = 1; step <= StageNames.Length; step++) {
            string prefix = step < current ? "\u2713" : step.ToString("00");
            updated = Text(GetDlgItem(parent, StepLabelId + step).ToInt64(), prefix + "  " + StageNames[step - 1]) && updated;
            IntPtr bar = GetDlgItem(parent, StepBarId + step);
            if (bar != IntPtr.Zero)
                updated = PostMessageW(bar, SetProgressMessage, new UIntPtr(step <= current ? 100u : 0u), IntPtr.Zero) && updated;
        }
        return updated;
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
    public void WriteDiagnostic(string message) {
        lock (gate) { if (!disposed) log.WriteLine(message); }
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
