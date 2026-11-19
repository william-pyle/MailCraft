# Dot-source without -Folder to test path selection without executing a program.
param([string]$Folder)
$ErrorActionPreference = 'Stop'
function Get-ProgramFiles([string]$Directory) {
  # Cargo also puts same-named programs under deps/. Only the explicit output directory
  # is authoritative; portable callers pass the known archive's top-level folder.
  $programs = @()
  foreach ($name in @('mailsolutions.exe','mailsolutions-cli.exe')) {
    $file = Get-Item -LiteralPath (Join-Path $Directory $name)
    if ($file.PSIsContainer) { throw "Expected a program file: $name" }
    $programs += $file
  }
  return $programs
}
if (-not $Folder) { return }
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$text = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
$match = [regex]::Match($text, '(?ms)^\[workspace\.package\]\s*.*?^version\s*=\s*"([^"]+)"')
if (-not $match.Success) { throw 'No workspace version' }
$version = $match.Groups[1].Value
$commit = (git -C $root rev-parse --short=8 HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'No checkout commit' }
foreach ($file in @(Get-ProgramFiles $Folder)) {
  $name = $file.Name
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = $file.FullName
  $start.Arguments = '--version'
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.WindowStyle = [Diagnostics.ProcessWindowStyle]::Hidden
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $process = [Diagnostics.Process]::Start($start)
  try {
    $stdout = $process.StandardOutput.ReadToEndAsync()
    $stderr = $process.StandardError.ReadToEndAsync()
    if (-not $process.WaitForExit(30000)) { throw "$name --version timed out" }
    if ($process.ExitCode -ne 0) { throw "$name --version failed" }
    $expected = if ($name -eq 'mailsolutions.exe') { "mailsolutions $version ($commit)" } else { "mailsolutions-cli $version" }
    if ($stdout.Result.Trim() -cne $expected -or $stderr.Result.Trim()) { throw "Unexpected $name version/commit output" }
    if ($name -eq 'mailsolutions.exe') {
      $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($file.FullName)
      if ($info.ProductName -cne 'MailSolutions' -or $info.OriginalFilename -cne $name -or $info.FileVersion -cne $version.Split('-')[0]) { throw 'Incorrect GUI version resources' }
    }
    Write-Output "Verified $expected; version-only execution, no profile opened"
  } finally { $process.Dispose() }
}
