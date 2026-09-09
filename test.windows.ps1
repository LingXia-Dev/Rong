param(
    [Alias("e")]
    [ValidateSet("quickjs", "jscore", "all")]
    [string]$Engine = "",
    [Alias("t")]
    [string]$Test = "",
    [Alias("c")]
    [switch]$Core,
    [Alias("m")]
    [switch]$Modules,
    [Alias("k")]
    [switch]$ContinueOnError,
    [Alias("h")]
    [switch]$Help
)

Set-StrictMode -Version Latest
$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot

$HostTlsBackend = if ($env:HOST_TLS_BACKEND) { $env:HOST_TLS_BACKEND } else { "tls-aws-lc" }

function Get-JscoreFeatures {
    $featureSet = "jscore,$HostTlsBackend"
    if ($env:RONG_JSC_SOURCE) {
        $featureSet = "$featureSet,rong/jscore-source"
    }
    return $featureSet
}

function Test-JscoreSourceConfigured {
    return [bool]($env:RONG_JSC_SOURCE -or $env:RONG_JSC_ROOT)
}

function Log-Info([string]$Message) {
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Log-Pass([string]$Message) {
    Write-Host "[PASS] $Message" -ForegroundColor Green
}

function Log-Fail([string]$Message) {
    Write-Host "[FAIL] $Message" -ForegroundColor Red
}

function Log-Warn([string]$Message) {
    Write-Host "[WARN] $Message" -ForegroundColor Yellow
}

function Get-CoreTests {
    if (-not (Test-Path "tests")) {
        return @()
    }
    $testsRoot = (Resolve-Path "tests").Path
    return @(
        Get-ChildItem "tests" -Recurse -File -Filter "*.rs" |
        ForEach-Object {
            $relativePath = $_.FullName.Substring($testsRoot.Length + 1)
            ($relativePath -replace "\\", "/" -replace "\.rs$", "")
        } |
        Sort-Object
    )
}

function Get-ModuleTests {
    if (-not (Test-Path "modules")) {
        return @()
    }

    $moduleDirs = @(
        Get-ChildItem "modules" -Directory |
        Where-Object {
            $_.Name -ne "" -and
            $_.Name -ne "modules" -and
            $_.Name -notmatch "^\." -and
            (Test-Path (Join-Path $_.FullName "Cargo.toml"))
        } |
        ForEach-Object { $_.Name } |
        Sort-Object -Unique
    )

    if ($moduleDirs.Count -gt 0) {
        return $moduleDirs
    }

    try {
        $metadataJson = & cargo metadata --no-deps --format-version 1 2>$null
        if ($LASTEXITCODE -ne 0 -or [string]::IsNullOrWhiteSpace($metadataJson)) {
            throw "cargo metadata failed"
        }

        $workspaceRoot = (Resolve-Path ".").Path
        $moduleRoot = Join-Path $workspaceRoot "modules"
        $metadata = $metadataJson | ConvertFrom-Json

        return @(
            $metadata.packages |
            Where-Object {
                $_.manifest_path -like "$moduleRoot\*\Cargo.toml"
            } |
            ForEach-Object { $_.name } |
            Sort-Object -Unique
        )
    } catch {
        return @()
    }
}

$script:TotalTests = 0
$script:PassedTests = 0
$script:FailedTests = 0
$script:FailFast = -not $ContinueOnError

function Print-Header {
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host "  Rong Test Runner (Windows)" -ForegroundColor Cyan
    Write-Host "================================" -ForegroundColor Cyan
}

function Print-Usage {
    Write-Host "Usage: .\test.windows.ps1 [OPTIONS]"
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  -e, -Engine ENGINE            Run tests for specific engine (quickjs, jscore, all)"
    Write-Host "  -t, -Test TEST                Run specific test (core test name or module name)"
    Write-Host "  -c, -Core                     Run only core tests"
    Write-Host "  -m, -Modules                  Run only module tests"
    Write-Host "  -k, -ContinueOnError          Continue running tests after failure (default: stop on error)"
    Write-Host "  -h, -Help                     Show this help message"
    Write-Host ""
    Write-Host "Examples:"
    Write-Host "  .\test.windows.ps1"
    Write-Host "  .\test.windows.ps1 -k"
    Write-Host "  .\test.windows.ps1 -e quickjs"
    Write-Host "  .\test.windows.ps1 -t iterator"
    Write-Host "  .\test.windows.ps1 -t rong_http"
    Write-Host "  .\test.windows.ps1 -k -m"
}

function Print-Summary {
    Write-Host ""
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host "  Test Summary" -ForegroundColor Cyan
    Write-Host "================================" -ForegroundColor Cyan
    Write-Host "Total tests: $script:TotalTests"
    Write-Host "Passed: $script:PassedTests" -ForegroundColor Green
    Write-Host "Failed: $script:FailedTests" -ForegroundColor Red

    if ($script:FailedTests -eq 0) {
        Write-Host ""
        Write-Host "All tests passed!" -ForegroundColor Green
        exit 0
    } else {
        Write-Host ""
        Write-Host "Some tests failed!" -ForegroundColor Red
        exit 1
    }
}

function Stop-IfFailFast {
    if ($script:FailFast) {
        Log-Fail "Stopping due to fail-fast mode (default); use -k/-ContinueOnError to keep going"
        Print-Summary
    }
}

function Run-CoreTest([string]$TestName, [string]$EngineName) {
    Log-Info "Running core test: $TestName (engine: $EngineName)"
    $script:TotalTests++

    $featureSet = "$EngineName,$HostTlsBackend"
    if ($EngineName -eq "jscore") {
        $featureSet = Get-JscoreFeatures
    }
    & cargo test "--test=$TestName" "--no-default-features" "--features=$featureSet" "--quiet"
    if ($LASTEXITCODE -eq 0) {
        Log-Pass "Core test $TestName passed on $EngineName"
        $script:PassedTests++
        return
    }

    Log-Fail "Core test $TestName failed on $EngineName"
    $script:FailedTests++
    Stop-IfFailFast
}

function Run-ModuleTest([string]$ModuleName, [string]$EngineName) {
    Log-Info "Running module test: $ModuleName (engine: $EngineName)"
    $script:TotalTests++

    $featureSet = "$EngineName,$HostTlsBackend"
    if ($EngineName -eq "jscore") {
        $featureSet = Get-JscoreFeatures
    }

    $testArgs = @("-p", $ModuleName, "--no-default-features", "--features=$featureSet", "--quiet")
    if ($ModuleName -eq "rong_timer") {
        # Timer tests share the global host executor and are sensitive to crate-level test overlap.
        $testArgs += @("--", "--test-threads=1")
    }

    & cargo test @testArgs
    if ($LASTEXITCODE -eq 0) {
        Log-Pass "Module test $ModuleName passed on $EngineName"
        $script:PassedTests++
        return
    }

    Log-Fail "Module test $ModuleName failed on $EngineName"
    $script:FailedTests++
    Stop-IfFailFast
}

function Get-FeatureSet([string]$EngineName) {
    if ($EngineName -eq "jscore") {
        return Get-JscoreFeatures
    }
    return "$EngineName,$HostTlsBackend"
}

function Record-SuiteResult([string]$Name, [int]$ExitCode) {
    $script:TotalTests++
    if ($ExitCode -eq 0) {
        Log-Pass $Name
        $script:PassedTests++
        return
    }
    Log-Fail $Name
    $script:FailedTests++
    Stop-IfFailFast
}

function Run-AllCoreTests([string]$EngineName, [string[]]$CoreTests) {
    Write-Host ""
    Write-Host "Running core tests on $EngineName..." -ForegroundColor Yellow
    Log-Info "Running rong integration tests as one cargo test ($EngineName)"

    $featureSet = Get-FeatureSet $EngineName
    $testArgs = @("-p", "rong", "--no-default-features", "--features=$featureSet", "--tests", "--quiet")
    if (-not $script:FailFast) {
        $testArgs += "--no-fail-fast"
    }
    & cargo test @testArgs
    Record-SuiteResult -Name "Core integration tests on $EngineName" -ExitCode $LASTEXITCODE
}

function Run-AllModuleTests([string]$EngineName, [string[]]$ModuleTests) {
    Write-Host ""
    Write-Host "Running module tests on $EngineName..." -ForegroundColor Yellow

    $featureSet = Get-FeatureSet $EngineName
    $packageArgs = @()
    $hasTimer = $false
    foreach ($moduleName in $ModuleTests) {
        if ($moduleName -eq "rong_timer") {
            $hasTimer = $true
        } else {
            $packageArgs += @("-p", $moduleName)
        }
    }
    if ($packageArgs.Count -gt 0) {
        Log-Info "Running module lib tests as one cargo test ($EngineName)"
        $testArgs = $packageArgs + @(
            "--no-default-features"
            "--features=$featureSet"
            "--quiet"
        )
        if (-not $script:FailFast) {
            $testArgs += "--no-fail-fast"
        }
        & cargo test @testArgs
        Record-SuiteResult -Name "Module tests on $EngineName" -ExitCode $LASTEXITCODE
    }

    if ($hasTimer) {
        Log-Info "Running rong_timer tests single-threaded ($EngineName)"
        $timerArgs = @("-p", "rong_timer", "--no-default-features", "--features=$featureSet", "--quiet")
        if (-not $script:FailFast) {
            $timerArgs += "--no-fail-fast"
        }
        $timerArgs += @("--", "--test-threads=1")
        & cargo test @timerArgs
        Record-SuiteResult -Name "rong_timer tests on $EngineName" -ExitCode $LASTEXITCODE
    }
}

function Run-SpecificTest([string]$TestName, [string]$EngineName, [string[]]$CoreTests, [string[]]$ModuleTests) {
    if ($CoreTests -contains $TestName) {
        Run-CoreTest -TestName $TestName -EngineName $EngineName
        return
    }

    if ($ModuleTests -contains $TestName) {
        Run-ModuleTest -ModuleName $TestName -EngineName $EngineName
        return
    }

    Log-Fail "Unknown test: $TestName"
    $script:FailedTests++
    Stop-IfFailFast
}

if ($Help) {
    Print-Usage
    exit 0
}

$engines = @("quickjs")
if ($Engine -eq "jscore") {
    if (-not (Test-JscoreSourceConfigured)) {
        Log-Fail "jscore tests on Windows require RONG_JSC_SOURCE=1 or RONG_JSC_ROOT"
        exit 1
    }
    $engines = @("jscore")
}
if ($Engine -eq "all") {
    if (Test-JscoreSourceConfigured) {
        $engines = @("quickjs", "jscore")
    }
}
if ($Engine -eq "quickjs") {
    $engines = @("quickjs")
}

Print-Header
Log-Info "Discovering test targets..."

$coreTests = @()
$moduleTests = @()

if (-not $Modules) {
    $coreTests = Get-CoreTests
}

if (-not $Core) {
    $moduleTests = Get-ModuleTests
}

Log-Info ("Discovered {0} core tests and {1} module tests" -f $coreTests.Count, $moduleTests.Count)

foreach ($engineName in $engines) {
    Write-Host ""
    Write-Host "Testing with engine: $engineName" -ForegroundColor Yellow

    if (-not [string]::IsNullOrWhiteSpace($Test)) {
        Run-SpecificTest -TestName $Test -EngineName $engineName -CoreTests $coreTests -ModuleTests $moduleTests
    } elseif ($Core) {
        Run-AllCoreTests -EngineName $engineName -CoreTests $coreTests
    } elseif ($Modules) {
        Run-AllModuleTests -EngineName $engineName -ModuleTests $moduleTests
    } else {
        Run-AllCoreTests -EngineName $engineName -CoreTests $coreTests
        Run-AllModuleTests -EngineName $engineName -ModuleTests $moduleTests
    }
}

Print-Summary
