[CmdletBinding()]
param(
    [ValidateSet('set-key', 'status', 'usage', 'probe', 'serve')]
    [string]$Action = 'status',
    [ValidateSet('map', 'list', 'table')]
    [string]$Page = 'table',
    [switch]$UseBuilt
)

$ErrorActionPreference = 'Stop'
$projectRoot = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot '..'))
$keyFile = Join-Path $projectRoot '.state/context-dev-key.dpapi'
if ($UseBuilt -and $Action -ne 'probe') { throw '-UseBuilt applies only to -Action probe.' }

if ($Action -eq 'set-key') {
    if ($env:OS -ne 'Windows_NT') { throw 'Encrypted key storage requires Windows. Use a server-side CONTEXT_DEV_API_KEY environment variable on other systems.' }
    Write-Host 'Context.dev API key setup for EmberWatch'
    Write-Host 'Paste your API key below. Input is hidden and is not added to command history.'
    Write-Host 'The saved key is encrypted for your Windows account under the ignored .state directory.'
    $secureKey = $null
    $statusFile = Join-Path $projectRoot '.state/context-key-status.json'
    [IO.Directory]::CreateDirectory((Split-Path $keyFile -Parent)) | Out-Null
    [IO.File]::WriteAllText($statusFile, '{"saved":false,"stage":"awaiting_input"}')
    try {
        $secureKey = Read-Host 'Context.dev API key' -AsSecureString
        if ($secureKey.Length -eq 0) { throw 'No key was entered; nothing was saved.' }
        $encrypted = ConvertFrom-SecureString -SecureString $secureKey
        [IO.File]::WriteAllText($keyFile, $encrypted)
        [IO.File]::WriteAllText($statusFile, '{"saved":true}')
        Write-Host 'Key saved encrypted. It will only be loaded into the server/probe process.'
        Read-Host 'Saved successfully. Press Enter to close this window' | Out-Null
    } catch {
        $failure = @{ saved = $false; error_type = $_.Exception.GetType().Name } | ConvertTo-Json -Compress
        [IO.File]::WriteAllText($statusFile, $failure)
        Write-Host 'The key was not saved. No key value has been logged.' -ForegroundColor Red
        Read-Host 'Press Enter to close this window' | Out-Null
        exit 1
    } finally { if ($null -ne $secureKey) { $secureKey.Dispose() } }
    exit 0
}

if ($Action -eq 'status') {
    [pscustomobject]@{
        process_key_present = -not [string]::IsNullOrWhiteSpace($env:CONTEXT_DEV_API_KEY)
        encrypted_key_present = Test-Path -LiteralPath $keyFile
        encrypted_key_path = $keyFile
    } | ConvertTo-Json -Compress
    exit 0
}

$previousKey = [Environment]::GetEnvironmentVariable('CONTEXT_DEV_API_KEY', 'Process')
$keyPointer = [IntPtr]::Zero
$secureKey = $null
try {
    if ([string]::IsNullOrWhiteSpace($previousKey)) {
        if (!(Test-Path -LiteralPath $keyFile)) { throw 'Set CONTEXT_DEV_API_KEY server-side or run infra/context.ps1 -Action set-key in your local terminal. Do not paste the key into chat.' }
        $secureKey = ConvertTo-SecureString ([IO.File]::ReadAllText($keyFile))
        $keyPointer = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($secureKey)
        $env:CONTEXT_DEV_API_KEY = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($keyPointer).Trim()
    }
    if ($Action -eq 'usage') {
        try {
            $usage = Invoke-RestMethod -Uri 'https://api.context.dev/v1/org/usage' -Headers @{ Authorization = "Bearer $env:CONTEXT_DEV_API_KEY" } -TimeoutSec 20 -MaximumRedirection 0
        } catch { throw 'Context usage check failed. Check key permissions (logs:read), connectivity, or account status. No response body or credential has been logged.' }
        [pscustomobject]@{
            credits_remaining = $usage.credits_remaining
            credits_used = $usage.period.credits_used
            auto_topup_enabled = $usage.auto_topup.enabled
            request_id = $usage.request_id
        } | ConvertTo-Json -Compress
        exit 0
    }
    $cargoCommand = Get-Command cargo -ErrorAction SilentlyContinue
    $cargo = if ($null -ne $cargoCommand) { $cargoCommand.Source } else { Join-Path $env:USERPROFILE '.cargo/bin/cargo.exe' }
    if (!(Test-Path -LiteralPath $cargo)) { throw 'Rust Cargo was not found. Install Rust or add Cargo to PATH.' }
    Push-Location $projectRoot
    try {
        if ($Action -eq 'probe') {
            if ($UseBuilt) {
                $probePath = Join-Path $projectRoot 'target/debug/examples/probe.exe'
                if (!(Test-Path -LiteralPath $probePath)) { throw 'Build the probe first with cargo test -p emberwatch-intelligence --no-run.' }
                $probeFile = Get-Item -LiteralPath $probePath
                $inputs = @(Get-ChildItem -Path 'crates/domain', 'crates/intelligence' -Recurse -File -Include '*.rs', 'Cargo.toml')
                $inputs += Get-Item -LiteralPath 'Cargo.toml', 'Cargo.lock', 'rust-toolchain.toml'
                if ($inputs | Where-Object { $_.LastWriteTimeUtc -gt $probeFile.LastWriteTimeUtc }) { throw 'The built probe predates source/config changes. Rebuild before using -UseBuilt.' }
                & $probeFile.FullName fogos $Page
            } else {
                & $cargo +stable run --locked -p emberwatch-intelligence --example probe -- fogos $Page
            }
        } else {
            & $cargo +stable run --locked -p emberwatch-server
        }
        $commandExitCode = $LASTEXITCODE
    } finally { Pop-Location }
} finally {
    [Environment]::SetEnvironmentVariable('CONTEXT_DEV_API_KEY', $previousKey, 'Process')
    if ($keyPointer -ne [IntPtr]::Zero) { [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($keyPointer) }
    if ($null -ne $secureKey) { $secureKey.Dispose() }
}
exit $commandExitCode
