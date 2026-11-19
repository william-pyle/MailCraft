param([Parameter(Mandatory=$true)][string]$Msi)
$ErrorActionPreference = 'Stop'
# Read-only Windows Installer database inspection: never install, repair or uninstall.
$installer = New-Object -ComObject WindowsInstaller.Installer
$database = $installer.OpenDatabase((Resolve-Path -LiteralPath $Msi).Path, 0)
function Read-MsiRows([string]$Query, [int]$Columns) {
  $view = $database.OpenView($Query)
  [void]$view.Execute()
  while ($record = $view.Fetch()) {
    $values = for ($i = 1; $i -le $Columns; $i++) { $record.StringData($i) }
    ,$values
  }
  [void]$view.Close()
}
$properties = @{}
Read-MsiRows 'SELECT `Property`, `Value` FROM `Property`' 2 | ForEach-Object { $properties[$_[0]] = $_[1] }
if ($properties.ProductName -ne 'MailSolutions') { throw 'Wrong product name' }
if ($properties.UpgradeCode -ne '{647FB3CC-7876-4DC8-84D9-CE2DD295D1C7}') { throw 'Upgrade family changed' }
$files = @(Read-MsiRows 'SELECT `FileName`, `Version` FROM `File`' 2)
foreach ($name in 'mailsolutions.exe', 'mailsolutions-cli.exe') {
  $file = @($files | Where-Object { ($_[0] -split '\|')[-1] -eq $name })
  if ($file.Count -ne 1) { throw "Missing or duplicated $name" }
  # The GUI has VERSIONINFO; the console CLI reports its version through --version.
  if ($name -eq 'mailsolutions.exe' -and -not $file[0][1]) { throw 'GUI version resource missing' }
  if ($file[0][1] -and -not $file[0][1].StartsWith($properties.ProductVersion + '.')) { throw "Version mismatch for $name" }
}
$directories = @(Read-MsiRows 'SELECT `Directory`, `DefaultDir` FROM `Directory`' 2)
if (-not ($directories | Where-Object { $_[0] -eq 'INSTALLFOLDER' -and ($_[1] -split '\|')[-1] -eq 'MailSolutions' })) { throw 'Wrong install directory' }
$registry = @(Read-MsiRows 'SELECT `Key`, `Name`, `Value` FROM `Registry`' 3)
if (-not ($registry | Where-Object { $_[0] -eq 'Software\RegisteredApplications' -and ($_[1] -split '\|')[-1] -eq 'MailSolutions' })) { throw 'Missing registered application' }
[ordered]@{ product=$properties.ProductName; version=$properties.ProductVersion; upgradeCode=$properties.UpgradeCode; productCode=$properties.ProductCode; files=$files; directories=$directories; registry=$registry; installed=$false } | ConvertTo-Json -Depth 8
