# Ebb CLI installer for Windows (PowerShell).
#   irm https://raw.githubusercontent.com/ege-arhan/ebb/main/install.ps1 | iex
# Installs `ebb.exe` to %LOCALAPPDATA%\Ebb\bin and adds it to your user PATH.
# To also connect your AI assistants afterwards, run:  ebb connect --all
$ErrorActionPreference = "Stop"
$repo = "ege-arhan/ebb"
$dir = Join-Path $env:LOCALAPPDATA "Ebb\bin"
$tag = (Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest").tag_name
$url = "https://github.com/$repo/releases/download/$tag/ebb-cli-$tag-x86_64-pc-windows-msvc.zip"
$tmp = Join-Path $env:TEMP "ebb-install"
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory $tmp | Out-Null
Write-Host "Downloading Ebb $tag …"
Invoke-WebRequest $url -OutFile "$tmp\ebb.zip"
Expand-Archive "$tmp\ebb.zip" -DestinationPath $tmp -Force
New-Item -ItemType Directory $dir -Force | Out-Null
Copy-Item "$tmp\ebb.exe" "$dir\ebb.exe" -Force
$path = [Environment]::GetEnvironmentVariable("Path", "User")
if (($path -split ";") -notcontains $dir) { [Environment]::SetEnvironmentVariable("Path", "$path;$dir", "User") }
Write-Host "Installed: $dir\ebb.exe  (open a new terminal so PATH updates)"
& "$dir\ebb.exe" --version
