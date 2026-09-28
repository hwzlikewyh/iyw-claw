using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Runtime.InteropServices;
using System.Text;

[StructLayout(LayoutKind.Sequential)]
public struct IywRmUniqueProcess
{
    public int ProcessId;
    public System.Runtime.InteropServices.ComTypes.FILETIME StartTime;
}

[StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
public struct IywRmProcessInfo
{
    public IywRmUniqueProcess Process;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 256)] public string AppName;
    [MarshalAs(UnmanagedType.ByValTStr, SizeConst = 64)] public string ServiceName;
    public uint AppType;
    public uint AppStatus;
    public uint SessionId;
    [MarshalAs(UnmanagedType.Bool)] public bool Restartable;
}

public static class IywRestartManager
{
    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    static extern int RmStartSession(out uint handle, int flags, StringBuilder key);

    [DllImport("rstrtmgr.dll", CharSet = CharSet.Unicode)]
    static extern int RmRegisterResources(uint handle, uint files, string[] paths,
        uint apps, IntPtr processes, uint services, string[] names);

    [DllImport("rstrtmgr.dll")]
    static extern int RmGetList(uint handle, out uint needed, ref uint count,
        [In, Out] IywRmProcessInfo[] info, out uint reason);

    [DllImport("rstrtmgr.dll")]
    static extern int RmEndSession(uint handle);

    public static int[] FindPids(string[] paths)
    {
        var result = new HashSet<int>();
        for (var offset = 0; offset < paths.Length; offset += 256)
        {
            var length = Math.Min(256, paths.Length - offset);
            var batch = new string[length];
            Array.Copy(paths, offset, batch, 0, length);
            uint session;
            var error = RmStartSession(out session, 0, new StringBuilder(64));
            if (error != 0) throw new Win32Exception(error);
            try
            {
                error = RmRegisterResources(session, (uint)batch.Length, batch, 0,
                    IntPtr.Zero, 0, null);
                if (error != 0) throw new Win32Exception(error);
                uint needed;
                uint count = 0;
                uint reason;
                error = RmGetList(session, out needed, ref count, null, out reason);
                if (error != 234 && error != 0) throw new Win32Exception(error);
                if (needed == 0) continue;
                var info = new IywRmProcessInfo[needed];
                count = needed;
                error = RmGetList(session, out needed, ref count, info, out reason);
                if (error != 0) throw new Win32Exception(error);
                for (var index = 0; index < count; index++)
                    result.Add(info[index].Process.ProcessId);
            }
            finally
            {
                RmEndSession(session);
            }
        }
        var values = new int[result.Count];
        result.CopyTo(values);
        return values;
    }
}
