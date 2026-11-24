# 日志仅提取允许字段；不复制 message、detail、reason、请求正文及运行时配置。
$LogTargets = '^(xinghe_model_connection|model_gateway|codex_core::(client::timing|responses_retry|tasks)|codex_otel\.trace_safe|iyw_claw_lib::(acp::(runtime_observation|session_activity)|commands::network_diagnostics::probe))$'
$SignalPatterns = [ordered]@{
    gateway_ready = 'process local model gateway ready'
    gateway_listener_failed = 'local model listener failed'
    gateway_request_failed = 'local model request failed'
    send_failed = 'error sending request'
    timeout = 'timed out|timeout'
    tls = 'certificate|\btls\b|\bssl\b'
    dns = '\bdns\b|could not resolve|failed to resolve'
    refused = 'actively refused|connection refused'
    retry = 'retrying|stream disconnected'
    interrupted = 'interrupt received'
    no_completion = 'ended_without_completion'
}

function Get-AppSnapshot {
    $result = [ordered]@{ processes = @(); listeners = @(); listener_query = 'not_run' }
    $processes = @(Get-Process -Name 'iyw-claw', 'iyw-claw-server' -ErrorAction SilentlyContinue)
    foreach ($process in $processes) {
        $version = $null
        try { $version = $process.MainModule.FileVersionInfo.FileVersion } catch { }
        $result.processes += [pscustomobject]@{ id = $process.Id; name = $process.ProcessName; version = $version }
    }
    if ($processes.Count -eq 0) { return [pscustomobject]$result }
    try {
        $result.listeners = @(Get-NetTCPConnection -State Listen -ErrorAction Stop |
            Where-Object { $_.OwningProcess -in $processes.Id } | ForEach-Object {
                [pscustomobject]@{ process_id = $_.OwningProcess; address = $_.LocalAddress; port = $_.LocalPort }
            })
        $result.listener_query = 'ok'
    } catch { $result.listener_query = 'unavailable_or_access_denied' }
    return [pscustomobject]$result
}

function Find-AppLogDirectory {
    $candidates = @()
    foreach ($process in @(Get-Process -Name 'iyw-claw', 'iyw-claw-server' -ErrorAction SilentlyContinue)) {
        if (-not $process.Path) { continue }
        $directory = Split-Path -Parent $process.Path
        $candidates += Join-Path $directory 'logs'
        $candidates += Join-Path (Split-Path -Parent $directory) 'logs'
    }
    $candidates += 'D:\WXDrive\原助理\logs'
    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate -PathType Container) { return $candidate }
    }
    return ''
}

function Get-ProxyPresence {
    $result = [ordered]@{}
    foreach ($name in @('HTTP_PROXY', 'HTTPS_PROXY', 'ALL_PROXY', 'NO_PROXY', 'IYW_CLAW_MODEL_GATEWAY_BASE_URL')) {
        $result[$name + '_present_in_collector'] = [bool][Environment]::GetEnvironmentVariable($name)
    }
    $settings = Get-ItemProperty -LiteralPath 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Internet Settings' -ErrorAction SilentlyContinue
    $result.windows_proxy_enabled = [bool]$settings.ProxyEnable
    $result.windows_proxy_address_present = [bool]$settings.ProxyServer
    $result.windows_pac_present = [bool]$settings.AutoConfigURL
    $result.note = '仅为采集进程环境及 Windows 设置，不等于已运行原助理的实际环境或代理配置。'
    return [pscustomobject]$result
}

function Convert-LogEvent {
    param($Entry)
    if ($Entry.target -notmatch $LogTargets) { return $null }
    $timestamp = [DateTimeOffset]::MinValue
    if (-not [DateTimeOffset]::TryParse([string]$Entry.timestamp, [ref]$timestamp)) { return $null }
    $fields = $Entry.fields
    $event = [ordered]@{ timestamp = $timestamp.ToString('o'); target = [string]$Entry.target; signals = @() }
    if ($Entry.level -in @('TRACE', 'DEBUG', 'INFO', 'WARN', 'ERROR')) { $event.level = $Entry.level }
    $signalText = @($fields.message, $fields.detail, $fields.reason, $fields.sampling_error, $fields.'error.message', $fields.stage) -join ' '
    $event.signals = @($SignalPatterns.Keys | Where-Object { $signalText -match $SignalPatterns[$_] })
    foreach ($name in @('duration_ms', 'elapsed_ms', 'attempt', 'retries', 'max_retries', 'status', 'sampling_request_count', 'sampling_retry_count')) {
        $number = 0.0
        if ([double]::TryParse([string]$fields.$name, [Globalization.NumberStyles]::Float, [Globalization.CultureInfo]::InvariantCulture, [ref]$number)) { $event[$name] = $number }
    }
    foreach ($name in @('auth.header_attached', 'content_observed', 'auth.retry_after_unauthorized')) {
        if ($fields.$name -is [bool]) { $event[$name] = $fields.$name }
    }
    if ($fields.stage -in @('request_ready', 'ended_without_completion', 'first_event', 'first_content', 'completed')) { $event.stage = $fields.stage }
    if ($fields.proxy_mode -in @('direct', 'proxy')) { $event.proxy_mode = $fields.proxy_mode }
    if ([string]$fields.code -match '^(cloud_gateway_|local_gateway_)[a-z_]{1,40}$') { $event.gateway_code = $fields.code }
    $threadIds = @($fields.'conversation.id', $fields.thread_id, $Entry.span.thread_id, $Entry.spans.thread_id, $Entry.spans.'thread.id')
    $event.thread_ids = @($threadIds | ForEach-Object { $_ } | Where-Object { [string]$_ -match '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' } | Select-Object -Unique)
    $connection = Get-ConnectionLogFields $Entry
    foreach ($name in $connection.Keys) { $event[$name] = $connection[$name] }
    return [pscustomobject]$event
}

function Get-ConnectionLogFields {
    param($Entry)
    $fields = $Entry.fields
    $result = @{}
    $enums = @{
        phase = @('ready','route_prepared','request_failed','received','cloud_send_failed','cloud_headers_timeout','cloud_response')
        proxy_policy = @('transport_default','respect_system_proxy')
        transport_error = @('connection','timeout','network','policy','build','http_response','other')
        request_scheme = @('http','https'); upstream_scheme = @('http','https')
    }
    foreach ($name in $enums.Keys) { if ($fields.$name -in $enums[$name]) { $result[$name] = $fields.$name } }
    foreach ($name in @('request_host','upstream_host','listen_address')) {
        if ([string]$fields.$name -match '^[a-zA-Z0-9.:\[\]-]{1,253}$') { $result[$name] = $fields.$name }
    }
    foreach ($name in @('request_port','upstream_port')) {
        $number = 0
        if ([int]::TryParse([string]$fields.$name, [ref]$number)) { $result[$name] = $number }
    }
    foreach ($name in @('managed_network','proxy_env_present','no_proxy_env_present','proxy_route_observed','is_connect','is_timeout','cause_chain_truncated')) {
        if ($fields.$name -is [bool]) { $result[$name] = $fields.$name }
    }
    $allowed = @{
        cause_classes = @('timeout','tls','certificate','dns','connection_refused','connection_reset','proxy','proxy_tunnel','permission_denied','unreachable','connect','unclassified')
        io_error_kinds = @('NotFound','PermissionDenied','ConnectionRefused','ConnectionReset','HostUnreachable','NetworkUnreachable','ConnectionAborted','NotConnected','AddrInUse','AddrNotAvailable','NetworkDown','BrokenPipe','WouldBlock','InvalidInput','InvalidData','TimedOut','Interrupted','Unsupported','UnexpectedEof','OutOfMemory','Other','Uncategorized')
        os_error_codes = @()
    }
    foreach ($name in $allowed.Keys) {
        $values = @(Get-ConnectionArray $fields.$name $allowed[$name])
        if ($values.Count) { $result[$name] = $values }
    }
    $requestId = @($Entry.span.gateway_request_id, $Entry.spans.gateway_request_id) | ForEach-Object { $_ } | Where-Object { $null -ne $_ } | Select-Object -First 1
    $number = 0L
    if ([long]::TryParse([string]$requestId, [ref]$number)) { $result.gateway_request_id = $number }
    $address = @($Entry.span.listen_address, $Entry.spans.listen_address) | ForEach-Object { $_ } | Where-Object { [string]$_ -match '^127\.0\.0\.1:[0-9]{1,5}$' } | Select-Object -First 1
    if ([string]$address -match '^127\.0\.0\.1:[0-9]{1,5}$') { $result.listen_address = $address }
    return $result
}

function Get-ConnectionArray {
    param($Value, $Allowed)
    if ($Value -is [string]) {
        try { $Value = $Value | ConvertFrom-Json -ErrorAction Stop } catch { return }
    }
    foreach ($item in @($Value) | Select-Object -First 16) {
        if ($Allowed.Count) { if ($item -in $Allowed) { $item }; continue }
        if ([string]$item -match '^-?[0-9]{1,10}$') { [long]$item }
    }
}

function Get-LogSnapshot {
    param([string]$Path)
    $result = [ordered]@{ status = 'not_supplied'; scanned_lines = 0; invalid_json_lines = 0;
        matched_events = 0; max_tail_lines_per_file = $TailLines; max_events = $MaxEvents; sources = @(); events = @() }
    if (-not $Path) { return [pscustomobject]$result }
    if (-not (Test-Path -LiteralPath $Path)) { $result.status = 'path_not_found'; return [pscustomobject]$result }
    $files = @(Get-Item -LiteralPath $Path)
    if ($files[0].PSIsContainer) {
        $files = @(Get-ChildItem -LiteralPath $Path -File -Filter 'iyw-claw*.log' |
            Sort-Object LastWriteTime -Descending | Select-Object -First 2 | Sort-Object LastWriteTime)
    }
    $events = New-Object 'System.Collections.Generic.List[object]'
    foreach ($file in $files) {
        $source = [ordered]@{ index = $result.sources.Count; bytes = $file.Length; modified_at = $file.LastWriteTimeUtc.ToString('o'); status = 'ok' }
        try { $lines = @(Get-Content -LiteralPath $file.FullName -Encoding UTF8 -Tail $TailLines) }
        catch { $source.status = 'read_failed'; $result.sources += [pscustomobject]$source; continue }
        $result.scanned_lines += $lines.Count
        foreach ($line in $lines) {
            try { $entry = $line | ConvertFrom-Json -ErrorAction Stop }
            catch { $result.invalid_json_lines++; continue }
            $event = Convert-LogEvent $entry
            if ($event) { $events.Add($event) }
        }
        $result.sources += [pscustomobject]$source
    }
    $result.matched_events = $events.Count
    $result.events = @($events | Sort-Object timestamp | Select-Object -Last $MaxEvents)
    $result.status = 'collected'
    return [pscustomobject]$result
}

function Get-CurlErrorKind {
    param([int]$ExitCode)
    if ($ExitCode -lt 0) { return 'curl_process_crash' }
    $codes = @{ 0 = 'none'; 5 = 'proxy_dns'; 6 = 'dns'; 7 = 'connect'; 28 = 'timeout';
        1 = 'unsupported_protocol'; 2 = 'curl_startup_or_option_error'; 3 = 'url_format_error';
        4 = 'curl_feature_unavailable'; 48 = 'unsupported_curl_option';
        35 = 'tls_handshake'; 60 = 'certificate'; 56 = 'receive_or_proxy'; 97 = 'proxy_handshake' }
    if ($codes.ContainsKey($ExitCode)) { return $codes[$ExitCode] }
    return 'other_curl_error'
}

function Invoke-WindowsHttpProbe {
    param([string]$Endpoint)
    $result = [ordered]@{ transport = 'Windows_NET_Framework'; mode = 'direct'; tls = 'TLS1.2';
        http_status = 0; status = 'not_started'; elapsed_ms = 0 }
    $previousTls = [Net.ServicePointManager]::SecurityProtocol
    $timer = [Diagnostics.Stopwatch]::StartNew()
    $response = $null
    $request = $null
    try {
        [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
        $request = [Net.HttpWebRequest]::Create($Endpoint)
        $request.Method = 'GET'
        $request.Proxy = $null
        $request.AllowAutoRedirect = $false
        $request.UseDefaultCredentials = $false
        $request.Timeout = $RequestSeconds * 1000
        $request.ReadWriteTimeout = $RequestSeconds * 1000
        $response = $request.GetResponse()
        $result.http_status = [int]$response.StatusCode
        $result.status = 'http_response'
    } catch {
        $failure = $_.Exception
        while ($failure -and $failure -isnot [Net.WebException]) { $failure = $failure.InnerException }
        if ($failure) {
            $result.status = [string]$failure.Status
            $response = $failure.Response
            if ($response) { $result.http_status = [int]$response.StatusCode; $result.status = 'http_response' }
        } else { $result.status = 'probe_failed'; $result.error_type = $_.Exception.GetType().FullName }
    } finally {
        if ($response) { $response.Close() }
        if ($request) { $request.Abort() }
        [Net.ServicePointManager]::SecurityProtocol = $previousTls
    }
    $result.elapsed_ms = $timer.ElapsedMilliseconds
    return [pscustomobject]$result
}

function Get-WindowsFinding {
    param($Probe)
    if ($Probe.http_status -gt 0) {
        return "Windows .NET 直连收到 HTTP $($Probe.http_status)，耗时 $($Probe.elapsed_ms) ms；这是独立于 curl 的连通性证据，不能证明模型认证和会话正常。"
    }
    return "Windows .NET 直连未收到 HTTP 响应：$($Probe.status)，耗时 $($Probe.elapsed_ms) ms。"
}

function Get-CrashSnapshot {
    $windowHours = 2
    $maxRecords = 50
    $maxMatches = 8
    $result = [ordered]@{ status = 'no_matching_records'; lookback_hours = $windowHours; events = @() }
    try {
        $filter = @{ LogName = 'Application'; ProviderName = 'Application Error'; Id = 1000;
            StartTime = (Get-Date).AddHours(-$windowHours) }
        $records = @(Get-WinEvent -FilterHashtable $filter -MaxEvents $maxRecords -ErrorAction Stop)
        foreach ($record in $records) {
            $xml = [xml]$record.ToXml()
            $fields = @{}
            foreach ($item in $xml.Event.EventData.Data) { $fields[[string]$item.Name] = [string]$item.'#text' }
            if ($fields.AppName -notin @('curl.exe', 'iyw-claw.exe', 'iyw-claw-server.exe')) { continue }
            $entry = [ordered]@{ timestamp = $record.TimeCreated.ToUniversalTime().ToString('o'); application = $fields.AppName }
            $module = [IO.Path]::GetFileName($fields.ModuleName)
            if ($module -match '^[a-zA-Z0-9_.-]{1,120}$') { $entry.fault_module = $module }
            if ($fields.ExceptionCode -match '^(0x)?[0-9a-fA-F]{8}$') { $entry.exception_code = $fields.ExceptionCode }
            if ($fields.ModuleVersion -match '^[0-9.]{1,40}$') { $entry.module_version = $fields.ModuleVersion }
            $result.events += [pscustomobject]$entry
        }
        $result.events = @($result.events | Select-Object -First $maxMatches)
        $result.status = 'queried'
    } catch {
        if ($_.FullyQualifiedErrorId -notmatch '^NoMatchingEventsFound') { $result.status = 'unavailable_or_access_denied' }
    }
    return [pscustomobject]$result
}
