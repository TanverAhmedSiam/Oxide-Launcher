<#
  Builds Oxide Launcher and packs it, with bundled Java, into ONE Windows installer.

  You need:  Rust (cargo)  +  Inno Setup 6.4 or newer (https://jrsoftware.org/isdl.php)  +  internet on the first run.

  Run from the project folder:
    powershell -ExecutionPolicy Bypass -File installer\build-installer.ps1

  Options:
    -Java 17,21     bundle only these Java versions (default: 8,17,21,25)
    -SkipBuild      reuse the existing target\release\oxide-launcher.exe

  Result: installer\output\OxideLauncher-Setup-<version>.exe
#>
param(
    [string[]]$Java = @('8', '17', '21', '25'),
    [switch]$SkipBuild
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'   # makes Invoke-WebRequest much faster
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

$here = $PSScriptRoot
$root = Split-Path $here -Parent
Set-Location $root

$version = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
Write-Host "Oxide Launcher $version" -ForegroundColor Yellow

# 1. Build the launcher
if (-not $SkipBuild) {
    Write-Host '==> Building (release)...'
    cargo build --release --bin oxide-launcher
    if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }
}
$exe = Join-Path $root 'target\release\oxide-launcher.exe'
if (-not (Test-Path $exe)) { throw "Not found: $exe" }

# 2. Fresh staging folder
$stage = Join-Path $here 'staging'
if (Test-Path $stage) { Remove-Item $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage, (Join-Path $stage 'runtimes') | Out-Null
Copy-Item $exe (Join-Path $stage 'oxide-launcher.exe')

# 3. Eclipse Temurin JREs (downloaded once, kept in installer\cache, checksum verified)
$cache = Join-Path $here 'cache'
New-Item -ItemType Directory -Force -Path $cache | Out-Null
foreach ($v in $Java) {
    $zip = Join-Path $cache "temurin-jre-$v.zip"
    if (-not (Test-Path $zip)) {
        Write-Host "==> Downloading Temurin JRE $v ..."
        $api = "https://api.adoptium.net/v3/assets/latest/$v/hotspot?architecture=x64&image_type=jre&os=windows&vendor=eclipse"
        $info = @(Invoke-RestMethod -Uri $api)
        if ($info.Count -eq 0) { throw "Adoptium has no JRE $v for Windows x64" }
        $pkg = $info[0].binary.package
        Invoke-WebRequest -Uri $pkg.link -OutFile $zip -UseBasicParsing
        $hash = (Get-FileHash $zip -Algorithm SHA256).Hash
        if ($hash -ne $pkg.checksum) { Remove-Item $zip -Force; throw "Checksum mismatch for Java $v" }
    }
    Write-Host "==> Unpacking Java $v ..."
    $tmp = Join-Path $cache "unpack-$v"
    if (Test-Path $tmp) { Remove-Item $tmp -Recurse -Force }
    Expand-Archive -Path $zip -DestinationPath $tmp -Force
    $inner = Get-ChildItem $tmp -Directory | Select-Object -First 1
    $dest = Join-Path $stage "runtimes\java-$v"
    Move-Item $inner.FullName $dest
    Remove-Item $tmp -Recurse -Force
    if (-not (Test-Path (Join-Path $dest 'bin\javaw.exe'))) { throw "Java ${v}: bin\javaw.exe is missing" }
}

# 4. Inno Setup
$iscc = @(
    "${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe",
    "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
    "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe"
) | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $iscc) {
    $c = Get-Command iscc -ErrorAction SilentlyContinue
    if ($c) { $iscc = $c.Source }
}
if (-not $iscc) { throw 'Inno Setup 6 was not found. Install it from https://jrsoftware.org/isdl.php and run this again.' }

Write-Host '==> Building the installer...'
& $iscc "/DAppVersion=$version" (Join-Path $here 'oxide.iss')
if ($LASTEXITCODE -ne 0) { throw 'Inno Setup failed' }

$out = Join-Path $here "output\OxideLauncher-Setup-$version.exe"
$mb = [math]::Round((Get-Item $out).Length / 1MB, 1)
Write-Host "Done: $out ($mb MB)" -ForegroundColor Green
