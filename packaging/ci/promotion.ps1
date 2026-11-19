# Trusted CI artifact promotion. No compiler, downloaded program, signing key or install runs here.
# Dot-source without -Mode for offline negative-path tests.
param(
  [ValidateSet('Seal','Promote')][string]$Mode,
  [string]$Output,
  [string]$RunId = ''
)
$ErrorActionPreference = 'Stop'
$script:SourceRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '../..'))
$script:Policy = Get-Content -Raw -LiteralPath (Join-Path $PSScriptRoot 'windows-candidate.json') | ConvertFrom-Json
$script:MaxBytes = 512MB

function Assert-Equal($Actual, $Expected, [string]$Name) {
  if ($null -eq $Actual -or [string]$Actual -cne [string]$Expected) { throw "Incompatible or missing $Name" }
}
function Get-Digest([string]$Path) { (Get-FileHash -LiteralPath $Path -Algorithm SHA256).Hash.ToLowerInvariant() }
function Write-Json([string]$Path, $Value) {
  [IO.File]::WriteAllText($Path, ($Value | ConvertTo-Json -Depth 30) + "`n", [Text.UTF8Encoding]::new($false))
}
function Get-Expected {
  $commit = (git -C $script:SourceRoot rev-parse HEAD).Trim()
  if ($LASTEXITCODE -ne 0 -or $commit -cnotmatch '^[0-9a-f]{40}$') { throw 'No exact checkout commit' }
  $manifest = Get-Content -Raw -LiteralPath (Join-Path $script:SourceRoot 'Cargo.toml')
  $match = [regex]::Match($manifest, '(?ms)^\[workspace\.package\]\s*.*?^version\s*=\s*"([0-9]+\.[0-9]+\.[0-9]+(?:-[A-Za-z0-9.-]+)?)"')
  if (-not $match.Success) { throw 'Missing workspace version' }
  $inputs = [ordered]@{}
  foreach ($file in @('Cargo.lock','Cargo.toml','.cargo/config.toml','.github/workflows/ci.yml','packaging/ci/windows-candidate.json','packaging/ci/promotion.ps1','packaging/windows/package.ps1')) {
    # Compare canonical Git blobs: checkout CRLF/LF differences must not reject identical source.
    $inputs[$file] = (git -C $script:SourceRoot hash-object "--path=$file" $file).Trim()
    if ($LASTEXITCODE -ne 0 -or $inputs[$file] -cnotmatch '^[0-9a-f]{40}$') { throw "Cannot identify input $file" }
  }
  [ordered]@{ commit=$commit; version=$match.Groups[1].Value; inputs=$inputs }
}
function Get-Api([string]$Path) {
  # Only this repository's GitHub Actions metadata is requested; never a URL from an artifact.
  $json = & gh api --method GET "repos/$($script:Policy.repository)/$Path" 2>$null
  if ($LASTEXITCODE -ne 0) { throw "GitHub metadata unavailable: $Path" }
  $json | ConvertFrom-Json
}
function Assert-Run($Run, $Workflow, $Jobs, [string]$Commit) {
  Assert-Equal $Workflow.id $script:Policy.workflowId 'trusted workflow ID'
  Assert-Equal $Workflow.path $script:Policy.workflow 'workflow path'
  Assert-Equal $Workflow.state 'active' 'workflow state'
  Assert-Equal $Run.workflow_id $Workflow.id 'workflow identity'
  Assert-Equal $Run.path $script:Policy.workflow 'run workflow path'
  Assert-Equal $Run.repository.id $script:Policy.repositoryId 'repository ID'
  Assert-Equal $Run.repository.full_name $script:Policy.repository 'repository name'
  Assert-Equal $Run.head_repository.id $script:Policy.repositoryId 'source repository ID'
  Assert-Equal $Run.head_sha $Commit 'exact source commit'
  Assert-Equal $Run.head_branch 'main' 'trusted branch'
  Assert-Equal $Run.status 'completed' 'run status'
  Assert-Equal $Run.conclusion 'success' 'run result'
  if ($Run.event -cnotin @('push','workflow_dispatch')) { throw 'Untrusted CI event' }
  if ([string]$Run.id -notmatch '^[1-9][0-9]*$' -or [string]$Run.run_attempt -notmatch '^[1-9][0-9]*$') { throw 'Missing run identity/attempt' }
  foreach ($name in $script:Policy.requiredJobs) {
    $found = @($Jobs | Where-Object name -CEQ $name)
    if ($found.Count -ne 1) { throw "Missing or ambiguous required job: $name" }
    Assert-Equal $found[0].conclusion 'success' "job $name result"
    Assert-Equal $found[0].status 'completed' "job $name status"
    Assert-Equal $found[0].head_sha $Commit "job $name commit"
    Assert-Equal $found[0].run_attempt $Run.run_attempt "job $name attempt"
  }
}
function Get-CandidateName($Commit, $Attempt) { "candidate-windows-x64-$Commit-$Attempt" }
function Assert-Artifact($Artifact, $Run) {
  Assert-Equal $Artifact.name (Get-CandidateName $Run.head_sha $Run.run_attempt) 'artifact name/attempt'
  Assert-Equal $Artifact.workflow_run.id $Run.id 'artifact run ID'
  Assert-Equal $Artifact.workflow_run.head_sha $Run.head_sha 'artifact commit'
  Assert-Equal $Artifact.workflow_run.head_branch 'main' 'artifact branch'
  Assert-Equal $Artifact.workflow_run.repository_id $script:Policy.repositoryId 'artifact repository'
  Assert-Equal $Artifact.workflow_run.head_repository_id $script:Policy.repositoryId 'artifact source repository'
  Assert-Equal $Artifact.expired $false 'artifact expiry'
  if ([string]$Artifact.id -notmatch '^[1-9][0-9]*$' -or $Artifact.digest -cnotmatch '^sha256:[0-9a-f]{64}$') { throw 'Missing immutable artifact ID/digest' }
  if ($Artifact.size_in_bytes -le 0 -or $Artifact.size_in_bytes -gt $script:MaxBytes) { throw 'Invalid artifact size' }
}
function Assert-Receipt($Receipt, $Run, $Expected) {
  foreach ($key in @('schema','repository','repositoryId','workflow','workflowId','target','rust','profile','rustflags','requiredWinres')) {
    Assert-Equal $Receipt.$key $script:Policy.$key "receipt $key"
  }
  Assert-Equal $Receipt.commit $Expected.commit 'receipt commit'
  Assert-Equal $Receipt.version $Expected.version 'receipt version'
  Assert-Equal $Receipt.runId $Run.id 'receipt run ID'
  Assert-Equal $Receipt.runAttempt $Run.run_attempt 'receipt run attempt'
  Assert-Equal $Receipt.workflowSha $Expected.commit 'workflow source commit'
  if ($Receipt.googleDesktopConfigured -isnot [bool]) { throw 'Missing desktop build configuration' }
  if (@($Receipt.inputs.PSObject.Properties).Count -ne $Expected.inputs.Count) { throw 'Unexpected/missing build input hashes' }
  foreach ($key in $Expected.inputs.Keys) { Assert-Equal $Receipt.inputs.$key $Expected.inputs[$key] "input $key" }
  [string[]]$names = @($Receipt.files.PSObject.Properties.Name)
  [Array]::Sort($names,[StringComparer]::Ordinal)
  Assert-Equal ($names -join ',') 'mailsolutions-cli.exe,mailsolutions.exe' 'binary inventory'
  foreach ($name in $names) {
    if ($Receipt.files.$name.sha256 -cnotmatch '^[0-9a-f]{64}$' -or $Receipt.files.$name.bytes -le 0 -or $Receipt.files.$name.bytes -gt $script:MaxBytes) { throw 'Invalid binary digest/size' }
  }
}
function Receive-Archive($Artifact, [string]$File) {
  # gh handles GitHub authentication and its storage redirect. Capture binary stdout directly,
  # never through PowerShell's text redirection and never put a credential on a command line.
  $start = [Diagnostics.ProcessStartInfo]::new()
  $start.FileName = 'gh'
  $start.Arguments = "api --method GET repos/$($script:Policy.repository)/actions/artifacts/$($Artifact.id)/zip"
  $start.UseShellExecute = $false
  $start.CreateNoWindow = $true
  $start.RedirectStandardOutput = $true
  $start.RedirectStandardError = $true
  $start.EnvironmentVariables['GH_PROMPT_DISABLED'] = '1'
  $process = [Diagnostics.Process]::Start($start)
  $fileStream = [IO.File]::Open($File,[IO.FileMode]::CreateNew)
  try {
    $stderr = $process.StandardError.ReadToEndAsync()
    $buffer = New-Object byte[] 65536
    $total = 0L
    while ($true) {
      $pending = $process.StandardOutput.BaseStream.ReadAsync($buffer,0,$buffer.Length)
      if (-not $pending.Wait(180000)) { throw 'Artifact download timed out' }
      $count = $pending.Result
      if ($count -eq 0) { break }
      $total += $count
      if ($total -gt $script:MaxBytes) { throw 'Artifact exceeded download limit' }
      $fileStream.Write($buffer,0,$count)
    }
    if (-not $process.WaitForExit(30000) -or $process.ExitCode -ne 0) { throw 'GitHub artifact download failed' }
  } finally {
    $fileStream.Dispose()
    # Only the short-lived gh child created above, never another app or user session.
    if (-not $process.HasExited) { $process.Kill() }
    $process.Dispose()
  }
}

function Expand-VerifiedCandidate([string]$Archive, $Artifact, $Run, $Expected, [string]$Destination) {
  Assert-Artifact $Artifact $Run
  Assert-Equal (Get-Digest $Archive) $Artifact.digest.Substring(7) 'GitHub archive SHA256'
  Assert-Equal (Get-Item -LiteralPath $Archive).Length $Artifact.size_in_bytes 'archive length'
  if (Test-Path -LiteralPath $Destination) { throw 'Output already exists; preserve prior evidence' }
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
  try {
    [string[]]$names = @($zip.Entries | ForEach-Object FullName)
    [Array]::Sort($names,[StringComparer]::Ordinal)
    Assert-Equal ($names -join ',') 'candidate.json,mailsolutions-cli.exe,mailsolutions.exe' 'archive inventory'
    if (($zip.Entries | Measure-Object Length -Sum).Sum -gt $script:MaxBytes) { throw 'Expanded artifact too large' }
    $metadata = $zip.GetEntry('candidate.json')
    if ($metadata.Length -gt 65536) { throw 'Receipt too large' }
    $reader = [IO.StreamReader]::new($metadata.Open())
    try { $receipt = $reader.ReadToEnd() | ConvertFrom-Json } finally { $reader.Dispose() }
    Assert-Receipt $receipt $Run $Expected
    $bin = Join-Path $Destination "$($script:Policy.target)/release"
    [void][IO.Directory]::CreateDirectory($bin)
    foreach ($name in @('mailsolutions.exe','mailsolutions-cli.exe')) {
      $entry = $zip.GetEntry($name)
      # Extract only fixed, checked names; never follow archive paths or execute their contents.
      $file = Join-Path $bin $name
      [IO.Compression.ZipFileExtensions]::ExtractToFile($entry,$file,$false)
      Assert-Equal (Get-Digest $file) $receipt.files.$name.sha256 "$name digest"
      Assert-Equal (Get-Item -LiteralPath $file).Length $receipt.files.$name.bytes "$name size"
    }
    Write-Json (Join-Path $Destination 'ci-build.json') ([ordered]@{
      receipt=$receipt; ciRunUrl=$Run.html_url; artifactId=$Artifact.id; artifactArchiveSha256=$Artifact.digest
      ciWorkflowId=$Run.workflow_id; sourceRepositoryId=$Run.repository.id
    })
  } finally { $zip.Dispose() }
}
function Seal-Candidate([string]$Destination) {
  git -C $script:SourceRoot diff --quiet HEAD --
  if ($LASTEXITCODE -ne 0) { throw 'Candidate checkout has uncommitted changes' }
  $expected = Get-Expected
  Assert-Equal $env:GITHUB_REPOSITORY $script:Policy.repository 'build repository'
  Assert-Equal $env:GITHUB_REPOSITORY_ID $script:Policy.repositoryId 'build repository ID'
  Assert-Equal $env:GITHUB_REF 'refs/heads/main' 'build branch'
  Assert-Equal $env:GITHUB_SHA $expected.commit 'build checkout'
  Assert-Equal $env:GITHUB_WORKFLOW_SHA $expected.commit 'build workflow commit'
  Assert-Equal $env:GITHUB_WORKFLOW_REF "$($script:Policy.repository)/$($script:Policy.workflow)@refs/heads/main" 'build workflow path'
  if ($env:GITHUB_EVENT_NAME -cnotin @('push','workflow_dispatch')) { throw 'Untrusted build event' }
  if ([string]$env:GITHUB_RUN_ID -notmatch '^[1-9][0-9]*$' -or [string]$env:GITHUB_RUN_ATTEMPT -notmatch '^[1-9][0-9]*$') { throw 'Missing build run identity' }
  $rust = & rustc --version
  if ($LASTEXITCODE -ne 0 -or $rust -notmatch ('^rustc '+[regex]::Escape($script:Policy.rust)+' ')) { throw 'Unexpected compiler version' }
  Assert-Equal $env:CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS $script:Policy.rustflags 'effective target flags'
  Assert-Equal $env:MAILSOLUTIONS_REQUIRE_WINRES '1' 'required Windows resources'
  Assert-Equal $env:CARGO_INCREMENTAL '0' 'non-incremental production build'
  if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS -or $env:CARGO_TARGET_DIR) { throw 'Unexpected production build override' }
  $client = -not [string]::IsNullOrEmpty($env:MAILSOLUTIONS_GOOGLE_CLIENT_ID)
  $clientSecret = -not [string]::IsNullOrEmpty($env:MAILSOLUTIONS_GOOGLE_CLIENT_SECRET)
  if ($client -ne $clientSecret) { throw 'Incomplete desktop OAuth build configuration' }
  if (Test-Path -LiteralPath $Destination) { throw 'Candidate output already exists' }
  [void][IO.Directory]::CreateDirectory($Destination)
  $files = [ordered]@{}
  foreach ($name in @('mailsolutions.exe','mailsolutions-cli.exe')) {
    $file = Join-Path $script:SourceRoot "target/$($script:Policy.target)/release/$name"
    Copy-Item -LiteralPath $file -Destination (Join-Path $Destination $name)
    $files[$name] = @{sha256=(Get-Digest $file);bytes=(Get-Item -LiteralPath $file).Length}
  }
  $receipt = [ordered]@{}
  foreach ($key in @('schema','repository','repositoryId','workflow','workflowId','target','rust','profile','rustflags','requiredWinres')) { $receipt[$key]=$script:Policy.$key }
  $receipt.commit=$expected.commit; $receipt.version=$expected.version; $receipt.inputs=$expected.inputs
  $receipt.runId=$env:GITHUB_RUN_ID; $receipt.runAttempt=$env:GITHUB_RUN_ATTEMPT; $receipt.workflowSha=$env:GITHUB_WORKFLOW_SHA
  $receipt.googleDesktopConfigured=$client; $receipt.files=$files
  Write-Json (Join-Path $Destination 'candidate.json') $receipt
}
function Promote-Candidate([string]$Destination, [string]$SelectedRun) {
  git -C $script:SourceRoot diff --quiet HEAD --
  if ($LASTEXITCODE -ne 0) { throw 'Promotion checkout has uncommitted changes' }
  $expected = Get-Expected
  Assert-Equal $env:GITHUB_REPOSITORY $script:Policy.repository 'promotion repository'
  Assert-Equal $env:GITHUB_REPOSITORY_ID $script:Policy.repositoryId 'promotion repository ID'
  Assert-Equal $env:GITHUB_SHA $expected.commit 'promotion checkout'
  $workflow = Get-Api 'actions/workflows/ci.yml'
  if ($SelectedRun) {
    if ($SelectedRun -notmatch '^[1-9][0-9]*$') { throw 'Invalid CI run ID' }
  } else {
    $runs = Get-Api "actions/workflows/ci.yml/runs?head_sha=$($expected.commit)&branch=main&status=success&per_page=100"
    $trusted = @($runs.workflow_runs | Where-Object { $_.event -cin @('push','workflow_dispatch') } | Sort-Object id -Descending)
    if (-not $trusted.Count) { throw 'No successful exact-commit main CI; do not rebuild during release' }
    $SelectedRun = [string]$trusted[0].id
  }
  $run = Get-Api "actions/runs/$SelectedRun"
  $jobs = Get-Api "actions/runs/$SelectedRun/attempts/$($run.run_attempt)/jobs?per_page=100"
  if ($jobs.total_count -gt 100) { throw 'Unexpected job count' }
  Assert-Run $run $workflow $jobs.jobs $expected.commit
  $inventory = Get-Api "actions/runs/$SelectedRun/artifacts?per_page=100"
  if ($inventory.total_count -gt 100) { throw 'Unexpected artifact count' }
  $artifacts = @($inventory.artifacts | Where-Object name -CEQ (Get-CandidateName $run.head_sha $run.run_attempt))
  if ($artifacts.Count -ne 1) { throw 'Missing/ambiguous immutable candidate; no release-time build fallback' }
  $artifact = Get-Api "actions/artifacts/$($artifacts[0].id)"
  Assert-Artifact $artifact $run
  $archive = Join-Path $env:RUNNER_TEMP "candidate-$($artifact.id)-$([guid]::NewGuid()).zip"
  Receive-Archive $artifact $archive
  Expand-VerifiedCandidate $archive $artifact $run $expected $Destination
  Write-Output "Verified exact-commit CI run $SelectedRun, artifact $($artifact.id), $($artifact.digest)"
}
if ($Mode -eq 'Seal') { Seal-Candidate $Output }
if ($Mode -eq 'Promote') { Promote-Candidate $Output $RunId }
