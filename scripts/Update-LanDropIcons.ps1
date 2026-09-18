param(
    [string]$CliProjectPath = (Split-Path $PSScriptRoot -Parent),
    [switch]$AndroidDensityRepairOnly
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path $PSScriptRoot -Parent
$master = Join-Path $repo 'assets\branding\landrop-icon.png'
$output = Join-Path ([System.IO.Path]::GetTempPath()) ('landrop-icons-' + [guid]::NewGuid().ToString('N'))
$androidRes = Join-Path $repo 'src-tauri\gen\android\app\src\main\res'

function Repair-AndroidHdpi([string]$ResourcePath) {
    # CLI 2.11.4's Android table mistakenly emits 49px at hdpi, not 72px.
    # Downsample its correctly masked 192px variants with the same pinned CLI;
    # this preserves the approved artwork and avoids enlarging the broken 49px.
    # https://github.com/tauri-apps/tauri/blob/tauri-cli-v2.11.4/crates/tauri-cli/src/icon.rs#L393
    foreach ($name in @('ic_launcher.png', 'ic_launcher_round.png')) {
        $source = Join-Path $ResourcePath "mipmap-xxxhdpi\$name"
        $sourceImage = [System.Drawing.Bitmap]::new($source)
        try {
            if ($sourceImage.Width -ne 192 -or $sourceImage.Height -ne 192) {
                throw "Expected 192px source launcher asset: $source"
            }
        } finally { $sourceImage.Dispose() }
        $custom = Join-Path $output ('hdpi-' + [IO.Path]::GetFileNameWithoutExtension($name))
        Push-Location $CliProjectPath
        try {
            & npm.cmd run tauri -- icon $source --png 72 --output $custom
            if ($LASTEXITCODE -ne 0) { throw 'HDPI launcher generation failed' }
        } finally { Pop-Location }
        $corrected = Join-Path $custom '72x72.png'
        $check = [System.Drawing.Bitmap]::new($corrected)
        try {
            if ($check.Width -ne 72 -or $check.Height -ne 72) { throw 'HDPI launcher must be 72px' }
        } finally { $check.Dispose() }
        Copy-Item -LiteralPath $corrected -Destination (Join-Path $ResourcePath "mipmap-hdpi\$name") -Force
    }
}

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

if ($AndroidDensityRepairOnly) {
    Repair-AndroidHdpi $androidRes
    Write-Output 'Corrected only the two Android HDPI launcher assets to 72px; desktop artwork unchanged.'
    return
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
Repair-AndroidHdpi (Join-Path $output 'android')
foreach ($density in @('mdpi', 'hdpi', 'xhdpi', 'xxhdpi', 'xxxhdpi')) {
    foreach ($name in @('ic_launcher.png', 'ic_launcher_round.png', 'ic_launcher_foreground.png')) {
        $generated = Join-Path $output "android\mipmap-$density\$name"
        if (-not (Test-Path $generated)) { throw "Missing generated Android icon: $generated" }
        $scale = @{ mdpi = 1; hdpi = 1.5; xhdpi = 2; xxhdpi = 3; xxxhdpi = 4 }[$density]
        $expectedSize = [int]($(if ($name -eq 'ic_launcher_foreground.png') { 108 } else { 48 }) * $scale)
        $generatedImage = [System.Drawing.Bitmap]::new($generated)
        try {
            if ($generatedImage.Width -ne $expectedSize -or $generatedImage.Height -ne $expectedSize) {
                throw "Wrong Android icon dimensions: $generated"
            }
        } finally { $generatedImage.Dispose() }
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
