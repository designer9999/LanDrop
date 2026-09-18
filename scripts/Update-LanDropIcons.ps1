param(
    [string]$CliProjectPath = (Split-Path $PSScriptRoot -Parent)
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$master = Join-Path $repo 'assets\branding\landrop-icon.png'
$output = Join-Path ([System.IO.Path]::GetTempPath()) ('landrop-icons-' + [guid]::NewGuid().ToString('N'))

# Use Windows Node and this project's installed, pinned Tauri CLI. Never download
# an unpinned generator or mix Windows and WSL dependency trees.
Add-Type -AssemblyName System.Drawing
$image = [System.Drawing.Bitmap]::new($master)
try {
    if ($image.Width -ne $image.Height -or $image.Width -lt 512) {
        throw 'The icon master must be square and at least 512 pixels wide.'
    }
    if ($image.GetPixel(0, 0).A -ne 0) {
        throw 'The icon master must have genuinely transparent rounded corners.'
    }
} finally {
    $image.Dispose()
}

Push-Location $CliProjectPath
try {
    & npm.cmd run tauri -- icon $master --output $output
    if ($LASTEXITCODE -ne 0) { throw "Tauri icon generation failed: $LASTEXITCODE" }
} finally {
    Pop-Location
}

$desktopFiles = @('32x32.png', '64x64.png', '128x128.png', '128x128@2x.png', 'icon.png', 'icon.ico', 'icon.icns')
foreach ($name in $desktopFiles) {
    if (-not (Test-Path (Join-Path $output $name))) { throw "Missing generated icon: $name" }
}
# Update only existing Android launcher assets; do not overwrite native project
# configuration, signing, themes, or resources unrelated to branding.
$androidRes = Join-Path $repo 'src-tauri\gen\android\app\src\main\res'
foreach ($density in @('mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi')) {
    foreach ($name in @('ic_launcher.png', 'ic_launcher_round.png', 'ic_launcher_foreground.png')) {
        $generated = Join-Path $output "android\mipmap-$density\$name"
        if (-not (Test-Path $generated)) { throw "Missing generated Android icon: $generated" }
    }
}
# Preflight all expected output files before replacing any repository assets.
foreach ($name in $desktopFiles) {
    Copy-Item (Join-Path $output $name) (Join-Path $repo "src-tauri\icons\$name") -Force
}
Copy-Item (Join-Path $output '128x128.png') (Join-Path $repo 'public\app-icon.png') -Force
foreach ($density in @('mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi')) {
    foreach ($name in @('ic_launcher.png', 'ic_launcher_round.png', 'ic_launcher_foreground.png')) {
        Copy-Item (Join-Path $output "android\mipmap-$density\$name") (Join-Path $androidRes "mipmap-$density\$name") -Force
    }
}
Write-Output "Updated desktop, in-app, notification, and Android launcher assets from $master"
Write-Output "Generated intermediate files retained at $output"
