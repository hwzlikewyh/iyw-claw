using System;
using System.Runtime.InteropServices;

// MSIX 必须在签名调用内加时间戳，并向 AppX SIP 传入相同的签名参数。
// https://learn.microsoft.com/windows/win32/appxpkg/how-to-programmatically-sign-a-package
public static partial class IywWindowsTokenSigner
{
    const uint TimestampRfc3161 = 2;
    const string Sha256Oid = "2.16.840.1.101.3.4.2.1";

    [StructLayout(LayoutKind.Sequential)]
    struct PackageSignerParameters
    {
        public uint Flags;
        public IntPtr Subject, Certificate, Signature, Provider;
        public uint TimestampFlags;
        public IntPtr TimestampAlgorithm, TimestampUrl, Attributes, SipData;
        public IntPtr SignerContext, CryptoPolicy, Reserved;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct AppxSipClientData
    {
        public IntPtr SignerParameters, State;
    }

    [DllImport("mssign32.dll")]
    static extern int SignerSignEx2(uint flags, IntPtr subject, IntPtr certificate,
        IntPtr signature, IntPtr provider, uint timestampFlags, IntPtr algorithm,
        IntPtr timestampUrl, IntPtr attributes, IntPtr sipData, IntPtr context,
        IntPtr cryptoPolicy, IntPtr reserved);

    public static void SignPackage(string file, string thumbprint, string timestampUrl)
    {
        string pin = Environment.GetEnvironmentVariable("IYW_CLAW_SAFENET_PIN");
        if (String.IsNullOrWhiteSpace(pin) || pin.IndexOf('\0') >= 0)
            throw new InvalidOperationException("Signing PIN is missing or invalid");
        if (String.IsNullOrWhiteSpace(timestampUrl))
            throw new InvalidOperationException("MSIX signing requires a timestamp server");
        using (var certificate = OpenCertificate(thumbprint))
        {
            Authenticate(certificate.Handle, pin);
            SignPackageFile(file, certificate.Handle, timestampUrl);
        }
        Console.WriteLine("[signing-token] MSIX signature and RFC3161 timestamp completed");
    }

    static void SignPackageFile(string file, IntPtr certificate, string timestampUrl)
    {
        using (var memory = new SigningMemory())
        {
            var fileInfo = new SignerFileInfo { Size = (uint)Marshal.SizeOf(typeof(SignerFileInfo)), FileName = memory.Text(file) };
            var subject = new SignerSubjectInfo { Size = (uint)Marshal.SizeOf(typeof(SignerSubjectInfo)),
                Index = memory.Keep(0u), Choice = SignerSubjectFile, FileInfo = memory.Keep(fileInfo) };
            var store = new SignerCertificateInfo { Size = (uint)Marshal.SizeOf(typeof(SignerCertificateInfo)),
                Certificate = certificate, Policy = CertificateChainWithoutRoot };
            var signer = new SignerCertificate { Size = (uint)Marshal.SizeOf(typeof(SignerCertificate)),
                Choice = SignerCertificateStore, StoreInfo = memory.Keep(store) };
            var signature = new SignerSignatureInfo { Size = (uint)Marshal.SizeOf(typeof(SignerSignatureInfo)), Algorithm = Sha256 };
            var parameters = new PackageSignerParameters { Subject = memory.Keep(subject),
                Certificate = memory.Keep(signer), Signature = memory.Keep(signature),
                TimestampFlags = TimestampRfc3161, TimestampUrl = memory.Text(timestampUrl),
                SignerContext = memory.Keep(IntPtr.Zero) };
            SignPackageParameters(parameters, memory);
        }
    }

    static void SignPackageParameters(PackageSignerParameters parameters, SigningMemory memory)
    {
        IntPtr algorithm = Marshal.StringToHGlobalAnsi(Sha256Oid);
        IntPtr sip = IntPtr.Zero;
        try
        {
            parameters.TimestampAlgorithm = algorithm;
            IntPtr block = memory.Keep(parameters);
            sip = memory.Keep(new AppxSipClientData { SignerParameters = block });
            parameters.SipData = sip;
            Marshal.StructureToPtr(parameters, block, false);
            int status = SignerSignEx2(parameters.Flags, parameters.Subject, parameters.Certificate,
                parameters.Signature, parameters.Provider, parameters.TimestampFlags,
                parameters.TimestampAlgorithm, parameters.TimestampUrl, parameters.Attributes,
                sip, parameters.SignerContext, parameters.CryptoPolicy, parameters.Reserved);
            if (status != 0) throw NativeError("MSIX signing with timestamp", status);
        }
        finally
        {
            IntPtr context = Marshal.ReadIntPtr(parameters.SignerContext);
            if (context != IntPtr.Zero) SignerFreeSignerContext(context);
            if (sip != IntPtr.Zero)
            {
                IntPtr state = Marshal.PtrToStructure<AppxSipClientData>(sip).State;
                if (state != IntPtr.Zero) Marshal.Release(state);
            }
            Marshal.FreeHGlobal(algorithm);
        }
    }
}
