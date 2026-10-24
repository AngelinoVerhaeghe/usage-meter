# Install (or reinstall) Usage Meter on Windows, without admin rights:
#   irm https://raw.githubusercontent.com/nealwebdev/usage-meter/main/scripts/install-windows.ps1 | iex
#
# Downloads the latest installer, clears the "downloaded from the internet" mark
# so SmartScreen doesn't stop the unsigned installer, and installs silently for
# this user. After this, the app updates itself.
$ErrorActionPreference = "Stop"
$repo = if ($env:USAGE_METER_REPO) { $env:USAGE_METER_REPO } else { "nealwebdev/usage-meter" }

$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -like "*-setup.exe" } | Select-Object -First 1
if (-not $asset) { throw "No Windows build found in the latest release of $repo" }

$file = Join-Path $env:TEMP $asset.name
Write-Host "Downloading Usage Meter..."
Invoke-WebRequest $asset.browser_download_url -OutFile $file -UseBasicParsing
Unblock-File $file

Get-Process | Where-Object { $_.Path -like "*\Usage Meter\*" } | Stop-Process -Force -ErrorAction SilentlyContinue
Start-Process $file -ArgumentList "/S" -Wait
Remove-Item $file -ErrorAction SilentlyContinue

$exe = Get-ChildItem (Join-Path $env:LOCALAPPDATA "Usage Meter") -Filter *.exe -ErrorAction SilentlyContinue |
  Where-Object { $_.Name -notlike "uninstall*" } | Select-Object -First 1
if ($exe) { Start-Process $exe.FullName }
Write-Host ""
Write-Host "Done! Usage Meter is in the tray next to the clock (bottom right)."
Write-Host "Don't see it? Click the ^ arrow next to the clock. You can close this window now."
