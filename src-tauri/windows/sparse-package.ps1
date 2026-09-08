# Runs on the END USER's machine from the NSIS installer/uninstaller, or from
# the app's context-menu setting. Registers (or removes) the sparse identity
# package that powers the Windows 11 context menu entry.
#
# Must run elevated: AppX deployment only trusts a signing certificate that
# lives in LocalMachine\TrustedPeople. The same cert in CurrentUser\TrustedPeople
# (or CurrentUser\Root) still fails with 0x800B0109 / CERT_E_UNTRUSTEDROOT --
# verified on Windows 11 build 26200. The installer/uninstaller already run
# elevated; when the app calls this script it re-launches itself behind a single
# UAC prompt, and the elevated child writes any failure text to -LogFile so the
# caller can surface it.
#
# Keep this file ASCII-only: Windows PowerShell 5.1 reads BOM-less .ps1 files as
# ANSI, so non-ASCII literals break parsing on non-UTF8 code pages.
param(
    [Parameter(Mandatory)][ValidateSet('Install', 'Uninstall')][string]$Action,
    [string]$InstallDir,
    [string]$LogFile
)
$ErrorActionPreference = 'Stop'

# $PSScriptRoot is still empty when param() default values are evaluated, so fill
# these in here instead of in the param block.
if (-not $InstallDir) { $InstallDir = $PSScriptRoot }
if (-not $LogFile) { $LogFile = Join-Path $env:TEMP 'hive-sparse-package.log' }

# NOTE: cert handling uses raw .NET X509Store APIs instead of the Cert:
# PSDrive / PKI cmdlets, which are unreliable on some machines.
$x509Store = [type]'System.Security.Cryptography.X509Certificates.X509Store'
$x509Cert2 = [type]'System.Security.Cryptography.X509Certificates.X509Certificate2'
$packageName = 'HiveViewer'
$certSubject = 'CN=HiveViewer'

$isAdmin = ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole(
    [Security.Principal.WindowsBuiltInRole]::Administrator
)
if (-not $isAdmin) {
    # Relaunch elevated. -ArgumentList must be an array with the paths quoted by
    # hand: PowerShell 5.1 joins the array verbatim and does not quote elements.
    Remove-Item $LogFile -Force -ErrorAction Ignore
    $argList = @(
        '-NoProfile', '-ExecutionPolicy', 'Bypass', '-WindowStyle', 'Hidden',
        '-File', "`"$PSCommandPath`"",
        '-Action', $Action,
        '-InstallDir', "`"$InstallDir`"",
        '-LogFile', "`"$LogFile`""
    )
    try {
        $p = Start-Process powershell -Verb RunAs -Wait -PassThru -ArgumentList $argList
    }
    catch {
        Write-Output "Admin rights are required to register the context menu: $($_.Exception.Message)"
        exit 1
    }
    if (Test-Path $LogFile) { Get-Content $LogFile }
    exit $p.ExitCode
}

try {
    if ($Action -eq 'Install') {
        # Trust the self-signed cert machine-wide, then bind the package identity
        # to the install directory (ExternalLocation). Remove any previous copy
        # first so repeated toggles don't pile up duplicates.
        $store = $x509Store::new('TrustedPeople', 'LocalMachine')
        $store.Open('ReadWrite')
        try {
            @($store.Certificates) |
                Where-Object { $_.Subject -eq $certSubject } |
                ForEach-Object { $store.Remove($_) }
            $store.Add($x509Cert2::new((Join-Path $InstallDir 'hive-viewer.cer')))
        }
        finally { $store.Close() }

        Add-AppxPackage -Path (Join-Path $InstallDir 'HiveViewer.Identity.msix') `
            -ExternalLocation $InstallDir -ForceUpdateFromAnyVersion

        if (-not (Get-AppxPackage -Name $packageName -ErrorAction Ignore)) {
            throw "Add-AppxPackage reported success but the $packageName package is still missing"
        }
    }
    else {
        Get-AppxPackage -Name $packageName -ErrorAction Ignore | Remove-AppxPackage
        # LocalMachine is where the trust lives now; CurrentUser is cleaned up too
        # so installs made before the machine-wide change don't leave a stray cert.
        foreach ($scope in 'LocalMachine', 'CurrentUser') {
            $store = $x509Store::new('TrustedPeople', $scope)
            $store.Open('ReadWrite')
            try {
                @($store.Certificates) |
                    Where-Object { $_.Subject -eq $certSubject } |
                    ForEach-Object { $store.Remove($_) }
            }
            finally { $store.Close() }
        }
    }
    Write-Output "$Action done"
    exit 0
}
catch {
    ($_ | Out-String).Trim() | Set-Content -Path $LogFile -Encoding utf8 -ErrorAction Ignore
    exit 1
}
