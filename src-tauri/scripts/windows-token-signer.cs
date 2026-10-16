using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using System.Security.Cryptography.X509Certificates;
using System.Text;

// PIN 认证和 Authenticode 签名共用证书缓存的密钥句柄，禁止交互式回退。
public static class IywWindowsTokenSigner
{
    const uint CertKeyProviderInfo = 2;
    const uint CacheKeyContext = 0x00000001;
    const uint SilentContext = 0x00000040;
    const uint InhibitPersistence = 0x40000000;
    const uint AcquireNoHealing = 0x00000008;
    const uint AcquireAllowNCrypt = 0x00010000;
    const uint NCryptKeySpec = 0xffffffff;
    const uint KeyExchangeSpec = 1;
    const uint SignatureSpec = 2;
    const uint KeyExchangePin = 32;
    const uint SignaturePin = 33;
    const uint SignerSubjectFile = 1;
    const uint SignerCertificateStore = 2;
    const uint CertificateChainWithoutRoot = 8;
    const uint CertificateEncoding = 0x00010001;
    const uint Sha256 = 0x0000800c;
    const uint Sha384 = 0x0000800d;
    const uint Sha512 = 0x0000800e;

    [StructLayout(LayoutKind.Sequential)]
    struct KeyProviderInfo
    {
        public IntPtr Container, Provider;
        public uint ProviderType, Flags, ParameterCount;
        public IntPtr Parameters;
        public uint KeySpec;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct SignerFileInfo
    {
        public uint Size;
        public IntPtr FileName, FileHandle;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct SignerSubjectInfo
    {
        public uint Size;
        public IntPtr Index;
        public uint Choice;
        public IntPtr FileInfo;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct SignerCertificateInfo
    {
        public uint Size;
        public IntPtr Certificate;
        public uint Policy;
        public IntPtr Store;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct SignerCertificate
    {
        public uint Size, Choice;
        public IntPtr StoreInfo, Window;
    }

    [StructLayout(LayoutKind.Sequential)]
    struct SignerSignatureInfo
    {
        public uint Size, Algorithm, AttributeChoice;
        public IntPtr Authcode, Authenticated, Unauthenticated;
    }

    [DllImport("crypt32.dll", SetLastError = true)]
    static extern IntPtr CertCreateCertificateContext(uint encoding, byte[] data, uint size);
    [DllImport("crypt32.dll")]
    static extern bool CertFreeCertificateContext(IntPtr certificate);
    [DllImport("crypt32.dll", SetLastError = true)]
    static extern bool CertGetCertificateContextProperty(IntPtr certificate, uint property, IntPtr data, ref uint size);
    [DllImport("crypt32.dll", SetLastError = true)]
    static extern bool CertSetCertificateContextProperty(IntPtr certificate, uint property, uint flags, IntPtr data);
    [DllImport("crypt32.dll", SetLastError = true)]
    static extern bool CryptAcquireCertificatePrivateKey(IntPtr certificate, uint flags, IntPtr parameters, out IntPtr key, out uint keySpec, out bool callerOwnsHandle);
    [DllImport("advapi32.dll", SetLastError = true)]
    static extern bool CryptSetProvParam(IntPtr provider, uint parameter, byte[] data, uint flags);
    [DllImport("ncrypt.dll", CharSet = CharSet.Unicode)]
    static extern int NCryptSetProperty(IntPtr key, string property, byte[] data, int size, uint flags);
    [DllImport("mssign32.dll")]
    static extern int SignerSignEx(uint flags, ref SignerSubjectInfo subject, ref SignerCertificate certificate, ref SignerSignatureInfo signature, IntPtr provider, IntPtr timestamp, IntPtr attributes, IntPtr sipData, out IntPtr context);
    [DllImport("mssign32.dll")]
    static extern int SignerFreeSignerContext(IntPtr context);

    public static void Sign(string file, string thumbprint, string digest)
    {
        string pin = Environment.GetEnvironmentVariable("IYW_CLAW_SAFENET_PIN");
        if (String.IsNullOrWhiteSpace(pin) || pin.IndexOf('\0') >= 0)
            throw new InvalidOperationException("IYW_CLAW_SAFENET_PIN is missing or invalid; interactive signing is disabled");
        if (!File.Exists(file)) throw new FileNotFoundException("Signing input does not exist", file);
        uint algorithm = ResolveDigest(digest);
        using (var certificate = OpenCertificate(thumbprint))
        {
            Authenticate(certificate.Handle, pin);
            SignFile(file, certificate.Handle, algorithm);
        }
        Console.WriteLine("[signing-token] Authenticode signing completed without interactive login");
    }

    static uint ResolveDigest(string digest)
    {
        switch (digest.ToLowerInvariant())
        {
            case "sha256": return Sha256;
            case "sha384": return Sha384;
            case "sha512": return Sha512;
            default: throw new InvalidOperationException("Token signing requires sha256, sha384 or sha512");
        }
    }

    static X509Certificate2 OpenCertificate(string thumbprint)
    {
        using (var store = new X509Store(StoreName.My, StoreLocation.CurrentUser))
        {
            store.Open(OpenFlags.ReadOnly | OpenFlags.OpenExistingOnly);
            var matches = store.Certificates.Find(X509FindType.FindByThumbprint, thumbprint, false);
            if (matches.Count != 1)
                throw new InvalidOperationException("Expected one signing certificate in CurrentUser\\My");
            using (var source = matches[0])
            {
                if (DateTime.Now < source.NotBefore || DateTime.Now > source.NotAfter)
                    throw new InvalidOperationException("Signing certificate is outside its validity period");
                return CopyCertificate(source);
            }
        }
    }

    static X509Certificate2 CopyCertificate(X509Certificate2 source)
    {
        // 使用脱离证书存储的上下文，兼容 Windows PowerShell 和新版 .NET。
        byte[] raw = source.RawData;
        IntPtr context = CertCreateCertificateContext(CertificateEncoding, raw, (uint)raw.Length);
        Check(context != IntPtr.Zero, "Create detached certificate context");
        X509Certificate2 certificate = null;
        try
        {
            certificate = new X509Certificate2(context);
            CopyProvider(source.Handle, certificate.Handle);
            return certificate;
        }
        catch
        {
            if (certificate != null) certificate.Dispose();
            throw;
        }
        finally { CertFreeCertificateContext(context); }
    }

    static void CopyProvider(IntPtr source, IntPtr destination)
    {
        uint size = 0;
        Check(CertGetCertificateContextProperty(source, CertKeyProviderInfo, IntPtr.Zero, ref size), "Read certificate provider size");
        IntPtr buffer = Marshal.AllocHGlobal((int)size);
        try
        {
            Check(CertGetCertificateContextProperty(source, CertKeyProviderInfo, buffer, ref size), "Read certificate provider");
            var info = Marshal.PtrToStructure<KeyProviderInfo>(buffer);
            info.Flags |= CacheKeyContext | SilentContext;
            Marshal.StructureToPtr(info, buffer, false);
            Check(CertSetCertificateContextProperty(destination, CertKeyProviderInfo, InhibitPersistence, buffer), "Configure silent certificate provider");
            Console.WriteLine("[signing-token] provider=" + Marshal.PtrToStringUni(info.Provider) + "; keySpec=" + info.KeySpec + "; interactive login disabled");
        }
        finally { Marshal.FreeHGlobal(buffer); }
    }

    static void Authenticate(IntPtr certificate, string pin)
    {
        IntPtr key;
        uint keySpec;
        bool callerOwnsHandle;
        uint flags = CacheKeyContext | SilentContext | AcquireNoHealing | AcquireAllowNCrypt;
        // CACHE_FLAG 将句柄交由证书上下文管理，证书释放前始终保持有效。
        Check(CryptAcquireCertificatePrivateKey(certificate, flags, IntPtr.Zero, out key, out keySpec, out callerOwnsHandle), "Acquire signing key silently");
        byte[] bytes = EncodePin(pin, keySpec);
        try
        {
            if (keySpec == NCryptKeySpec)
            {
                int status = NCryptSetProperty(key, "SmartCardPin", bytes, bytes.Length, 0);
                if (status != 0) throw NativeError("Set signing key PIN", status);
            }
            else
            {
                uint parameter = keySpec == KeyExchangeSpec ? KeyExchangePin : SignaturePin;
                Check(CryptSetProvParam(key, parameter, bytes, 0), "Set signing provider PIN");
            }
        }
        finally { Array.Clear(bytes, 0, bytes.Length); }
    }

    static byte[] EncodePin(string pin, uint keySpec)
    {
        if (keySpec == NCryptKeySpec) return Encoding.Unicode.GetBytes(pin + "\0");
        if (keySpec != KeyExchangeSpec && keySpec != SignatureSpec)
            throw new InvalidOperationException("Unsupported certificate key specification");
        foreach (char character in pin)
            if (character > 0x7f)
                throw new InvalidOperationException("The CSP requires an ASCII PIN");
        return Encoding.ASCII.GetBytes(pin + "\0");
    }

    static void SignFile(string file, IntPtr certificate, uint algorithm)
    {
        using (var memory = new SigningMemory())
        {
            var fileInfo = new SignerFileInfo {
                Size = (uint)Marshal.SizeOf(typeof(SignerFileInfo)),
                FileName = memory.Text(file)
            };
            var subject = new SignerSubjectInfo {
                Size = (uint)Marshal.SizeOf(typeof(SignerSubjectInfo)),
                Index = memory.Keep(0u), Choice = SignerSubjectFile,
                FileInfo = memory.Keep(fileInfo)
            };
            var storeInfo = new SignerCertificateInfo {
                Size = (uint)Marshal.SizeOf(typeof(SignerCertificateInfo)),
                Certificate = certificate, Policy = CertificateChainWithoutRoot
            };
            var signer = new SignerCertificate {
                Size = (uint)Marshal.SizeOf(typeof(SignerCertificate)),
                Choice = SignerCertificateStore, StoreInfo = memory.Keep(storeInfo)
            };
            var signature = new SignerSignatureInfo {
                Size = (uint)Marshal.SizeOf(typeof(SignerSignatureInfo)),
                Algorithm = algorithm
            };
            IntPtr context = IntPtr.Zero;
            Console.WriteLine("[signing-token] signing with the authenticated certificate context");
            try
            {
                int status = SignerSignEx(0, ref subject, ref signer, ref signature, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, IntPtr.Zero, out context);
                if (status != 0) throw NativeError("Authenticode signing", status);
            }
            finally { if (context != IntPtr.Zero) SignerFreeSignerContext(context); }
        }
    }

    static void Check(bool success, string operation)
    {
        if (!success) throw NativeError(operation, Marshal.GetLastWin32Error());
    }

    static Exception NativeError(string operation, int code)
    {
        return new InvalidOperationException(operation + " failed (0x" + ((uint)code).ToString("x8") + "): " + new Win32Exception(code).Message);
    }

    sealed class SigningMemory : IDisposable
    {
        readonly List<IntPtr> allocations = new List<IntPtr>();
        public IntPtr Keep<T>(T value) where T : struct
        {
            IntPtr pointer = Marshal.AllocHGlobal(Marshal.SizeOf(typeof(T)));
            allocations.Add(pointer);
            Marshal.StructureToPtr(value, pointer, false);
            return pointer;
        }
        public IntPtr Text(string value)
        {
            IntPtr pointer = Marshal.StringToHGlobalUni(value);
            allocations.Add(pointer);
            return pointer;
        }
        public void Dispose()
        {
            foreach (IntPtr pointer in allocations) Marshal.FreeHGlobal(pointer);
        }
    }
}
