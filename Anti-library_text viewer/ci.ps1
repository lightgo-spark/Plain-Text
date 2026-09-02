<#
.SYNOPSIS
    Every gate the reader has to pass, in one command.

.DESCRIPTION
    Build, lint, test, the quality gate, the mutation check, and the notices
    file. Each gate prints its own line and the run stops at the first failure,
    so a red build says which gate went red and not merely that something did.

    The mutation check is the one that keeps the rest honest: it puts each
    defect back and requires the test written for it to go red. Without it a
    green suite says only that the tests ran.

.PARAMETER Quick
    Skip the mutation check, which rebuilds the library once per mutation.

.EXAMPLE
    ./ci.ps1
    ./ci.ps1 -Quick
#>
[CmdletBinding()]
param(
    [switch]$Quick
)

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

$script:Passed = 0
$script:Started = Get-Date

function Invoke-Gate {
    param(
        [Parameter(Mandatory)][string]$Name,
        [Parameter(Mandatory)][scriptblock]$Body
    )
    Write-Host ("-> {0}" -f $Name) -ForegroundColor Cyan
    $at = Get-Date
    & $Body
    if ($LASTEXITCODE -ne 0) {
        Write-Host ("   FAILED: {0} (exit {1})" -f $Name, $LASTEXITCODE) -ForegroundColor Red
        exit 1
    }
    $took = [int]((Get-Date) - $at).TotalSeconds
    $script:Passed++
    Write-Host ("   ok ({0}s)" -f $took) -ForegroundColor DarkGray
}

Write-Host "Anti-library" -ForegroundColor White
Write-Host ("{0:-<58}" -f "")

# First, before anything is built on top of it. The mutation check restores
# what it touches in a `finally`, which does not run when the process is
# killed — and a tree left in that state builds, tests and passes the gate
# while quietly carrying a defect somebody put back on purpose.
Invoke-Gate "no mutation left in the tree" { python tools/check_mutations.py }

Invoke-Gate "build (debug)" { cargo build --all-targets }
Invoke-Gate "build (release)" { cargo build --release --all-targets }

# Warnings are defects that have not been read yet.
Invoke-Gate "clippy, no warnings allowed" {
    cargo clippy --all-targets --all-features -- -D warnings
}

Invoke-Gate "unit and integration tests" { cargo test --release }

# The gate proper: a few thousand checks over the matrix, each answer held
# against one derived a different way.
Invoke-Gate "quality gate" { cargo run --release --quiet --bin antilib-qa }

if (-not $Quick) {
    # The output goes to a file rather than down the pipeline. Thirty-seven
    # mutations, each rebuilding and running a test, produce thousands of lines,
    # and pushing those through a caller's pipeline has killed this gate three
    # times with `exit -1` and no message — while the same command run on its
    # own passed 37 of 37. What matters here is the exit code and the summary.
    Invoke-Gate "mutation check" {
        $log = Join-Path ([System.IO.Path]::GetTempPath()) 'antilib-mutations.log'
        python -u tools/mutate.py *> $log
        $code = $LASTEXITCODE
        Get-Content -LiteralPath $log -Tail 2 | ForEach-Object { Write-Host ("   {0}" -f $_) }
        Write-Host ("   full log: {0}" -f $log) -ForegroundColor DarkGray
        $global:LASTEXITCODE = $code
    }
}

# A notices file that has drifted states in writing that you shipped something
# you did not. Skipped, loudly, where cargo-license is not installed.
if (Get-Command cargo-license -ErrorAction SilentlyContinue) {
    Invoke-Gate "notices match the build" { python tools/notices.py --check }
    # The notices say which licence; this says what the licence actually is.
    # Several of them require their text to travel with the binaries, and a
    # file gathered from the registry goes stale the moment a crate moves.
    Invoke-Gate "third-party licence texts match the build" {
        python tools/licenses.py --check
    }
    # The document says every licence it names has its text somewhere inside.
    # That is a claim, and it was false: nothing in this build shipped a copy
    # of CC0-1.0, so the one crate offered under it went out with a sentence
    # saying its terms were enclosed when they were not.
    Invoke-Gate "every licence named has its text enclosed" {
        python tools/check_licence_cover.py
    }
} else {
    Write-Host "-> notices match the build" -ForegroundColor Cyan
    Write-Host "   SKIPPED: cargo-license is not installed (cargo install cargo-license)" -ForegroundColor Yellow
}

# A package nobody built is a package nobody knows is broken.
Invoke-Gate "the package builds and runs when unpacked" {
    & (Join-Path $PSScriptRoot 'dist.ps1') | Out-Null
    if ($LASTEXITCODE -eq $null) { $global:LASTEXITCODE = 0 }
}

$total = [int]((Get-Date) - $script:Started).TotalSeconds
Write-Host ("{0:-<58}" -f "")
Write-Host ("{0} gates passed in {1}s" -f $script:Passed, $total) -ForegroundColor Green
