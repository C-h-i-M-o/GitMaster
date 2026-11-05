$ErrorActionPreference = 'Stop'
# 只读取 Git 版本，通过 Trace2 区分程序初始化、Git 主逻辑与进程退出耗时。
foreach ($sample in 0..5) {
    foreach ($candidate in @('cmd', 'mingw64/bin')) {
        $start = [System.Diagnostics.ProcessStartInfo]::new()
        $start.FileName = Join-Path $env:ProgramFiles "Git/$candidate/git.exe"
        $start.ArgumentList.Add('--version')
        $start.UseShellExecute = $false
        $start.CreateNoWindow = $true
        $start.RedirectStandardOutput = $true
        $start.RedirectStandardError = $true
        $start.Environment['GIT_TRACE2_EVENT'] = '1'
        $process = [System.Diagnostics.Process]::new()
        $process.StartInfo = $start
        $clock = [System.Diagnostics.Stopwatch]::StartNew()
        [void]$process.Start()
        $outcome = $process.StandardOutput.ReadToEndAsync()
        $trace = $process.StandardError.ReadToEndAsync()
        if (-not $process.WaitForExit(30000)) {
            $process.Kill($true)
            $process.WaitForExit()
            throw '只读版本探针未能在诊断期限内结束'
        }
        $clock.Stop()
        $lines = @($trace.Result -split "`n" | Where-Object { $_.StartsWith('{') })
        $events = @($lines | ForEach-Object { $_ | ConvertFrom-Json })
        $first = $events | Select-Object -First 1
        $last = $events | Select-Object -Last 1
        [pscustomobject]@{
            candidate = $candidate
            sample = $sample
            wallMs = $clock.ElapsedMilliseconds
            cpuMs = $process.TotalProcessorTime.TotalMilliseconds
            firstEventDelayMs = if ($first) { ([DateTimeOffset]::Parse([regex]::Match($lines[0], '"time":"([^"]+)"').Groups[1].Value).UtcDateTime - $process.StartTime.ToUniversalTime()).TotalMilliseconds } else { $null }
            gitMainMs = if ($last.t_abs) { $last.t_abs * 1000 } else { $null }
            exit = $process.ExitCode
        } | ConvertTo-Json -Compress
        $process.Dispose()
    }
}
