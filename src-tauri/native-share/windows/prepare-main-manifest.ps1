param(
    [Parameter(Mandatory=$true)][string]$Manifest,
    [Parameter(Mandatory=$true)][string]$Publisher,
    [Parameter(Mandatory=$true)][string]$Output
)
$ErrorActionPreference = 'Stop'
[xml]$document = Get-Content -LiteralPath $Manifest -Raw -Encoding UTF8
$assembly = $document.DocumentElement
if ($assembly.LocalName -ne 'assembly') { throw '主程序清单缺少 assembly 根元素。' }
foreach ($existing in @($assembly.SelectNodes('*[local-name()="msix"]'))) {
    $null = $assembly.RemoveChild($existing)
}
if (-not $assembly.SelectSingleNode('*[local-name()="assemblyIdentity"]')) {
    $identity = $document.CreateElement('assemblyIdentity', $assembly.NamespaceURI)
    $identity.SetAttribute('name', 'IywAssistant')
    $identity.SetAttribute('version', '1.0.0.0')
    $null = $assembly.PrependChild($identity)
}
$msix = $document.CreateElement('msix', 'urn:schemas-microsoft-com:msix.v1')
$msix.SetAttribute('publisher', $Publisher)
$msix.SetAttribute('packageName', 'IywAssistant.Desktop')
$msix.SetAttribute('applicationId', 'Share.Hub')
$null = $assembly.AppendChild($msix)
$settings = New-Object Xml.XmlWriterSettings
$settings.Encoding = New-Object Text.UTF8Encoding($false)
$writer = [Xml.XmlWriter]::Create($Output, $settings)
try { $document.Save($writer) } finally { $writer.Dispose() }
