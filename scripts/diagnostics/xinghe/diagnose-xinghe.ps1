#requires -Version 5.1
[CmdletBinding()]
param(
    [string]$ApiBaseUrl = 'https://gateway.iyw.cn/iyw-fusion-api/v1',
    [string]$ProxyUrl = '',
    [string]$LogPath = '',
    [string]$OutputDirectory = '',
    [switch]$NonInteractive
)
$ErrorActionPreference = 'Stop'
# Windows PowerShell 5.1 在参数默认值求值时尚未初始化 PSScriptRoot。
if ([string]::IsNullOrWhiteSpace($OutputDirectory)) { $OutputDirectory = $PSScriptRoot }
$ConnectSeconds = 10
$RequestSeconds = 25
$LookupMilliseconds = 8000
$TailLines = 8000
$MaxEvents = 250
. (Join-Path $PSScriptRoot 'collect-xinghe.ps1')

function Convert-Endpoint {
    param([string]$Value, [switch]$Proxy)
    $uri = $null
    $schemes = @('http', 'https')
    if ($Proxy) { $schemes += @('socks5', 'socks5h') }
    if (-not [uri]::TryCreate($Value.Trim(), [UriKind]::Absolute, [ref]$uri)) {
        throw '地址不是完整 URL，请包含 http:// 或 https://。'
    }
    if ($uri.Scheme -notin $schemes -or -not $uri.Host) { throw '地址协议或主机名不受支持。' }
    if ($uri.UserInfo -or $uri.Query -or $uri.Fragment) {
        throw '请只填写不带账号、密码、查询参数或片段的地址。无需 API Key。'
    }
    if ($Proxy) { return $uri.AbsoluteUri }
    $url = $uri.AbsoluteUri.TrimEnd('/')
    if (-not $url.EndsWith('/responses')) { $url += '/responses' }
    return $url
}

function Get-SafeUrl {
    param([string]$Value)
    $uri = [uri]$Value
    $path = $uri.AbsolutePath -replace '(?i)/[^/]{32,}', '/[redacted]'
    return $uri.GetLeftPart([UriPartial]::Authority) + $path
}

function Test-Dns {
    param([uri]$Endpoint)
    $result = [ordered]@{ status = 'unknown'; addresses = @() }
    $operation = $null
    try {
        $operation = [Net.Dns]::BeginGetHostAddresses($Endpoint.DnsSafeHost, $null, $null)
        if (-not $operation.AsyncWaitHandle.WaitOne($LookupMilliseconds)) {
            $result.status = 'timeout'
            return [pscustomobject]$result
        }
        $result.addresses = @([Net.Dns]::EndGetHostAddresses($operation) | ForEach-Object { $_.IPAddressToString })
        $result.status = 'ok'
    } catch { $result.status = 'resolution_failed' }
    finally { if ($operation) { $operation.AsyncWaitHandle.Close() } }
    return [pscustomobject]$result
}

function Test-Tcp {
    param([uri]$Endpoint)
    $client = New-Object Net.Sockets.TcpClient
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $status = 'connection_failed'
    $operation = $null
    try {
        $operation = $client.BeginConnect($Endpoint.DnsSafeHost, $Endpoint.Port, $null, $null)
        if ($operation.AsyncWaitHandle.WaitOne($LookupMilliseconds)) {
            $client.EndConnect($operation)
            $status = 'ok'
        } else { $status = 'timeout' }
    } catch { $status = 'connection_failed' }
    finally {
        $client.Close()
        if ($operation) { $operation.AsyncWaitHandle.Close() }
    }
    return [pscustomobject]@{ status = $status; address_family = 'ipv4'; elapsed_ms = $timer.ElapsedMilliseconds }
}

function Invoke-HttpProbe {
    param([string]$Endpoint, [string]$Mode, [string]$Proxy)
    Write-Host "正在检查：$Mode（最多 $RequestSeconds 秒）..."
    $result = [ordered]@{ mode = $Mode; curl_exit = $null; http_status = 0; error_kind = 'curl_unavailable' }
    if (-not $CurlCommand) { return [pscustomobject]$result }
    # -q 禁用用户 curlrc；不携带认证、不跟随跳转、不保存响应头/正文。
    $format = 'IYW_DIAG|%{http_code}|%{remote_ip}|%{time_namelookup}|%{time_connect}|%{time_appconnect}|%{time_starttransfer}|%{time_total}|%{ssl_verify_result}'
    $arguments = @('-q', '--silent', '--show-error', '--globoff', '--proto', '=http,https',
        '--connect-timeout', "$ConnectSeconds", '--max-time', "$RequestSeconds",
        '--output', 'NUL', '--write-out', $format)
    # curl 8.16 之前不支持 --option=value，必须使用独立参数。
    if ($Proxy) { $arguments += @('--proxy', $Proxy) }
    else { $arguments += @('--noproxy', '*') }
    if ($Mode -eq 'direct_ipv4') { $arguments += '--ipv4' }
    if ($Mode -eq 'direct_ipv6') { $arguments += '--ipv6' }
    $arguments += @('--url', $Endpoint)
    $previousPreference = $ErrorActionPreference
    $previousNoProxy = [Environment]::GetEnvironmentVariable('NO_PROXY', 'Process')
    try {
        # 仅暂时清空采集进程变量，让代理对照不被 NO_PROXY 绕过；不改系统或应用。
        if ($Proxy) { [Environment]::SetEnvironmentVariable('NO_PROXY', $null, 'Process') }
        $ErrorActionPreference = 'Continue'
        $raw = @(& $CurlCommand.Source @arguments 2>&1)
        $result.curl_exit = $LASTEXITCODE
    } finally {
        if ($Proxy) { [Environment]::SetEnvironmentVariable('NO_PROXY', $previousNoProxy, 'Process') }
        $ErrorActionPreference = $previousPreference
    }
    $result.error_kind = Get-CurlErrorKind $result.curl_exit
    if ($result.curl_exit -lt 0) { $result.windows_status = '0x' + [Convert]::ToString([int]$result.curl_exit, 16).ToUpperInvariant() }
    $line = $raw | ForEach-Object { [string]$_ } | Where-Object { $_ -match '^IYW_DIAG\|' } | Select-Object -Last 1
    if (-not $line) { return [pscustomobject]$result }
    $values = $line -split '\|'
    if ($values.Count -ne 9) { return [pscustomobject]$result }
    $result.http_status = [int]$values[1]
    $result.remote_ip = $values[2]
    $names = @('dns_seconds', 'tcp_seconds', 'tls_seconds', 'first_byte_seconds', 'total_seconds', 'tls_verify_result')
    for ($index = 0; $index -lt $names.Count; $index++) { $result[$names[$index]] = $values[$index + 3] }
    return [pscustomobject]$result
}

function Get-Findings {
    param($Probes, $Logs, $App)
    $direct = $Probes | Where-Object { $_.mode -eq 'direct' } | Select-Object -First 1
    $probeFailed = $direct.error_kind -in @('curl_process_crash', 'curl_unavailable', 'curl_startup_or_option_error',
        'url_format_error', 'curl_feature_unavailable', 'unsupported_curl_option', 'unsupported_protocol')
    if ($probeFailed) {
        "HTTP 探测工具未能完成检查：$($direct.error_kind)，curl exit=$($direct.curl_exit)。不能据此判断云端网络不通。"
    } elseif ($direct.http_status -gt 0) {
        '云端直连收到了 HTTP 响应。无认证 GET 的 401/403/404/405 不能用于判断账号或模型是否正常。'
    } else { "云端直连未收到 HTTP 响应：$($direct.error_kind)，curl exit=$($direct.curl_exit)。" }
    $proxy = $Probes | Where-Object { $_.mode -eq 'explicit_proxy' } | Select-Object -First 1
    if ($proxy -and $proxy.http_status -gt 0 -and $direct.http_status -eq 0 -and -not $probeFailed) {
        '代理对照能收到响应而直连失败。当前源码的云端模型出口强制直连，改系统代理不保证对此版本生效。'
    }
    if (@($App.processes).Count -eq 0) { '未检测到原助理主进程；请保持原助理运行，并在复现故障后再采集一次。' }
    if ($Logs.events.Count -eq 0) { '没有采集到匹配日志；请确认日志路径和故障发生时间。' }
    $signals = @($Logs.events | ForEach-Object { $_.signals })
    if ('gateway_listener_failed' -in $signals) { '日志出现本机模型网关监听失败；需结合应用版本和日志时刻排查本机这一段。' }
    if ('send_failed' -in $signals) { '日志出现请求发送失败；需要结合时间、会话和 model_gateway 错误码区分本机与云端链路。' }
    '本报告只反映采集时刻的网络；监听端口不能单独证明网关健康，历史日志也不能单独证明当前仍有故障。'
    '未执行带认证的模型 POST、SSE 流或内置工具调用；curl 与应用 Rust/TLS 实现不同，成功不等于星河会话已恢复。'
}

try {
    Write-Host '星河 / 原助理连接诊断（进程内置运行时）' -ForegroundColor Cyan
    Write-Host '请保持原助理运行，先复现一次故障。无需管理员权限或 API Key。'
    Write-Host '仅生成本地报告，不修改原助理、代理、证书或防火墙设置。'
    $app = Get-AppSnapshot
    if (-not $LogPath) { $LogPath = Find-AppLogDirectory }
    if (-not $NonInteractive) {
        $answer = Read-Host "云端 API 基础地址 [回车使用正式版默认 $ApiBaseUrl]"
        if ($answer.Trim()) { $ApiBaseUrl = $answer.Trim() }
        $answer = Read-Host '可选代理地址（仅作对照，无代理回车跳过）'
        if ($answer.Trim()) { $ProxyUrl = $answer.Trim() }
        $answer = Read-Host "应用 logs 目录或导出的日志文件 [回车使用 $LogPath；为空则跳过]"
        if ($answer.Trim()) { $LogPath = $answer.Trim().Trim('"') }
    }
    $endpoint = Convert-Endpoint $ApiBaseUrl
    if ($ProxyUrl) { $ProxyUrl = Convert-Endpoint $ProxyUrl -Proxy }
    $CurlCommand = Get-Command curl.exe -CommandType Application -ErrorAction SilentlyContinue
    $curlVersion = 'unavailable'
    if ($CurlCommand) { $curlVersion = [string](& $CurlCommand.Source -q --version | Select-Object -First 1) }
    Write-Host '正在检查 DNS 和 TCP...'
    $dns = Test-Dns ([uri]$endpoint)
    $tcp = Test-Tcp ([uri]$endpoint)
    $probes = @(Invoke-HttpProbe $endpoint 'direct' '')
    if (@($dns.addresses | Where-Object { $_ -match ':' }).Count -gt 0) {
        $probes += Invoke-HttpProbe $endpoint 'direct_ipv4' ''
        $probes += Invoke-HttpProbe $endpoint 'direct_ipv6' ''
    }
    if ($ProxyUrl) { $probes += Invoke-HttpProbe $endpoint 'explicit_proxy' $ProxyUrl }
    Write-Host "正在用 Windows .NET 独立检查 HTTPS（最多 $RequestSeconds 秒）..."
    $windowsHttps = Invoke-WindowsHttpProbe $endpoint
    $crashes = Get-CrashSnapshot
    Write-Host '正在提取日志字段；不复制原始日志、请求正文或凭据...'
    $logs = Get-LogSnapshot $LogPath
    $findings = @(Get-WindowsFinding $windowsHttps) + @(Get-Findings $probes $logs $app)
    $report = [ordered]@{
        schema_version = 4; script_version = '1.4.0'; collected_at = [DateTimeOffset]::Now.ToString('o')
        os_version = [Environment]::OSVersion.Version.ToString(); powershell = $PSVersionTable.PSVersion.ToString()
        curl = $curlVersion; runtime = 'embedded_xinghe'; authenticated_model_test = $false
        endpoint = Get-SafeUrl $endpoint; endpoint_source = 'operator_input_or_source_default_not_live_process_config'
        proxy_test = [bool]$ProxyUrl; environment = Get-ProxyPresence; app = $app
        dns = $dns; direct_tcp = $tcp; http = $probes; windows_http = $windowsHttps
        application_crashes = $crashes; logs = $logs; findings = $findings
    }
    if ($ProxyUrl) { $report.proxy_endpoint = Get-SafeUrl $ProxyUrl }
    $null = New-Item -ItemType Directory -Path $OutputDirectory -Force
    $reportPath = Join-Path $OutputDirectory ('xinghe-report-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff') + '.txt')
    $text = "星河连接诊断报告`r`n" + ($findings -join "`r`n") + "`r`n`r`n" + ($report | ConvertTo-Json -Depth 12)
    [IO.File]::WriteAllText($reportPath, $text, (New-Object Text.UTF8Encoding($true)))
    Write-Host "`n检查完成。请把这个 TXT 文件发回来分析：" -ForegroundColor Green
    Write-Host $reportPath
    $findings | ForEach-Object { Write-Host "- $_" }
} catch {
    Write-Host '诊断未完成。请检查输入地址、文件访问权限及输出目录；错误类型如下：' -ForegroundColor Red
    Write-Host $_.Exception.GetType().FullName
    Write-Host ('文件：{0}；行：{1}；命令：{2}；参数：{3}' -f
        [IO.Path]::GetFileName($_.InvocationInfo.ScriptName), $_.InvocationInfo.ScriptLineNumber,
        $_.InvocationInfo.MyCommand.Name, $_.Exception.ParameterName)
    # 不输出异常原文，防止输入中的凭据或本地路径被带入报告。
    exit 1
}
