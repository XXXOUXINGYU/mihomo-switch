$ErrorActionPreference = 'Stop'

function Get-Sha256Hex {
  param([Parameter(Mandatory = $true)][string]$Path)

  $stream = [System.IO.File]::OpenRead($Path)
  try {
    $sha256 = [System.Security.Cryptography.SHA256]::Create()
    try {
      $bytes = $sha256.ComputeHash($stream)
      return ([System.BitConverter]::ToString($bytes)).Replace('-', '').ToLowerInvariant()
    }
    finally {
      $sha256.Dispose()
    }
  }
  finally {
    $stream.Dispose()
  }
}

$projectRoot = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$tauriDir = Join-Path $projectRoot 'src-tauri'
$iconPath = (Resolve-Path (Join-Path $tauriDir 'icons\icon.ico')).Path
$nsisScript = Join-Path $tauriDir 'target\release\nsis\x64\installer.nsi'
$nsisBinary = Join-Path $env:LOCALAPPDATA 'tauri\NSIS\makensis.exe'
$bundleDir = Join-Path $tauriDir 'target\release\bundle\nsis'
$releaseDir = Join-Path $tauriDir 'target\release'
$internalTools = @('backend_audit', 'fetch_debug')

Push-Location $projectRoot
try {
  foreach ($tool in $internalTools) {
    Get-ChildItem -LiteralPath $releaseDir -Filter "$tool.*" -File -ErrorAction SilentlyContinue |
      Remove-Item -Force
  }
  & cargo tauri build -b nsis
  if ($LASTEXITCODE -ne 0) {
    throw "cargo tauri build failed with exit code $LASTEXITCODE"
  }

  if (-not (Test-Path $nsisScript)) {
    throw "NSIS script not found: $nsisScript"
  }

  if (-not (Test-Path $nsisBinary)) {
    throw "makensis.exe not found: $nsisBinary"
  }

  $scriptContent = Get-Content -Raw $nsisScript
  $escapedIconPath = $iconPath.Replace('\', '\\')
  $updatedContent = [Regex]::Replace(
    $scriptContent,
    '!define INSTALLERICON ".*"',
    "!define INSTALLERICON `"$escapedIconPath`"",
    [System.Text.RegularExpressions.RegexOptions]::None
  )
  $updatedContent = [Regex]::Replace(
    $updatedContent,
    '(?m)^\s*File /a "/oname=(backend_audit|fetch_debug)\.exe".*\r?\n',
    '',
    [System.Text.RegularExpressions.RegexOptions]::None
  )
  $updatedContent = [Regex]::Replace(
    $updatedContent,
    '(?m)^\s*Delete "\$INSTDIR\\(backend_audit|fetch_debug)\.exe"\r?\n',
    '',
    [System.Text.RegularExpressions.RegexOptions]::None
  )
  $updatedContent = $updatedContent.Replace(
    '    RmDir /r "$LOCALAPPDATA\${BUNDLEID}"',
    "    RmDir /r `"`$LOCALAPPDATA\`${BUNDLEID}`"`r`n    RmDir /r `"`$PROFILE\.mihomo_switch`""
  )

  if ($updatedContent -eq $scriptContent) {
    throw "Failed to patch INSTALLERICON in $nsisScript"
  }
  if (-not $updatedContent.Contains('RmDir /r "$PROFILE\.mihomo_switch"')) {
    throw "Failed to add runtime data cleanup to $nsisScript"
  }

  Set-Content -Path $nsisScript -Value $updatedContent -Encoding UTF8

  $nsisWorkdir = Split-Path $nsisScript
  Push-Location $nsisWorkdir
  try {
    & $nsisBinary 'installer.nsi'
    if ($LASTEXITCODE -ne 0) {
      throw "makensis failed with exit code $LASTEXITCODE"
    }
  }
  finally {
    Pop-Location
  }

  $rebuiltInstaller = Join-Path $nsisWorkdir 'nsis-output.exe'
  if (-not (Test-Path $rebuiltInstaller)) {
    throw "Rebuilt installer not found: $rebuiltInstaller"
  }

  $finalInstaller = Get-ChildItem $bundleDir -Filter '*-setup.exe' |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1

  if ($null -eq $finalInstaller) {
    throw "Bundle installer not found in $bundleDir"
  }

  Copy-Item $rebuiltInstaller $finalInstaller.FullName -Force
  Get-ChildItem -LiteralPath $bundleDir -Filter '*-setup.exe' -File |
    Where-Object { $_.FullName -ne $finalInstaller.FullName } |
    Remove-Item -Force

  $checksumPath = Join-Path $bundleDir 'SHA256SUMS.txt'
  $checksumTargets = @(
    (Join-Path $releaseDir 'mihomo_switch.exe'),
    $finalInstaller.FullName,
    (Join-Path $releaseDir 'mihomo.exe')
  )
  $checksumLines = foreach ($target in $checksumTargets) {
    if (-not (Test-Path -LiteralPath $target)) {
      throw "Release artifact not found for checksum: $target"
    }
    $hash = Get-Sha256Hex -Path $target
    "$hash  $([System.IO.Path]::GetFileName($target))"
  }
  Set-Content -LiteralPath $checksumPath -Value $checksumLines -Encoding ASCII

  Write-Host "Installer rebuilt with custom icon:"
  Write-Host $finalInstaller.FullName
  Write-Host "SHA-256 checksums:"
  Write-Host $checksumPath
}
finally {
  Pop-Location
}
