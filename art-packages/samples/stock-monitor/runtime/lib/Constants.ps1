# Owns immutable runtime limits and process-local Stock Monitor state.

$script:SurfaceAction = $null
$script:SurfaceActionBudgets = $null
$script:AllowedIntervals = @(1, 3, 5, 15, 30, 60, 120, 300)
$script:AllowedPeriods = @("minute", "five-day", "day", "week", "month", "quarter", "year", "minute-120", "minute-60", "minute-30", "minute-15", "minute-5", "minute-1")
$script:MaxHistoryRows = 2000
$script:MaxOrderBookLevels = 10
$script:MaxRequestBytes = 4 * 1024 * 1024
$script:MaxRequestNodes = 100000
$script:MaxJsonDepth = 32
$script:MaxLiveAgeSeconds = 90
$script:MaxOrderBookAgeSeconds = 120
$script:ProviderVersion = "2.9.0"
$script:UpstreamVersion = "2.7.3"
$script:Disclaimer = "行情可能延迟，仅用于信息展示，不构成投资建议或交易指令"
