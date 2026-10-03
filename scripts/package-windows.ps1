<#
.SYNOPSIS
    Builds the Neural-Thinker Community edition for Windows and packages it as a
    portable zip and an installer, with SHA-256 checksums.

.DESCRIPTION
    Used by .github/workflows/release.yml and runnable locally from the repo root:

        pwsh scripts/package-windows.ps1              # zip + installer (needs Inno Setup 6)
        pwsh scripts/package-windows.ps1 -NoInstaller # zip only

    Output goes to target/dist/out/. Before anything is written there the script
    checks that no private (paid services) code can be in the package: the private
    crate must not be in the dependency graph, the package must hold only the
    allowed files, and the executable must not carry the private marker.
#>
param(
    [switch]$NoInstaller
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$Root = (git rev-parse --show-toplevel).Trim()
Set-Location $Root
$InCi = [bool]$env:GITHUB_ACTIONS

function Fail([string]$Message) {
    Write-Error "package-windows: $Message"
    exit 1
}

# --- Version --------------------------------------------------------------------
$meta = cargo metadata --locked --format-version 1 | ConvertFrom-Json
if ($LASTEXITCODE -ne 0) { Fail 'cargo metadata failed (is Cargo.lock up to date?)' }
$pkg = $meta.packages | Where-Object { $_.name -eq 'neural-thinker' -and $_.source -eq $null }
$Version = $pkg.version
Write-Host "Packaging Neural-Thinker $Version"

# --- Private code guard -------------------------------------------------------------
# Release builds run on a fresh CI checkout, where src/private/ cannot exist. A
# local checkout may have it; the Community binary never links it, but say so.
if (Test-Path 'src/private') {
    if ($InCi) { Fail 'src/private/ exists in the CI checkout' }
    Write-Warning 'src/private/ exists locally; it is not part of the Community build.'
}
$privateDeps = $meta.packages | Where-Object { $_.name -match '^neural[-_]thinker[-_](private|pro)' }
if ($privateDeps) { Fail "the private crate is in the dependency graph: $($privateDeps.name)" }

# --- Build ----------------------------------------------------------------------------
# Fails the build if the icon and version info cannot be embedded (see build.rs).
$env:NT_REQUIRE_WIN_RESOURCES = '1'
cargo build --release --locked --bin neural-thinker
if ($LASTEXITCODE -ne 0) { Fail 'cargo build failed' }
Remove-Item Env:NT_REQUIRE_WIN_RESOURCES

$Exe = Join-Path $Root 'target/release/neural-thinker.exe'
$info = (Get-Item $Exe).VersionInfo
if ($info.ProductVersion -ne $Version -or $info.ProductName -ne 'Neural-Thinker') {
    Fail "version info not embedded (ProductName='$($info.ProductName)', ProductVersion='$($info.ProductVersion)')"
}

# --- Stage ------------------------------------------------------------------------------
$Name = "neural-thinker-$Version-windows-x64"
$Dist = Join-Path $Root 'target/dist'
$Stage = Join-Path $Dist $Name
$Out = Join-Path $Dist 'out'
if (Test-Path $Dist) { Remove-Item $Dist -Recurse -Force }
New-Item -ItemType Directory -Path $Stage, $Out | Out-Null

Copy-Item $Exe (Join-Path $Stage 'neural-thinker.exe')
Copy-Item 'LICENSE.md' (Join-Path $Stage 'LICENSE.txt')
Copy-Item 'packaging/windows/NOTICE.txt' (Join-Path $Stage 'NOTICE.txt')

# --- Verify the package -------------------------------------------------------------
$allowed = @('LICENSE.txt', 'NOTICE.txt', 'neural-thinker.exe')
$staged = @(Get-ChildItem $Stage -Recurse -File | ForEach-Object { $_.Name } | Sort-Object)
if (Compare-Object $allowed $staged) {
    Fail "unexpected package contents: $($staged -join ', ')"
}
# The marker is split so this script does not carry it (see scripts/check-private.sh).
$forbidden = @(('NT-PRIVATE-' + 'DO-NOT-PUBLISH'), 'neural_thinker_pro', 'neural-thinker-pro',
    'neural_thinker_private', 'neural-thinker-private')
foreach ($file in Get-ChildItem $Stage -File) {
    $text = [System.Text.Encoding]::GetEncoding(28591).GetString([System.IO.File]::ReadAllBytes($file.FullName))
    foreach ($needle in $forbidden) {
        if ($text.Contains($needle)) { Fail "$($file.Name) contains '$needle'" }
    }
}
$notice = Get-Content (Join-Path $Stage 'LICENSE.txt') -TotalCount 1
if ($notice -notlike 'Required Notice:*') { Fail 'LICENSE.txt does not start with the Required Notice' }

# --- Zip ------------------------------------------------------------------------------------
Compress-Archive -Path $Stage -DestinationPath (Join-Path $Out "$Name.zip")

# --- Installer ------------------------------------------------------------------------------
if (-not $NoInstaller) {
    $iscc = (Get-Command 'ISCC.exe' -ErrorAction SilentlyContinue).Source
    if (-not $iscc) {
        $iscc = Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6\ISCC.exe'
    }
    if (-not (Test-Path $iscc)) {
        if (-not $InCi) { Fail 'Inno Setup 6 not found; install it or pass -NoInstaller' }
        choco install innosetup --no-progress -y
        if ($LASTEXITCODE -ne 0) { Fail 'could not install Inno Setup' }
    }
    $numeric = ($Version -split '[-+]')[0]
    & $iscc /Q "/DAppVersion=$Version" "/DAppNumericVersion=$numeric" "/DStageDir=$Stage" "/DOutputDir=$Out" 'packaging/windows/neural-thinker.iss'
    if ($LASTEXITCODE -ne 0) { Fail 'Inno Setup failed' }
}

# --- Checksums -------------------------------------------------------------------------------
$sums = Get-ChildItem $Out -File | Sort-Object Name | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
}
Set-Content -Path (Join-Path $Out 'SHA256SUMS.txt') -Value $sums -Encoding ascii

Write-Host 'Packaged:'
Get-ChildItem $Out -File | ForEach-Object { Write-Host "  $($_.Name)  ($([math]::Round($_.Length / 1MB, 2)) MB)" }
if ($env:GITHUB_OUTPUT) {
    "version=$Version" | Out-File -FilePath $env:GITHUB_OUTPUT -Append -Encoding utf8
}
