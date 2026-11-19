# Offline fixtures only: no GitHub calls, real binaries, signing credentials or profiles.
$ErrorActionPreference = 'Stop'
. (Join-Path $PSScriptRoot 'promotion.ps1')
$base = Join-Path ([IO.Path]::GetTempPath()) ('mailsolutions-promotion-' + [guid]::NewGuid())
[void][IO.Directory]::CreateDirectory($base)
$script:Count = 0
function Copy-Json($Value) { $Value | ConvertTo-Json -Depth 30 | ConvertFrom-Json }
function Must-Fail([string]$Name, [scriptblock]$Action) {
  $failed = $false
  try { & $Action | Out-Null } catch { $failed = $true }
  if (-not $failed) { throw "Accepted invalid $Name" }
  $script:Count++
}
try {
  $expected = Get-Expected
  $commit = $expected.commit
  $workflow = [pscustomobject]@{id=419588732;path='.github/workflows/ci.yml';state='active'}
  $repo = [pscustomobject]@{id=1458034748;full_name='william-pyle/MailSolutions'}
  $run = [pscustomobject]@{id=123;run_attempt=1;workflow_id=419588732;path='.github/workflows/ci.yml';repository=$repo;head_repository=$repo;head_sha=$commit;head_branch='main';status='completed';conclusion='success';event='push';html_url='https://github.com/william-pyle/MailSolutions/actions/runs/123'}
  $jobs = @($script:Policy.requiredJobs | ForEach-Object { [pscustomobject]@{name=$_;status='completed';conclusion='success';head_sha=$commit;run_attempt=1} })
  Assert-Run $run $workflow $jobs $commit
  foreach ($pair in @(@('workflow_id',43),@('path','.github/workflows/other.yml'),@('head_sha',('f'*40)),@('head_branch','feature'),@('status','in_progress'),@('conclusion','failure'),@('event','pull_request'),@('run_attempt',2),@('id',0))) {
    $bad = Copy-Json $run; $bad.($pair[0])=$pair[1]
    Must-Fail "run $($pair[0])" { Assert-Run $bad $workflow $jobs $commit }
  }
  $bad = Copy-Json $run; $bad.repository.id=9
  Must-Fail 'foreign repository' { Assert-Run $bad $workflow $jobs $commit }
  $bad = Copy-Json $run; $bad.head_repository.id=9
  Must-Fail 'fork source' { Assert-Run $bad $workflow $jobs $commit }
  $bad = Copy-Json $workflow; $bad.state='disabled_manually'
  Must-Fail 'disabled workflow' { Assert-Run $run $bad $jobs $commit }
  foreach ($state in @('failure','cancelled','skipped','neutral','')) {
    $bad = Copy-Json $jobs; $bad[0].conclusion=$state
    Must-Fail "job $state" { Assert-Run $run $workflow $bad $commit }
  }
  Must-Fail 'missing ci-ok' { Assert-Run $run $workflow $jobs[0..2] $commit }
  Must-Fail 'duplicate ci-ok' { Assert-Run $run $workflow ($jobs + $jobs[-1]) $commit }
  $artifact = [pscustomobject]@{id=555;name=(Get-CandidateName $commit 1);expired=$false;digest=('sha256:'+('a'*64));size_in_bytes=100;workflow_run=[pscustomobject]@{id=123;head_sha=$commit;head_branch='main';repository_id=1458034748;head_repository_id=1458034748}}
  Assert-Artifact $artifact $run
  foreach ($pair in @(@('expired',$true),@('name','candidate-windows-x64-other-1'),@('digest',''),@('size_in_bytes',0),@('size_in_bytes',1GB),@('id',0))) {
    $bad = Copy-Json $artifact; $bad.($pair[0])=$pair[1]
    Must-Fail "artifact $($pair[0])" { Assert-Artifact $bad $run }
  }
  foreach ($key in @('id','head_sha','head_branch','repository_id','head_repository_id')) {
    $bad = Copy-Json $artifact; $bad.workflow_run.$key='0'
    Must-Fail "artifact source $key" { Assert-Artifact $bad $run }
  }
  $receipt = [ordered]@{}
  foreach ($key in @('schema','repository','repositoryId','workflow','workflowId','target','rust','profile','rustflags','requiredWinres')) { $receipt[$key]=$script:Policy.$key }
  $receipt.commit=$commit; $receipt.version=$expected.version; $receipt.inputs=$expected.inputs
  $receipt.runId=123; $receipt.runAttempt=1; $receipt.workflowSha=$commit; $receipt.googleDesktopConfigured=$false
  $receipt.files=[ordered]@{}
  foreach ($name in @('mailsolutions.exe','mailsolutions-cli.exe')) {
    $file=Join-Path $base $name
    [IO.File]::WriteAllText($file,"synthetic $name")
    $receipt.files[$name]=@{sha256=(Get-Digest $file);bytes=(Get-Item -LiteralPath $file).Length}
  }
  $receipt = Copy-Json $receipt
  Assert-Receipt $receipt $run $expected
  foreach ($key in @('schema','repository','repositoryId','workflow','workflowId','target','rust','profile','rustflags','requiredWinres','commit','version','runId','runAttempt','workflowSha','googleDesktopConfigured')) {
    $bad=Copy-Json $receipt; $bad.$key='incompatible'
    Must-Fail "receipt $key" { Assert-Receipt $bad $run $expected }
  }
  foreach ($key in $expected.inputs.Keys) {
    $bad=Copy-Json $receipt; $bad.inputs.$key='0'*64
    Must-Fail "changed input $key" { Assert-Receipt $bad $run $expected }
  }
  $bad=Copy-Json $receipt; $bad.inputs.PSObject.Properties.Remove('Cargo.lock')
  Must-Fail 'missing locked inputs' { Assert-Receipt $bad $run $expected }
  Add-Type -AssemblyName System.IO.Compression
  Add-Type -AssemblyName System.IO.Compression.FileSystem
  function Make-Archive([string]$Name, $Metadata, [string]$Extra='') {
    $path=Join-Path $base "$Name.zip"
    $zip=[IO.Compression.ZipFile]::Open($path,[IO.Compression.ZipArchiveMode]::Create)
    try {
      foreach ($file in @('mailsolutions.exe','mailsolutions-cli.exe')) { [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip,(Join-Path $base $file),$file) }
      $entry=$zip.CreateEntry('candidate.json')
      $writer=[IO.StreamWriter]::new($entry.Open())
      try { $writer.Write(($Metadata | ConvertTo-Json -Depth 30)) } finally { $writer.Dispose() }
      if ($Extra) { [void]$zip.CreateEntry($Extra) }
    } finally { $zip.Dispose() }
    return $path
  }
  $archive=Make-Archive 'good' $receipt
  $artifact.digest='sha256:'+(Get-Digest $archive); $artifact.size_in_bytes=(Get-Item -LiteralPath $archive).Length
  Expand-VerifiedCandidate $archive $artifact $run $expected (Join-Path $base 'accepted')
  if (-not (Test-Path -LiteralPath (Join-Path $base 'accepted/ci-build.json'))) { throw 'No verified source evidence' }
  Must-Fail 'overwrite existing evidence' { Expand-VerifiedCandidate $archive $artifact $run $expected (Join-Path $base 'accepted') }
  $bad=Copy-Json $artifact; $bad.digest='sha256:'+('0'*64)
  Must-Fail 'tampered archive' { Expand-VerifiedCandidate $archive $bad $run $expected (Join-Path $base 'bad-digest') }
  foreach ($extra in @('../outside','/absolute','C:/outside','nested/extra','mailsolutions.exe')) {
    $path=Make-Archive ([guid]::NewGuid().ToString()) $receipt $extra
    $bad=Copy-Json $artifact; $bad.digest='sha256:'+(Get-Digest $path); $bad.size_in_bytes=(Get-Item -LiteralPath $path).Length
    Must-Fail "unsafe/duplicate/extra archive $extra" { Expand-VerifiedCandidate $path $bad $run $expected (Join-Path $base ([guid]::NewGuid().ToString())) }
  }
  $badReceipt=Copy-Json $receipt; $badReceipt.files.'mailsolutions.exe'.sha256='0'*64
  $path=Make-Archive 'bad-file' $badReceipt
  $bad=Copy-Json $artifact; $bad.digest='sha256:'+(Get-Digest $path); $bad.size_in_bytes=(Get-Item -LiteralPath $path).Length
  Must-Fail 'binary digest mismatch' { Expand-VerifiedCandidate $path $bad $run $expected (Join-Path $base 'bad-file') }
  Write-Output "promotion: valid synthetic artifact passed; $script:Count negative cases rejected; no network or program execution"
} finally {
  $resolved=[IO.Path]::GetFullPath($base)
  $temp=[IO.Path]::GetFullPath([IO.Path]::GetTempPath())
  if (-not $resolved.StartsWith($temp,[StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($resolved) -notmatch '^mailsolutions-promotion-[0-9a-f-]+$') { throw 'Unsafe fixture cleanup target' }
  Remove-Item -LiteralPath $resolved -Recurse -Force
}
