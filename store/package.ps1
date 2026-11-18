[CmdletBinding()]
param(
    [Parameter(Mandatory)] [ValidatePattern('^[a-zA-Z0-9.\-]{3,50}$')] [string] $PackageName,
    [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string] $Publisher,
    [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string] $PublisherDisplayName,
    [Parameter(Mandatory)] [ValidateNotNullOrEmpty()] [string] $DisplayName,
    [Parameter(Mandatory)] [ValidatePattern('^[1-9][0-9]*\.[0-9]+\.[0-9]+\.0$')] [string] $PackageVersion,
    [Parameter(Mandatory)] [string] $Executable,
    [Parameter(Mandatory)] [string] $Notices,
    [Parameter(Mandatory)] [string] $Output
)

$ErrorActionPreference = 'Stop'
if (([version] $PackageVersion).Major -gt 65535 -or
    ([version] $PackageVersion).Minor -gt 65535 -or
    ([version] $PackageVersion).Build -gt 65535) {
    throw 'Each MSIX version component must fit in 16 bits.'
}

$sdk = Get-ItemPropertyValue -Path 'HKLM:\SOFTWARE\Microsoft\Windows Kits\Installed Roots' -Name KitsRoot10
$makeAppx = Get-ChildItem -LiteralPath (Join-Path $sdk 'bin') -Directory |
    Where-Object { $_.Name -match '^\d+\.\d+\.\d+\.\d+$' -and (Test-Path (Join-Path $_.FullName 'x64\makeappx.exe')) } |
    Sort-Object { [version] $_.Name } -Descending |
    Select-Object -First 1 |
    ForEach-Object { Join-Path $_.FullName 'x64\makeappx.exe' }
if (!$makeAppx) { throw 'Install the Windows SDK to build an MSIX package.' }

$work = Join-Path ([IO.Path]::GetTempPath()) ('blur-and-save-msix-' + [guid]::NewGuid())
$outputFile = [IO.Path]::GetFullPath($Output)
try {
    $stage = New-Item -ItemType Directory -Path (Join-Path $work 'package\Assets') -Force
    $package = $stage.Parent.FullName
    Copy-Item -LiteralPath $Executable -Destination (Join-Path $package 'blur-and-save.exe')
    Copy-Item -LiteralPath $Notices -Destination (Join-Path $package 'THIRD-PARTY-NOTICES.txt')
    Copy-Item -Path "$PSScriptRoot\logos\*.png" -Destination $stage.FullName

    $manifest = [xml] (Get-Content -LiteralPath "$PSScriptRoot\AppxManifest.xml" -Raw)
    $identity = $manifest.Package.Identity
    $identity.SetAttribute('Name', $PackageName)
    $identity.SetAttribute('Publisher', $Publisher)
    $identity.SetAttribute('Version', $PackageVersion)
    $manifest.Package.Properties.DisplayName = $DisplayName
    $manifest.Package.Properties.PublisherDisplayName = $PublisherDisplayName
    $namespaces = [System.Xml.XmlNamespaceManager]::new($manifest.NameTable)
    $namespaces.AddNamespace('uap', 'http://schemas.microsoft.com/appx/manifest/uap/windows10')
    $manifest.SelectSingleNode('//uap:VisualElements', $namespaces).SetAttribute('DisplayName', $DisplayName)
    $manifest.Save((Join-Path $package 'AppxManifest.xml'))

    New-Item -ItemType Directory -Path (Split-Path -Parent $outputFile) -Force | Out-Null
    & $makeAppx pack /d $package /p $outputFile /o
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed to pack $outputFile." }

    $unpacked = Join-Path $work 'unpacked'
    & $makeAppx unpack /p $outputFile /d $unpacked /o
    if ($LASTEXITCODE -ne 0) { throw "MakeAppx failed to unpack $outputFile." }
    $roundTrip = [xml] (Get-Content -LiteralPath (Join-Path $unpacked 'AppxManifest.xml') -Raw)
    if ($roundTrip.Package.Identity.Name -cne $PackageName -or
        $roundTrip.Package.Identity.Publisher -cne $Publisher -or
        $roundTrip.Package.Identity.Version -cne $PackageVersion -or
        $roundTrip.Package.Properties.PublisherDisplayName -cne $PublisherDisplayName -or
        $roundTrip.Package.Properties.DisplayName -cne $DisplayName) {
        throw 'The packaged manifest does not match the requested Store identity.'
    }
    $sourceHash = (Get-FileHash -LiteralPath $Executable).Hash
    $packagedHash = (Get-FileHash -LiteralPath (Join-Path $unpacked 'blur-and-save.exe')).Hash
    if ($sourceHash -ne $packagedHash) { throw 'The packaged executable differs from the input.' }
    Get-FileHash -LiteralPath $outputFile
} finally {
    if (Test-Path -LiteralPath $work) { Remove-Item -LiteralPath $work -Recurse -Force }
}
