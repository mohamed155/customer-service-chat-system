$ErrorActionPreference = 'Stop'
$envFile = Join-Path $PSScriptRoot '.env'
$envVars = @{}
Get-Content $envFile | ForEach-Object {
    $line = $_.Trim()
    if ($line -and -not $line.StartsWith('#') -and $line -match '^([A-Z_][A-Z0-9_]*)=(.*)$') {
        $envVars[$Matches[1]] = $Matches[2]
    }
}
$wrapperPath = Join-Path $PSScriptRoot '_run-server.cmd'
$lines = New-Object System.Collections.Generic.List[string]
foreach ($k in $envVars.Keys) { $lines.Add("set `"$k=$($envVars[$k])`"") }
$lines.Add("`"$PSScriptRoot\target\release\server.exe`"")
Set-Content -Path $wrapperPath -Value $lines
$proc = Start-Process -FilePath 'cmd.exe' `
    -ArgumentList '/c', $wrapperPath `
    -RedirectStandardOutput (Join-Path $PSScriptRoot 'server.log') `
    -RedirectStandardError (Join-Path $PSScriptRoot 'server.err.log') `
    -PassThru -NoNewWindow `
    -WorkingDirectory $PSScriptRoot
Start-Sleep -Milliseconds 500
if ($proc.HasExited) {
    Write-Host "Process exited immediately with code $($proc.ExitCode)"
    Get-Content (Join-Path $PSScriptRoot 'server.err.log') -Tail 20
    exit 1
}
Write-Host "Started PID $($proc.Id)"
