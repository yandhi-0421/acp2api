param([string]$Operation, [long]$Start, [string]$Time, [string]$Rule, [string]$Program)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
function Read-Events([string]$Filter, [int]$Cap) {
    $query = [System.Diagnostics.Eventing.Reader.EventLogQuery]::new('Security', [System.Diagnostics.Eventing.Reader.PathType]::LogName, $Filter)
    $reader = [System.Diagnostics.Eventing.Reader.EventLogReader]::new($query)
    try {
        $count = 0
        while ($null -ne ($event = $reader.ReadEvent())) {
            try {
                if (++$count -gt $Cap) { throw 'Security log record cap exceeded' }
                $xml = [xml]$event.ToXml()
                $fields = @{}
                foreach ($data in $xml.Event.EventData.Data) { $fields[[string]$data.Name] = [string]$data.'#text' }
                [pscustomobject]@{record = $event.RecordId; id = $event.Id; fields = $fields; time = $event.TimeCreated.ToUniversalTime().ToString('o')}
            } finally { $event.Dispose() }
        }
    } finally { $reader.Dispose() }
}
switch ($Operation) {
    'BlockControl' {
        New-NetFirewallRule -Name $Rule -DisplayName $Rule -Direction Outbound -Action Block -Program $Program -RemoteAddress 192.0.2.1 | Out-Null
    }
    'UnblockControl' {
        try { $existing = Get-NetFirewallRule -Name $Rule }
        catch {
            if ($_.FullyQualifiedErrorId -notlike 'CmdletizationQuery_NotFound_Name,*') { throw }
            $existing = $null
        }
        if ($null -ne $existing) { $existing | Remove-NetFirewallRule }
    }
    'Cursor' {
        $last = Get-WinEvent -LogName Security -MaxEvents 1
        @{record = $last.RecordId; time = $last.TimeCreated.ToUniversalTime().ToString('o')} | ConvertTo-Json -Compress
    }
    'Read' {
        $oldest = (Get-WinEvent -LogName Security -Oldest -MaxEvents 1).RecordId
        $newest = (Get-WinEvent -LogName Security -MaxEvents 1).RecordId
        $anchor = @(Read-Events "*[System[EventRecordID=$Start]]" 1)
        $filter = "*[System[(EventID=5156 or EventID=5157 or EventID=1100 or EventID=1101 or EventID=1102 or EventID=1104 or EventID=1108 or EventID=4719) and EventRecordID > $Start and EventRecordID <= $newest]]"
        $events = @(Read-Events $filter 4096 | Select-Object record,id,fields)
        @{anchor = ($anchor.Count -eq 1 -and $anchor[0].time -eq $Time); oldest = $oldest; newest = $newest; events = $events} | ConvertTo-Json -Depth 6 -Compress
    }
    default { throw 'Unknown network observer operation' }
}
