using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.IO;
using System.Runtime.InteropServices;
using System.Text;

public static class IywInstallerNative {
    const uint MessageTimeoutMs = 500;
    const uint SetTextMessage = 0x000C;
    const uint SetProgressMessage = 0x0402;
    const uint SetMarqueeMessage = 0x040A;
    const int WindowStyleIndex = -16;
    const int MarqueeStyle = 0x8;
    const int MarqueeIntervalMs = 50;
    const uint AbortIfHung = 0x0002;
    const int StepLabelId = 2400;
    const int StepBarId = 2410;
    const int FeedbackId = 2420;
    const int SlowProgressSeconds = 30;
    static readonly string[] StageNames = { "\u51C6\u5907\u5B89\u88C5", "\u521D\u59CB\u5316\u7EC4\u4EF6", "\u5B8C\u6210\u8BBE\u7F6E" };
    static readonly Dictionary<string, string> Activities = new Dictionary<string, string> {
        { "checking", "\u68c0\u67e5\u5df2\u5b89\u88c5\u7ec4\u4ef6" },
        { "resolving", "\u8fde\u63a5\u73af\u5883\u670d\u52a1" },
        { "planning", "\u751f\u6210\u73af\u5883\u8ba1\u5212" },
        { "downloading", "\u4e0b\u8f7d\u73af\u5883\u7ec4\u4ef6" },
        { "extracting", "\u89e3\u538b\u73af\u5883\u7ec4\u4ef6" },
        { "downloaded", "\u51c6\u5907\u73af\u5883\u7ec4\u4ef6" },
        { "preparing", "\u51c6\u5907\u73af\u5883\u7ec4\u4ef6" },
        { "prepared", "\u7b49\u5f85\u5e94\u7528\u6587\u4ef6\u5199\u5165" },
        { "revalidating", "\u6821\u9a8c\u73af\u5883\u7ec4\u4ef6" },
        { "verifying", "\u6821\u9a8c\u73af\u5883\u7ec4\u4ef6" },
        { "reusing", "\u590d\u7528\u5df2\u5b89\u88c5\u7ec4\u4ef6" },
        { "starting", "\u542f\u52a8\u521d\u59cb\u5316\u4efb\u52a1" },
        { "committed", "\u6821\u9a8c\u5e94\u7528\u6587\u4ef6" },
        { "commit", StageNames[2] }
    };
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
    [DllImport("user32.dll")] static extern int GetWindowLongW(IntPtr window, int index);
    [DllImport("user32.dll")] static extern int SetWindowLongW(IntPtr window, int index, int value);
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
        string percent = value < 100 ? "\u51c6\u5907\u4e2d" : (value / 100) + "%";
        if (!Text(label, percent)) return false;
        SetWaiting(new IntPtr(bar), value < 100);
        if (value < 100) return true;
        return PostMessageW(new IntPtr(bar), SetProgressMessage, new UIntPtr((uint)value), IntPtr.Zero);
    }
    static void SetWaiting(IntPtr bar, bool waiting) {
        int style = GetWindowLongW(bar, WindowStyleIndex);
        if (((style & MarqueeStyle) != 0) == waiting) return;
        if (waiting) {
            SetWindowLongW(bar, WindowStyleIndex, style | MarqueeStyle);
            PostMessageW(bar, SetMarqueeMessage, new UIntPtr(1), new IntPtr(MarqueeIntervalMs));
        } else {
            PostMessageW(bar, SetMarqueeMessage, UIntPtr.Zero, IntPtr.Zero);
            SetWindowLongW(bar, WindowStyleIndex, style & ~MarqueeStyle);
        }
    }
    static string CurrentStage(string phase, int current) {
        string activity;
        return Activities.TryGetValue(phase, out activity) ? activity : StageNames[current - 1];
    }
    public static string FeedbackText(string phase, int elapsedSeconds, int idleSeconds) {
        string text = CurrentStage(phase, 1) + "   |   \u5df2\u7528\u65f6 " + elapsedSeconds + " \u79d2";
        if (idleSeconds < SlowProgressSeconds) return text;
        string hint;
        switch (phase) {
            case "resolving":
            case "downloading":
                hint = "\u8bf7\u68c0\u67e5\u7f51\u7edc\u6216\u4ee3\u7406\uff0c\u5e76\u7b49\u5f85\u91cd\u8bd5\u3002"; break;
            case "prepared":
                hint = "\u6b63\u5728\u7b49\u5f85\u5e94\u7528\u6587\u4ef6\u5199\u5165\u5b8c\u6210\u3002"; break;
            default:
                hint = "\u8bf7\u4fdd\u6301\u7a97\u53e3\u6253\u5f00\uff0c\u7b49\u5f85\u5f53\u524d\u6b65\u9aa4\u8fd4\u56de\u3002"; break;
        }
        return text + "\r\n" + idleSeconds + " \u79d2\u65e0\u65b0\u8fdb\u5ea6\u3002" + hint;
    }
    public static bool Feedback(long stageWindow, string text) {
        if (stageWindow == 0) return true;
        IntPtr parent = GetParent(new IntPtr(stageWindow));
        return Text(GetDlgItem(parent, FeedbackId).ToInt64(), text);
    }
    public static string TimeoutText(string phase, int elapsedSeconds) {
        return CurrentStage(phase, 1) + "\u8d85\u65f6\uff08" + elapsedSeconds +
            " \u79d2\uff09\u3002\u8bf7\u68c0\u67e5\u7f51\u7edc\u3001\u78c1\u76d8\u7a7a\u95f4\u548c\u76ee\u5f55\u6743\u9650\u540e\u91cd\u8bd5\u3002";
    }
    public static bool Stage(long window, string phase) {
        if (window == 0) return true;
        bool complete = phase == "complete" || phase == "commit" || phase == "committed";
        bool initialize = phase == "downloading" || phase == "downloaded" ||
            phase == "extracting" || phase == "preparing" || phase == "prepared" ||
            phase == "verifying" || phase == "revalidating" || phase == "reusing";
        int current = complete ? 3 : initialize ? 2 : 1;
        bool updated = Text(window, current + " / 3   " + CurrentStage(phase, current));
        IntPtr parent = GetParent(new IntPtr(window));
        for (int step = 1; step <= StageNames.Length; step++) {
            string prefix = step < current ? "\u2713" : step.ToString("00");
            updated = Text(GetDlgItem(parent, StepLabelId + step).ToInt64(), prefix + "  " + StageNames[step - 1]) && updated;
            IntPtr bar = GetDlgItem(parent, StepBarId + step);
            if (bar != IntPtr.Zero)
                updated = PostMessageW(bar, SetProgressMessage, new UIntPtr(step < current ? 100u : 0u), IntPtr.Zero) && updated;
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
    public string FailureMessage { get; private set; }
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
            StandardOutputEncoding = new UTF8Encoding(false), StandardErrorEncoding = new UTF8Encoding(false),
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
        lock (gate) {
            if (disposed) return;
            log.WriteLine(args.Data);
            if (args.Data.StartsWith("iyw-environment: ", StringComparison.Ordinal))
                FailureMessage = args.Data.Substring("iyw-environment: ".Length);
        }
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
