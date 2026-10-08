# Install the verified release binary and, if missing, Cloudflare's official tunnel binary.
[CmdletBinding()]
param(
    [string]$Version = 'latest',
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'mcptunnel\bin'),
    [switch]$NoPath,
    [switch]$SkipCloudflared,
    [string]$AssetsDirectory
)
$ErrorActionPreference = 'Stop'
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
function Get-Sha256([string]$File) {
    $Stream = [IO.File]::OpenRead($File)
    $Hasher = [Security.Cryptography.SHA256]::Create()
    try { return [BitConverter]::ToString($Hasher.ComputeHash($Stream)).Replace('-', '').ToLowerInvariant() }
    finally { $Hasher.Dispose(); $Stream.Dispose() }
}
if ([Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Use install.sh on Linux/macOS.' }
if (![Environment]::Is64BitOperatingSystem) { throw 'mcptunnel requires a 64-bit Windows installation.' }
# The x64 release also runs under Windows ARM64 emulation.
$Target = 'x86_64-pc-windows-msvc'
$Repository = 'https://github.com/mcptunnel/mcptunnel'
if ($AssetsDirectory -and $Version -eq 'latest') { throw 'Offline assets require -Version vX.Y.Z.' }
if ($Version -eq 'latest') {
    $Version = (Invoke-RestMethod 'https://api.github.com/repos/mcptunnel/mcptunnel/releases/latest').tag_name
}
if ($Version -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+$') { throw 'Version must be vX.Y.Z.' }
$TempDir = Join-Path ([IO.Path]::GetTempPath()) ('mcptunnel-install-' + [Guid]::NewGuid())
New-Item -ItemType Directory -Path $TempDir | Out-Null
try {
    $Name = "mcptunnel-$Version-$Target"
    $ArchiveName = "$Name.zip"
    $Archive = Join-Path $TempDir $ArchiveName
    $Checksums = Join-Path $TempDir 'SHA256SUMS'
    foreach ($Asset in @($ArchiveName, 'SHA256SUMS')) {
        $Destination = Join-Path $TempDir $Asset
        if ($AssetsDirectory) {
            Copy-Item -LiteralPath (Join-Path $AssetsDirectory $Asset) -Destination $Destination
        } else {
            Invoke-WebRequest -UseBasicParsing "$Repository/releases/download/$Version/$Asset" -OutFile $Destination
        }
    }
    $Lines = @(Get-Content -LiteralPath $Checksums | Where-Object { $_ -match ('^[a-fA-F0-9]{64}  ' + [regex]::Escape($ArchiveName) + '$') })
    if ($Lines.Count -ne 1) { throw 'Missing or ambiguous archive checksum.' }
    $Expected = $Lines[0].Substring(0, 64)
    if ((Get-Sha256 $Archive) -ne $Expected) { throw 'Archive checksum mismatch; nothing installed.' }
    Expand-Archive -LiteralPath $Archive -DestinationPath $TempDir
    $Binary = Join-Path $TempDir "$Name\mcptunnel.exe"
    if (!(Test-Path -LiteralPath $Binary -PathType Leaf)) { throw 'Release archive is missing mcptunnel.exe.' }
    $Reported = & $Binary --version
    if ($LASTEXITCODE -ne 0 -or $Reported -ne ('mcptunnel ' + $Version.Substring(1))) { throw 'Release binary version mismatch.' }
    $Cloudflared = $null
    if (!$SkipCloudflared -and !(Get-Command cloudflared -ErrorAction SilentlyContinue) -and !(Test-Path -LiteralPath (Join-Path $InstallDir 'cloudflared.exe'))) {
        $Cloudflared = Join-Path $TempDir 'cloudflared.exe'
        Invoke-WebRequest -UseBasicParsing 'https://github.com/cloudflare/cloudflared/releases/download/2026.10.0/cloudflared-windows-amd64.exe' -OutFile $Cloudflared
        if ((Get-Sha256 $Cloudflared) -ne '86aee4017b26625cee8484c113558f48effa4cd47f7aa05fcf425604e5d2b23c') { throw 'Cloudflared checksum mismatch; nothing installed.' }
    }
    New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
    Copy-Item -LiteralPath $Binary -Destination (Join-Path $InstallDir 'mcptunnel.exe') -Force
    if ($Cloudflared) { Copy-Item -LiteralPath $Cloudflared -Destination (Join-Path $InstallDir 'cloudflared.exe') -Force }
    if (!$NoPath) {
        $UserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
        if (($UserPath -split ';') -notcontains $InstallDir) {
            [Environment]::SetEnvironmentVariable('Path', (($InstallDir, $UserPath | Where-Object { $_ }) -join ';'), 'User')
        }
        if (($env:Path -split ';') -notcontains $InstallDir) { $env:Path = "$InstallDir;$env:Path" }
    }
    Write-Host "Installed mcptunnel $Version in $InstallDir"
    Write-Host 'Next: follow the Filesystem MCP quick start at https://github.com/mcptunnel/mcptunnel#quick-start'
} finally {
    # Only remove the exact temporary directory created by this invocation.
    $ResolvedTemp = [IO.Path]::GetFullPath($TempDir)
    $ExpectedParent = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    if ($ResolvedTemp.StartsWith($ExpectedParent, [StringComparison]::OrdinalIgnoreCase) -and ([IO.Path]::GetFileName($ResolvedTemp)).StartsWith('mcptunnel-install-')) {
        Remove-Item -LiteralPath $ResolvedTemp -Recurse -Force
    }
}
