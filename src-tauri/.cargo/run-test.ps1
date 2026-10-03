$ErrorActionPreference = 'Stop'

if ($args.Count -eq 0) {
    throw 'Cargo test runner expected the test executable path'
}

$testExe = (Resolve-Path -LiteralPath $args[0]).Path
$testArgs = if ($args.Count -gt 1) { $args[1..($args.Count - 1)] } else { @() }

$separator = [IO.Path]::PathSeparator
$keptPath = foreach ($entry in ($env:Path -split [regex]::Escape([string]$separator))) {
    if (-not $entry) {
        continue
    }
    if (-not (Test-Path -LiteralPath $entry -PathType Container)) {
        $entry
        continue
    }

    $apiSetDll = Get-ChildItem -LiteralPath $entry -Filter 'api-ms-win-*.dll' -File -ErrorAction SilentlyContinue |
        Select-Object -First 1
    if ($apiSetDll) {
        Write-Host "Removing PATH entry containing stale API-set DLLs: $entry"
    } else {
        $entry
    }
}
$env:Path = $keptPath -join $separator

$windowsKits = Join-Path ${env:ProgramFiles(x86)} 'Windows Kits\10\bin'
$mt = Get-ChildItem -LiteralPath $windowsKits -Directory -ErrorAction SilentlyContinue |
    Sort-Object Name -Descending |
    ForEach-Object { Join-Path $_.FullName 'x64\mt.exe' } |
    Where-Object { Test-Path -LiteralPath $_ } |
    Select-Object -First 1
if (-not $mt) {
    $mtCommand = Get-Command mt.exe -ErrorAction SilentlyContinue
    if ($mtCommand) {
        $mt = $mtCommand.Source
    }
}
if (-not $mt) {
    throw 'Could not locate mt.exe from the Windows SDK to embed the test manifest'
}

$manifest = Join-Path $PSScriptRoot 'comctl-v6.xml'
if (-not (Test-Path -LiteralPath $manifest -PathType Leaf)) {
    throw "Test manifest not found: $manifest"
}

& $mt -nologo -manifest $manifest "-outputresource:$testExe;#1" | Out-Null
if ($LASTEXITCODE -ne 0) {
    throw "mt.exe failed to embed the test manifest (exit $LASTEXITCODE)"
}

& $testExe @testArgs
exit $LASTEXITCODE
