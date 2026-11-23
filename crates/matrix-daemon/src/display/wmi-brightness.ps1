$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false)
$hash = [Security.Cryptography.SHA256]::Create()
try {
    $rows = @(Get-CimInstance -Namespace root/WMI -ClassName WmiMonitorBrightness |
        Where-Object { $_.Active -and $_.InstanceName } |
        ForEach-Object {
            $bytes = [Text.Encoding]::UTF8.GetBytes($_.InstanceName)
            $key = [BitConverter]::ToString($hash.ComputeHash($bytes)).Replace('-', '').Substring(0, 24)
            @{ id = "WMI-$key"; level = $_.CurrentBrightness }
        })
    ConvertTo-Json -Compress -InputObject $rows
} finally {
    $hash.Dispose()
}
