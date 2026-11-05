# Native x64 build with the existing currentUser NSIS hooks/marker unchanged.
param(
    [Parameter(Mandatory=$true)][string]$Output,
    [Parameter(Mandatory=$true)][string]$CertificateSha1,
    [Parameter(Mandatory=$true)][string]$TimestampUrl,
    [Parameter(Mandatory=$true)][string]$Signtool,
    [Parameter(Mandatory=$true)][string]$Tauri
)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if (-not [Environment]::Is64BitProcess -or [Environment]::OSVersion.Platform -ne 'Win32NT') { throw 'Native Windows x64 host required' }
if ($CertificateSha1 -notmatch '^[a-fA-F0-9]{40}$') { throw 'Invalid CertificateSha1' }
if (-not $env:CARGO_TARGET_DIR -or -not [IO.Path]::IsPathRooted($env:CARGO_TARGET_DIR)) { throw 'External CARGO_TARGET_DIR required' }
if (Test-Path -LiteralPath $Output) { throw 'Output must not exist' }
$Root = Split-Path -Parent $PSScriptRoot
# Use structured argv; no command strings or certificate/password interpolation.
# Tauri replaces the literal %1 with each final application/installer path.
$Sign = @{ cmd = $Signtool; args = @('sign', '/sha1', $CertificateSha1, '/tr', $TimestampUrl, '/td', 'SHA256', '/fd', 'SHA256', '%1') }
$Overlay = Join-Path (Split-Path -Parent $Output) 'windows-signing.json'
$OverlayJson = @{ bundle = @{ windows = @{ signCommand = $Sign } } } | ConvertTo-Json -Depth 10
[IO.File]::WriteAllText($Overlay, $OverlayJson, (New-Object Text.UTF8Encoding($false)))
Push-Location $Root
try {
    & cargo build --release --locked --manifest-path (Join-Path $Root 'rust/tokenmarket-cli/Cargo.toml')
    if ($LASTEXITCODE -ne 0) { throw 'CLI native build failed' }
    & $Tauri build --bundles nsis --config $Overlay
    if ($LASTEXITCODE -ne 0) { throw 'Desktop native build/sign failed' }
    $Release = Join-Path $env:CARGO_TARGET_DIR 'release'
    $App = Join-Path $Release 'app.exe'
    # Tauri signs the desktop and NSIS via signCommand. Sign the matching CLI too.
    & $Signtool sign /sha1 $CertificateSha1 /tr $TimestampUrl /td SHA256 /fd SHA256 (Join-Path $Release 'tokenmarket.exe')
    if ($LASTEXITCODE -ne 0) { throw 'CLI native signing failed' }
    $Nsis = @(Get-ChildItem -LiteralPath (Join-Path $Release 'bundle/nsis') -Filter '*.exe')
    if ($Nsis.Count -ne 1) { throw 'Exactly one NSIS required' }
    foreach ($Path in @($App, $Nsis[0].FullName, (Join-Path $Release 'tokenmarket.exe'))) {
        $Signature = Get-AuthenticodeSignature -LiteralPath $Path
        if ($Signature.Status -ne 'Valid' -or $Signature.SignerCertificate.Thumbprint -ne $CertificateSha1) { throw 'Final Authenticode identity verification failed' }
    }
    New-Item -ItemType Directory -Path $Output | Out-Null
    Copy-Item -LiteralPath $App -Destination (Join-Path $Output 'TokenMarket.exe')
    Copy-Item -LiteralPath $Nsis[0].FullName -Destination (Join-Path $Output 'setup.exe')
    Copy-Item -LiteralPath $Nsis[0].FullName -Destination (Join-Path $Output 'manual.exe')
    Copy-Item -LiteralPath (Join-Path $Release 'tokenmarket.exe') -Destination (Join-Path $Output 'tokenmarket.exe')
    $Evidence = @{ certificateSha1 = $CertificateSha1.ToLower() } | ConvertTo-Json
    [IO.File]::WriteAllText((Join-Path $Output 'signing-manifest.json'), $Evidence, (New-Object Text.UTF8Encoding($false)))
} finally {
    Pop-Location
    Remove-Item -LiteralPath $Overlay -ErrorAction SilentlyContinue
}
