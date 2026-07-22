$script:SmokePortMinimum = 30000
$script:SmokePortMaximum = 45000
$script:AllocatedSmokePorts = [System.Collections.Generic.HashSet[int]]::new()

function Get-LoomSmokePort {
    for ($attempt = 0; $attempt -lt 64; $attempt++) {
        $port = Get-Random -Minimum $script:SmokePortMinimum -Maximum ($script:SmokePortMaximum + 1)
        $listener = [System.Net.Sockets.TcpListener]::new(
            [System.Net.IPAddress]::Parse("127.0.0.1"),
            $port
        )
        $listener.ExclusiveAddressUse = $true
        try {
            $listener.Start()
            if ($script:AllocatedSmokePorts.Add([int]$port)) {
                return [int]$port
            }
        }
        catch { }
        finally {
            $listener.Stop()
        }
    }
    throw "Unable to allocate an isolated Loom smoke port between $script:SmokePortMinimum and $script:SmokePortMaximum."
}
