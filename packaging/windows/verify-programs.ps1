param([Parameter(Mandatory=$true)][string]$Folder)
$ErrorActionPreference = 'Stop'
$root = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$text = Get-Content -Raw -LiteralPath (Join-Path $root 'Cargo.toml')
$match = [regex]::Match($text, '(?ms)^\[workspace\.package\]\s*.*?^version\s*=\s*"([^"]+)"')
if (-not $match.Success) { throw 'No workspace version' }
$version = $match.Groups[1].Value
$commit = (git -C $root rev-parse --short=8 HEAD).Trim()
if ($LASTEXITCODE -ne 0) { throw 'No checkout commit' }
foreach ($name in @('mailsolutions.exe','mailsolutions-cli.exe')) {
  $files = @(Get-ChildItem -LiteralPath $Folder -Recurse -File -Filter $name)
  if ($files.Count -ne 1) { throw "Expected one $name" }
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = $files[0].FullName
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
      $info = [Diagnostics.FileVersionInfo]::GetVersionInfo($files[0].FullName)
      if ($info.ProductName -cne 'MailSolutions' -or $info.OriginalFilename -cne $name -or $info.FileVersion -cne $version.Split('-')[0]) { throw 'Incorrect GUI version resources' }
    }
    Write-Output "Verified $expected; version-only execution, no profile opened"
  } finally { $process.Dispose() }
}
