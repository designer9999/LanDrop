param(
    [ValidateSet('Initialize', 'Sign')]
    [Parameter(Mandatory = $true)][string]$Action,
    [string]$Apk,
    [string]$OutputApk,
    [string]$NodePath,
    [string]$JavaHome = 'C:\Program Files\Android\Android Studio\jbr',
    [string]$AndroidSdk = "$env:LOCALAPPDATA\Android\Sdk"
)

# Windows-only release custody. No plaintext password file or repository key.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Security
$originalJavaHome = [Environment]::GetEnvironmentVariable('JAVA_HOME', 'Process')
$stagedOutput = $null
$applicationId = 'io.github.designer9999.landrop'
# Public trust anchors, not signing secrets. Intentional key rotation requires
# reviewing this pin and the release environment's expected certificate together.
$approvedFingerprint = 'ab80892384f01e61d3a1c6b9387f9c65111b46bccad383f254415c5f9e9c0c03'
$revokedFingerprint = 'd55ee2d0dbbc31bdb14fad8bb15d0c9104f829f0d169aa8afd53e4598c996cb9'
$vault = Join-Path $env:LOCALAPPDATA 'LanDropRelease\Android\io.github.designer9999.landrop\signing'
$keystore = Join-Path $vault 'release.p12'
$protectedPassword = Join-Path $vault 'password.dpapi'
$certificate = Join-Path $vault 'certificate.der'
$alias = 'landrop-android-2026'
$keytool = Join-Path $JavaHome 'bin\keytool.exe'
if (!(Test-Path -LiteralPath $keytool -PathType Leaf)) { throw 'Java keytool unavailable' }

function Assert-ApkMetadata {
    param([string]$Aapt, [string]$Path, [string]$Version)
    $lines = & $Aapt dump badging $Path
    if ($LASTEXITCODE -ne 0) { throw 'Unable to inspect APK metadata' }
    $metadata = $lines -join "`n"
    $packageLines = @($lines | Where-Object { $_ -match '^package: ' })
    if ($packageLines.Count -ne 1) { throw 'APK has missing or ambiguous package metadata' }
    $attributes = @{}
    foreach ($match in [regex]::Matches($packageLines[0], "(\w+)='([^']*)'")) {
        $attributes[$match.Groups[1].Value] = $match.Groups[2].Value
    }
    $semver = [regex]::Match($Version, '^(\d+)\.(\d+)\.(\d+)$')
    if (!$semver.Success) { throw 'Android release requires a stable source version' }
    $versionCode = [decimal]$semver.Groups[1].Value * 1000000 +
        [decimal]$semver.Groups[2].Value * 1000 + [decimal]$semver.Groups[3].Value
    if ($versionCode -le 0 -or $versionCode -gt 2100000000) { throw 'Invalid Android source versionCode' }
    if ($attributes['name'] -cne $applicationId -or
        $attributes['versionName'] -cne $Version -or
        $attributes['versionCode'] -cne $versionCode.ToString([Globalization.CultureInfo]::InvariantCulture)) {
        throw 'APK identity/version does not match the new Android release source'
    }
    if ($metadata -notmatch "(?m)^sdkVersion:'24'\r?$" -or
        $metadata -notmatch "(?m)^targetSdkVersion:'36'\r?$" -or
        $metadata -notmatch "(?m)^native-code: 'arm64-v8a'\s*$" -or
        $metadata -match '(?m)^application-debuggable') {
        throw 'APK must be non-debuggable, arm64-only, minSdk24 and targetSdk36'
    }
    return $metadata
}

try {
    if ($Action -eq 'Initialize') {
        if (Test-Path -LiteralPath $vault) { throw 'Signing vault already exists; refusing to overwrite it' }
        $directory = New-Item -ItemType Directory -Path $vault
        $acl = New-Object System.Security.AccessControl.DirectorySecurity
        $acl.SetAccessRuleProtection($true, $false)
        $identity = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
        foreach ($sid in @($identity, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
            $rule = [System.Security.AccessControl.FileSystemAccessRule]::new(
                $sid, 'FullControl', 'ContainerInherit,ObjectInherit', 'None', 'Allow')
            $acl.AddAccessRule($rule)
        }
        Set-Acl -LiteralPath $directory.FullName -AclObject $acl
        $random = New-Object byte[] 48
        $rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
        try { $rng.GetBytes($random) } finally { $rng.Dispose() }
        $password = [Convert]::ToBase64String($random)
        [Array]::Clear($random, 0, $random.Length)
        $bytes = [Text.Encoding]::UTF8.GetBytes($password)
        try {
            $encrypted = [System.Security.Cryptography.ProtectedData]::Protect(
                $bytes, $null, [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
            [IO.File]::WriteAllBytes($protectedPassword, $encrypted)
        } finally { [Array]::Clear($bytes, 0, $bytes.Length) }
        $env:LANDROP_ANDROID_SIGN_PASSWORD = $password
        & $keytool -genkeypair -keystore $keystore -storetype PKCS12 -alias $alias `
            -keyalg RSA -keysize 4096 -sigalg SHA256withRSA -validity 10000 `
            -dname 'CN=LanDrop Android Release' -storepass:env LANDROP_ANDROID_SIGN_PASSWORD `
            -keypass:env LANDROP_ANDROID_SIGN_PASSWORD -noprompt
        if ($LASTEXITCODE -ne 0) { throw 'Android signing-key generation failed; vault preserved for inspection' }
        & $keytool -exportcert -keystore $keystore -alias $alias `
            -storepass:env LANDROP_ANDROID_SIGN_PASSWORD -file $certificate
        if ($LASTEXITCODE -ne 0) { throw 'Certificate export failed' }
    } else {
        if (!$Apk -or !$OutputApk) { throw 'Sign requires explicit input and output APK paths' }
        if (!(Test-Path -LiteralPath $Apk -PathType Leaf) -or (Test-Path -LiteralPath $OutputApk)) {
            throw 'Input APK missing or output already exists; refusing overwrite'
        }
        $Apk = [IO.Path]::GetFullPath($Apk)
        $OutputApk = [IO.Path]::GetFullPath($OutputApk)
        $outputDirectory = Split-Path -Parent $OutputApk
        if (!(Test-Path -LiteralPath $outputDirectory -PathType Container)) { throw 'Output directory does not exist' }
        $buildTools = Join-Path $AndroidSdk 'build-tools\36.0.0'
        $zipalign = Join-Path $buildTools 'zipalign.exe'
        $apksigner = Join-Path $buildTools 'apksigner.bat'
        $aapt = Join-Path $buildTools 'aapt.exe'
        foreach ($tool in @($zipalign, $apksigner, $aapt)) {
            if (!(Test-Path -LiteralPath $tool -PathType Leaf)) { throw 'Pinned Android Build Tools36.0.0 unavailable' }
        }
        $repoRoot = Split-Path -Parent $PSScriptRoot
        $baseConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\tauri.conf.json') -Raw | ConvertFrom-Json
        $androidConfig = Get-Content -LiteralPath (Join-Path $repoRoot 'src-tauri\tauri.android.conf.json') -Raw | ConvertFrom-Json
        if ($baseConfig.identifier -cne 'com.landrop.app' -or $androidConfig.identifier -cne $applicationId) {
            throw 'Desktop/Android source identities are inconsistent'
        }
        $sourceVersion = if ($androidConfig.version) { $androidConfig.version } else { $baseConfig.version }
        $unsignedMetadata = Assert-ApkMetadata -Aapt $aapt -Path $Apk -Version $sourceVersion
        & $zipalign -c -P 16 4 $Apk
        if ($LASTEXITCODE -ne 0) { throw 'Input APK is not 16-KB ZIP-aligned; do not sign it' }
        Write-Output 'PASS: unsigned APK 16-KB ZIP alignment'
        if (!$NodePath) {
            $NodePath = (Get-Command -Name node.exe -CommandType Application -ErrorAction Stop | Select-Object -First 1).Source
        }
        $readelf = Join-Path $AndroidSdk 'ndk\27.0.12077973\toolchains\llvm\prebuilt\windows-x86_64\bin\llvm-readelf.exe'
        $jar = Join-Path $JavaHome 'bin\jar.exe'
        foreach ($tool in @($NodePath, $readelf, $jar)) {
            if (!(Test-Path -LiteralPath $tool -PathType Leaf)) { throw 'Native Windows Node/JDK/NDK r27 ELF verification tools unavailable' }
        }
        $nodePlatform = & $NodePath -p 'process.platform'
        if ($LASTEXITCODE -ne 0 -or $nodePlatform -cne 'win32') { throw 'Local signing requires native Windows Node.js' }
        & $NodePath (Join-Path $PSScriptRoot 'Verify-AndroidElfAlignment.mjs') `
            --apk $Apk --readelf $readelf --jar $jar
        if ($LASTEXITCODE -ne 0) { throw 'APK failed the ELF 16-KB gate; signing credentials were not decrypted' }
        $expected = (Get-FileHash -LiteralPath $certificate -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($expected -ne $approvedFingerprint -or $expected -eq $revokedFingerprint) {
            throw 'Local release certificate is not the approved fresh Android signing identity'
        }
        $bytes = [System.Security.Cryptography.ProtectedData]::Unprotect(
            [IO.File]::ReadAllBytes($protectedPassword), $null,
            [System.Security.Cryptography.DataProtectionScope]::CurrentUser)
        try { $env:LANDROP_ANDROID_SIGN_PASSWORD = [Text.Encoding]::UTF8.GetString($bytes) }
        finally { [Array]::Clear($bytes, 0, $bytes.Length) }
        $env:JAVA_HOME = $JavaHome
        $stagedOutput = Join-Path $outputDirectory ('.landrop-signing-' + [guid]::NewGuid().ToString('N') + '.apk')
        & $apksigner sign --ks $keystore --ks-key-alias $alias `
            --ks-pass env:LANDROP_ANDROID_SIGN_PASSWORD --key-pass env:LANDROP_ANDROID_SIGN_PASSWORD `
            --out $stagedOutput $Apk
        if ($LASTEXITCODE -ne 0) { throw 'APK signing failed' }
        $verification = & $apksigner verify --verbose --print-certs $stagedOutput
        if ($LASTEXITCODE -ne 0) { throw 'APK signature verification failed' }
        $digests = @($verification | Select-String '^Signer #\d+ certificate SHA-256 digest: ([0-9a-fA-F]{64})$')
        if ($digests.Count -ne 1 -or $digests[0].Matches[0].Groups[1].Value.ToLowerInvariant() -ne $expected) {
            throw 'APK signer does not match the fresh private release certificate'
        }
        & $zipalign -c -P 16 4 $stagedOutput
        if ($LASTEXITCODE -ne 0) { throw 'Signed APK failed 16-KB ZIP alignment verification' }
        Write-Output 'PASS: signed APK 16-KB ZIP alignment'
        $signedMetadata = Assert-ApkMetadata -Aapt $aapt -Path $stagedOutput -Version $sourceVersion
        if ($signedMetadata -cne $unsignedMetadata) { throw 'APK metadata changed during signing' }
        # Same-directory rename publishes only a verified artifact. File.Move's
        # two-argument overload fails if another process created OutputApk meanwhile.
        [IO.File]::Move($stagedOutput, $OutputApk)
        Write-Output $verification
        Write-Output "Signed APK: $OutputApk"
    }
    $fingerprint = (Get-FileHash -LiteralPath $certificate -Algorithm SHA256).Hash.ToLowerInvariant()
    Write-Output "Certificate SHA-256: $fingerprint"
    Write-Output "Signing vault: $vault"
} finally {
    Remove-Item Env:\LANDROP_ANDROID_SIGN_PASSWORD -ErrorAction SilentlyContinue
    [Environment]::SetEnvironmentVariable('JAVA_HOME', $originalJavaHome, 'Process')
    if ($stagedOutput) {
        Remove-Item -LiteralPath $stagedOutput -Force -ErrorAction SilentlyContinue
        Remove-Item -LiteralPath ($stagedOutput + '.idsig') -Force -ErrorAction SilentlyContinue
    }
    $password = $null
}
