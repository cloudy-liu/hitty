param([string]$CoreBinary, [string]$Peer, [string]$Role)
$ErrorActionPreference = 'Stop'
[Console]::Write([char]27 + ']7501;state=idle:app=hitty-test' + [char]27 + '\')
[Console]::WriteLine('PEER_READY')
if ($Role -eq 'sender') {
    & $CoreBinary send $Peer 'PING_FROM_FIRST' --json
    if ($LASTEXITCODE -ne 0) { exit 1 }
}
$expected = if ($Role -eq 'sender') { 'PONG_FROM_SECOND' } else { 'PING_FROM_FIRST' }
$received = ''
while ($null -ne ($line = [Console]::ReadLine())) {
    $received += $line + "`n"
    if ($received.Contains($expected) -and $received.Contains($Peer)) {
        [Console]::WriteLine('VERIFIED_' + $expected)
        if ($Role -eq 'receiver') {
            & $CoreBinary send $Peer 'PONG_FROM_SECOND' --json
            if ($LASTEXITCODE -ne 0) { exit 1 }
        }
        exit 0
    }
}
exit 2
