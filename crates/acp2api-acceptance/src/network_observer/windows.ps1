param([string]$Operation, [long]$Start, [string]$Time, [string]$Rule, [string]$Program, [string]$Pids)
$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = [System.Text.UTF8Encoding]::new($false)
function Read-Events([string[]]$Filter, [int]$Cap) {
    $selectors = $Filter | ForEach-Object { "<Select Path='Security'>$([System.Security.SecurityElement]::Escape($_))</Select>" }
    $queryText = "<QueryList><Query Id='0'>$($selectors -join '')</Query></QueryList>"
    $query = [System.Diagnostics.Eventing.Reader.EventLogQuery]::new($null, [System.Diagnostics.Eventing.Reader.PathType]::LogName, $queryText)
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
        $pidTerms = foreach ($value in $Pids.Split(',')) {
            $number = [uint32]$value
            "Data[@Name='ProcessID']='$number'"
            "Data[@Name='ProcessID']='0x$('{0:x}' -f $number)'"
            "Data[@Name='ProcessID']='0x$('{0:X}' -f $number)'"
        }
        $window = "EventRecordID > $Start and EventRecordID <= $newest"
        $connections = "*[System[(EventID=5156 or EventID=5157) and $window] and EventData[$($pidTerms -join ' or ')]]"
        $gaps = "*[System[(EventID=1100 or EventID=1101 or EventID=1102 or EventID=1104 or EventID=1108 or EventID=4719) and $window]]"
        $events = @(Read-Events @($connections, $gaps) 4096 | Select-Object record,id,fields)
        @{anchor = ($anchor.Count -eq 1 -and $anchor[0].time -eq $Time); oldest = $oldest; newest = $newest; events = $events} | ConvertTo-Json -Depth 6 -Compress
    }
    default { throw 'Unknown network observer operation' }
}
