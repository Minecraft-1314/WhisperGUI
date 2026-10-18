[CmdletBinding()]
param(
  [string]$Configuration = 'Release',
  [switch]$DebugBuild,
  [switch]$SkipTests,
  [switch]$SkipUiAudit,
  [switch]$CleanDebug
)

$ErrorActionPreference = 'Continue'
Set-StrictMode -Version Latest

$Root = $PSScriptRoot

$NodeHome = if ($env:NODE_HOME) { $env:NODE_HOME } else { 'E:\node' }
$NodeExe = Join-Path $NodeHome 'node.exe'
Add-Type -AssemblyName System.IO.Compression.FileSystem
function New-PortableZip {
    param([string]$SourceDir, [string]$Destination)
    if (Test-Path -LiteralPath $Destination) { Remove-Item -LiteralPath $Destination -Force }
    # Compress-Archive rejects the 1973 timestamp shipped in WebView2Loader.dll
    [System.IO.Compression.ZipFile]::CreateFromDirectory(
        $SourceDir, $Destination, [System.IO.Compression.CompressionLevel]::Optimal, $false)
    if (-not (Test-Path -LiteralPath $Destination)) { throw "Portable ZIP was not produced: $Destination" }
    Write-Host "portable: $Destination" -ForegroundColor Green
}

$DistDir = Join-Path $Root 'dist'
$AppDir = Join-Path $DistDir 'WhisperGUI-win-x64'
$Cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
$CargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
$AuditDir = Join-Path $Root 'artifacts\ui-audit'

function Write-Step([string]$text) {
  Write-Host ''
  Write-Host "==> $text" -ForegroundColor Cyan
}

function Invoke-Cargo([string[]]$arguments) {
  & $Cargo @arguments
  if ($LASTEXITCODE -ne 0) {
    throw "Command failed: cargo $($arguments -join ' ')"
  }
}

function Remove-Tree([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) { return }
  $resolved = [System.IO.Path]::GetFullPath($path)
  $rootResolved = [System.IO.Path]::GetFullPath($Root)
  if (-not $resolved.StartsWith($rootResolved, [System.StringComparison]::OrdinalIgnoreCase)) {
    throw "Refusing to delete outside the project: $resolved"
  }
  [System.IO.Directory]::Delete($resolved, $true)
}

function Copy-Required([string]$from, [string]$to) {
  if (-not (Test-Path -LiteralPath $from -PathType Leaf)) {
    throw "Required file missing: $from"
  }
  Copy-Item -LiteralPath $from -Destination $to -Force -ErrorAction Stop
}

function Assert-Exists([string]$path) {
  if (-not (Test-Path -LiteralPath $path)) {
    throw "Required path missing: $path"
  }
}

if (-not (Test-Path -LiteralPath $Cargo)) {
  throw "cargo was not found at $Cargo"
}
$env:PATH = "$CargoBin;$env:PATH"

Push-Location $Root
try {
  $profile = if ($DebugBuild) { 'debug' } else { 'release' }
  $cargoProfile = if ($DebugBuild) { @() } else { @('--release') }

  Write-Step 'Format check'
  Invoke-Cargo @('fmt', '--all', '--', '--check')

  Write-Step 'Clippy (warnings are errors)'
  Invoke-Cargo @('clippy', '--all-targets', '--', '-D', 'warnings')

  if (-not $SkipTests) {
    Write-Step 'Tests'
    Invoke-Cargo @('test')
  }

  Write-Step "Build ($profile)"
  Invoke-Cargo (@('build') + $cargoProfile)

  Write-Step 'Package'
  Remove-Tree $DistDir
  New-Item -ItemType Directory -Path $AppDir -Force | Out-Null
  Copy-Required (Join-Path $Root "target\$profile\WhisperGUI.exe") (Join-Path $AppDir 'WhisperGUI.exe')
Copy-Required (Join-Path $Root "target\$profile\uninstall.exe") (Join-Path $AppDir 'uninstall.exe')

  $loader = Get-ChildItem -LiteralPath (Join-Path $Root 'target') -Recurse -File -Filter 'WebView2Loader.dll' -ErrorAction SilentlyContinue |
    Where-Object { $_.FullName -match '\\x64\\' } |
    Select-Object -First 1
  if ($null -eq $loader) {
    throw 'WebView2Loader.dll was not produced by the build.'
  }
  Copy-Required $loader.FullName (Join-Path $AppDir 'WebView2Loader.dll')

  Copy-Required (Join-Path $Root 'app.ico') (Join-Path $AppDir 'app.ico')
  Copy-Required (Join-Path $Root 'README.md') (Join-Path $AppDir 'README.md')
  Copy-Required (Join-Path $Root 'LICENSE') (Join-Path $AppDir 'LICENSE')

  foreach ($name in @('wwwroot', 'resources', 'python')) {
    $source = Join-Path $Root $name
    Assert-Exists $source
    Copy-Item -LiteralPath $source -Destination (Join-Path $AppDir $name) -Recurse -Force -ErrorAction Stop
  }

  foreach ($relative in @(
      'WhisperGUI.exe',
      'WebView2Loader.dll',
      'app.ico',
      'README.md',
      'LICENSE',
      'wwwroot\index.html',
      'wwwroot\assets\app.css',
      'wwwroot\assets\app.js',
      'wwwroot\assets\i18n.js',
      'wwwroot\assets\lucide.min.js',
      'wwwroot\i18n\zh-CN.json',
      'wwwroot\i18n\en-US.json',
      'resources\models.json',
      'python\transcribe.py'
    )) {
    Assert-Exists (Join-Path $AppDir $relative)
  }

  if (-not $SkipUiAudit) {
    $audit = Join-Path $Root 'tools\ui-audit\audit.mjs'
    if (Test-Path -LiteralPath $audit) {
      Write-Step 'UI audit'
      $node = Join-Path $NodeHome 'node_modules\playwright-core\package.json'
      if (-not (Test-Path -LiteralPath $node)) {
        Write-Host '  playwright-core is not installed; skipping the UI audit (development-only check).' -ForegroundColor Yellow
      } else {
        New-Item -ItemType Directory -Path $AuditDir -Force | Out-Null
        & $NodeExe $audit (Join-Path $AppDir 'wwwroot') $AuditDir
        if ($LASTEXITCODE -ne 0) {
          throw 'UI audit reported violations.'
        }
      }
    }
  }

  Write-Step 'Inno Setup'
  $iscc = 'C:\Program Files (x86)\Inno Setup 7\ISCC.exe'
  if (-not (Test-Path -LiteralPath $iscc)) { $iscc = 'C:\Program Files (x86)\Inno Setup 6\ISCC.exe' }
  if (-not (Test-Path -LiteralPath $iscc)) {
    Write-Warning 'Inno Setup was not found; skipping the installer step.'
  } else {
    & $iscc --quiet (Join-Path $Root 'installer\WhisperGUI.iss')
    if ($LASTEXITCODE -ne 0) { throw 'Inno Setup reported a compilation error.' }
    Write-Host ("installer: " + (Join-Path $DistDir 'WhisperGUI-Setup.exe')) -ForegroundColor Green
  }

  Write-Step 'WiX MSI'
  $wix = Join-Path $env:USERPROFILE '.dotnet\tools\wix.exe'
  if (-not (Test-Path -LiteralPath $wix)) {
    Write-Warning 'The WiX tool was not found; skipping the MSI step.'
  } else {
    $msiFile = Join-Path $DistDir 'WhisperGUI.msi'
    & $wix build (Join-Path $Root 'installer\Package.wxs') -ext WixToolset.UI.wixext -arch x64 -pdbtype none -d ProductVersion=0.0.1 -d ("AppSource=" + $AppDir) -o $msiFile
    if ($LASTEXITCODE -ne 0) { throw 'WiX reported a compilation error.' }
    if (-not (Test-Path -LiteralPath $msiFile)) { throw "MSI was not produced: $msiFile" }
    Write-Host "installer: $msiFile" -ForegroundColor Green
  }

  Write-Step 'Portable ZIP'
  New-PortableZip -SourceDir $AppDir -Destination (Join-Path $DistDir 'WhisperGUI-portable-win-x64.zip')

  Write-Step 'Result'
  Get-ChildItem -LiteralPath $AppDir -Recurse -File |
    Sort-Object FullName |
    ForEach-Object { '{0,-58} {1,12:N0} bytes' -f $_.FullName.Substring($AppDir.Length + 1), $_.Length }
  Write-Host ''
  Write-Host 'Build finished.' -ForegroundColor Green
}
finally {
  Pop-Location
}
if ($CleanDebug) {
    $debugDir = Join-Path $PSScriptRoot 'target\debug'
    if (Test-Path -LiteralPath $debugDir) {
        Remove-Item -LiteralPath $debugDir -Recurse -Force
        Write-Host "cleaned: $debugDir" -ForegroundColor Green
    }
}
