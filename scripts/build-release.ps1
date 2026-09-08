# One-shot release build for Hive Viewer, including the Windows 11 context
# menu shell extension. Safe to re-run; the dev certificate is created once
# and reused from Cert:\CurrentUser\My.
#
#   scripts\build-release.ps1                full build (installer included)
#   scripts\build-release.ps1 -SkipTauriBuild   only the signed identity package
#
# Steps: build the COM handler DLL -> ensure self-signed cert -> pack + sign
# the sparse identity package -> pnpm tauri build.
param([switch]$SkipTauriBuild)
$ErrorActionPreference = 'Stop'

# NOTE: cert handling below uses raw .NET X509Store APIs instead of the Cert:
# PSDrive / PKI cmdlets, which are unreliable on some machines.

function Get-HiveCert {
    $x = 'System.Security.Cryptography.X509Certificates'
    $store = New-Object "$x.X509Store" -ArgumentList 'My', 'CurrentUser'
    $store.Open('ReadWrite')
    try {
        $cert = @($store.Certificates) |
            Where-Object { $_.Subject -eq 'CN=HiveViewer' -and $_.HasPrivateKey } |
            Select-Object -First 1
        if (-not $cert) {
            Write-Host 'Creating self-signed certificate CN=HiveViewer ...'
            $dn  = New-Object "$x.X500DistinguishedName" -ArgumentList 'CN=HiveViewer'
            $rsa = [System.Security.Cryptography.RSA]::Create(2048)
            $sha256 = [System.Security.Cryptography.HashAlgorithmName]::SHA256
            $pkcs1 = [System.Security.Cryptography.RSASignaturePadding]::Pkcs1
            $req = New-Object "$x.CertificateRequest" -ArgumentList $dn, $rsa, $sha256, $pkcs1
            $bc = New-Object "$x.X509BasicConstraintsExtension" -ArgumentList $false, $false, 0, $false
            $req.CertificateExtensions.Add($bc)
            $kuFlags = [System.Security.Cryptography.X509Certificates.X509KeyUsageFlags]::DigitalSignature
            $ku = New-Object "$x.X509KeyUsageExtension" -ArgumentList $kuFlags, $false
            $req.CertificateExtensions.Add($ku)
            $eku = [System.Security.Cryptography.OidCollection]::new()
            [void]$eku.Add([System.Security.Cryptography.Oid]::new('1.3.6.1.5.5.7.3.3')) # code signing
            $ekuExt = New-Object "$x.X509EnhancedKeyUsageExtension" -ArgumentList $eku, $false
            $req.CertificateExtensions.Add($ekuExt)
            $cert = $req.CreateSelfSigned([DateTimeOffset]::UtcNow.AddDays(-1), [DateTimeOffset]::UtcNow.AddYears(10))
            # Re-import via PFX so the private key is persisted in the user store.
            $pass = [Guid]::NewGuid().ToString('N')
            $pfx = $cert.Export('Pfx', $pass)
            $cert = New-Object "$x.X509Certificate2" -ArgumentList $pfx, $pass, 'Exportable,PersistKeySet,UserKeySet'
            $store.Add($cert)
        }
    }
    finally { $store.Close() }
    $cert
}

$root       = Split-Path $PSScriptRoot -Parent
$windowsDir = Join-Path $root 'src-tauri\windows'
$manifest   = Join-Path $windowsDir 'AppxManifest.xml'
$msixOut    = Join-Path $windowsDir 'HiveViewer.Identity.msix'
$cerOut     = Join-Path $windowsDir 'hive-viewer.cer'

# 1. Shell extension DLL.
Push-Location (Join-Path $root 'shell-ext')
try {
    cargo build --release
    if ($LASTEXITCODE) { throw 'cargo build failed' }
}
finally { Pop-Location }

# 2. Self-signed code-signing certificate (subject must match the manifest
#    Identity Publisher). Private key stays in the user's cert store; only the
#    public .cer is shipped and imported by the installer.
$cert = Get-HiveCert
[System.IO.File]::WriteAllBytes($cerOut, $cert.Export('Cer'))

# 3. Pack + sign the sparse identity package.
$sdkBin = Get-ChildItem 'C:\Program Files (x86)\Windows Kits\10\bin' -Directory |
    Where-Object { $_.Name -like '10.*' } |
    Sort-Object { [version]$_.Name } -Descending |
    Select-Object -First 1
if (-not $sdkBin) { throw 'Windows SDK not found (expected under C:\Program Files (x86)\Windows Kits\10\bin)' }
$makeAppx = Join-Path $sdkBin.FullName 'x64\makeappx.exe'
$signTool = Join-Path $sdkBin.FullName 'x64\signtool.exe'
if (-not (Test-Path $makeAppx)) { throw "makeappx.exe not found in $($sdkBin.FullName)" }
if (-not (Test-Path $signTool)) { throw "signtool.exe not found in $($sdkBin.FullName)" }

$stage = Join-Path $windowsDir '.stage'
Remove-Item $stage -Recurse -Force -ErrorAction Ignore
New-Item $stage -ItemType Directory | Out-Null
Copy-Item $manifest $stage
& $makeAppx pack /o /nv /d $stage /p $msixOut
if ($LASTEXITCODE) { throw 'makeappx pack failed' }
Remove-Item $stage -Recurse -Force

& $signTool sign /fd SHA256 /sha1 $cert.Thumbprint $msixOut
if ($LASTEXITCODE) { throw 'signtool sign failed' }
Write-Host "Signed identity package: $msixOut"

# 4. Tauri app + NSIS installer.
if (-not $SkipTauriBuild) {
    Push-Location $root
    try { pnpm tauri build }
    finally { Pop-Location }

    # Tauri names the installer "<productName>_<version>_x64-setup.exe" and
    # offers no override; strip spaces from the file name (productName also
    # drives the Start Menu / install dir display names, so leave it alone).
    Get-ChildItem (Join-Path $root 'src-tauri\target\release\bundle\nsis') -Filter '*-setup.exe' |
        Where-Object { $_.Name.Contains(' ') } |
        ForEach-Object {
            $dest = Join-Path $_.DirectoryName ($_.Name -replace ' ', '')
            Remove-Item $dest -Force -ErrorAction Ignore
            Rename-Item $_.FullName $dest
        }
}
