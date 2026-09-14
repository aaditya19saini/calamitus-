$repo = "aaditya19saini/calamitus-"
$installDir = "$env:LOCALAPPDATA\launch"
New-Item -ItemType Directory -Force -Path $installDir | Out-Null

$release = Invoke-RestMethod "https://api.github.com/repos/$repo/releases/latest"
$asset = $release.assets | Where-Object { $_.name -eq "launch.exe" }
Invoke-WebRequest -Uri $asset.browser_download_url -OutFile "$installDir\launch.exe"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    [Environment]::SetEnvironmentVariable("Path", "$userPath;$installDir", "User")
    Write-Host "Added $installDir to PATH. Restart your terminal to use it."
}
Write-Host "launch installed. Try: launch notepad"
