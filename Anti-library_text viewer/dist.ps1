<#
.SYNOPSIS
    Build a folder someone else can be handed.

.DESCRIPTION
    Release binaries, the licence, the notices, and the readme, in one place and
    one zip. The zip is checked afterwards by unpacking it somewhere else and
    running the terminal reader out of it, because a package that was never
    opened is not a package that works — the only thing worse than no installer
    is one that produces a folder which does not run.

    What this deliberately does not do: sign the code, or arrange updates.
    Signing needs a certificate this project does not have, and an update needs
    somewhere to fetch from. Both are named in the README as missing rather than
    faked here.

.EXAMPLE
    ./dist.ps1
    ./dist.ps1 -Out D:\ship
#>
[CmdletBinding()]
param(
    [string]$Out = '',

    # Thumbprint of a code-signing certificate in the current user's store. The
    # binaries and the installer are signed with it when given. Without one the
    # run says so, loudly, and carries on — a build that quietly skips signing
    # is how an unsigned release goes out believing it was signed.
    [string]$CertThumbprint = '',

    # Where releases are announced, written into latest.json for an update
    # check to read. Nothing is uploaded by this script.
    [string]$ReleaseUrl = '',

    [switch]$SkipInstaller,

    # Install the installer, run what it put down, uninstall it, and check that
    # it took everything of its own away and none of the reader's. Writes to
    # this profile's Start menu and uninstall list while it runs, which is why
    # it is not the default.
    [switch]$VerifyInstaller
)

$ErrorActionPreference = 'Stop'
Set-Location -LiteralPath $PSScriptRoot

$version = (Select-String -LiteralPath 'Cargo.toml' -Pattern '^version = "(.+)"' |
    Select-Object -First 1).Matches[0].Groups[1].Value
$name = "anti-library-$version-windows-x64"
if ($Out -eq '') { $Out = Join-Path $PSScriptRoot 'dist' }
$stage = Join-Path $Out $name

Write-Host "Anti-library $version" -ForegroundColor White
Write-Host ("{0:-<58}" -f "")

# The C runtime goes inside the executables. Without this they import
# VCRUNTIME140.dll and the api-ms-win-crt-* stubs, and a machine without the
# Visual C++ redistributable refuses to start them with a message that explains
# nothing. `--target` is given explicitly because RUSTFLAGS without one is also
# applied to build scripts and proc-macros, which are built for this machine and
# do not want it.
$triple = 'x86_64-pc-windows-msvc'
$built = Join-Path 'target' (Join-Path $triple 'release')

Write-Host '-> build (static CRT)' -ForegroundColor Cyan
$env:RUSTFLAGS = '-C target-feature=+crt-static'
try {
    cargo build --release --bins --target $triple
} finally {
    Remove-Item Env:\RUSTFLAGS -ErrorAction SilentlyContinue
}
if ($LASTEXITCODE -ne 0) { Write-Host '   FAILED' -ForegroundColor Red; exit 1 }

Write-Host '-> stage' -ForegroundColor Cyan
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

# The reader itself, and the two tools worth handing over with it. The quality
# gate stays behind: it is for whoever works on this, not whoever reads with it.
foreach ($exe in 'antilib-gui.exe', 'antilib.exe', 'antilib-bench.exe') {
    Copy-Item -LiteralPath (Join-Path $built $exe) -Destination $stage
}
foreach ($doc in 'LICENSE', 'NOTICES.md', 'THIRD-PARTY-LICENSES.md', 'README.md') {
    Copy-Item -LiteralPath $doc -Destination $stage
}

# ---------------------------------------------------------------------------
# Did the runtime actually go in?
#
# Read from the PE import table, not by searching the file for text: a DLL name
# can appear in a binary that only loads it at runtime, and the question here is
# what the loader demands before `main` is reached.

Add-Type @'
using System;
using System.IO;
using System.Collections.Generic;
public class PeImports {
    public static List<string> Of(string path) {
        var found = new List<string>();
        byte[] b = File.ReadAllBytes(path);
        int pe = BitConverter.ToInt32(b, 0x3C);
        int opt = pe + 24;
        ushort magic = BitConverter.ToUInt16(b, opt);
        int dirs = opt + (magic == 0x20b ? 112 : 96);
        int impRva = BitConverter.ToInt32(b, dirs + 8);
        if (impRva == 0) return found;
        ushort nsec = BitConverter.ToUInt16(b, pe + 6);
        ushort optSize = BitConverter.ToUInt16(b, pe + 20);
        int sec = pe + 24 + optSize;
        Func<int,int> at = rva => {
            for (int i = 0; i < nsec; i++) {
                int s = sec + i * 40;
                int va = BitConverter.ToInt32(b, s + 12);
                int sz = BitConverter.ToInt32(b, s + 16);
                int po = BitConverter.ToInt32(b, s + 20);
                if (rva >= va && rva < va + sz) return rva - va + po;
            }
            return -1;
        };
        int off = at(impRva);
        if (off < 0) return found;
        for (int i = 0; ; i++) {
            int e = off + i * 20;
            int nameRva = BitConverter.ToInt32(b, e + 12);
            if (nameRva == 0) break;
            int no = at(nameRva);
            if (no < 0) break;
            var sb = new System.Text.StringBuilder();
            while (b[no] != 0) sb.Append((char)b[no++]);
            found.Add(sb.ToString());
        }
        return found;
    }
}
'@

Write-Host '-> the C runtime is inside the executables' -ForegroundColor Cyan
$crtLeft = @()
foreach ($exe in (Get-ChildItem -LiteralPath $stage -Filter '*.exe')) {
    foreach ($dll in [PeImports]::Of($exe.FullName)) {
        if ($dll -match '^(vcruntime|msvcp|api-ms-win-crt)') {
            $crtLeft += ("{0} needs {1}" -f $exe.Name, $dll)
        }
    }
}
if ($crtLeft.Count -gt 0) {
    Write-Host '   FAILED: the static CRT flag did not take effect' -ForegroundColor Red
    $crtLeft | ForEach-Object { Write-Host ("     {0}" -f $_) -ForegroundColor Red }
    exit 1
}
Write-Host '   ok' -ForegroundColor DarkGray

# ---------------------------------------------------------------------------
# Signing. Optional because a certificate is not something a repository can
# hold, and never silent: a run that did not sign says which it was.

function Get-SignTool {
    $bin = 'C:\Program Files (x86)\Windows Kits\10\bin'
    if (-not (Test-Path -LiteralPath $bin)) { return $null }
    Get-ChildItem -LiteralPath $bin -Filter 'signtool.exe' -Recurse -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object FullName -Descending |
        Select-Object -First 1 -ExpandProperty FullName
}

$signed = $false
if ($CertThumbprint -ne '') {
    Write-Host '-> sign' -ForegroundColor Cyan
    $signtool = Get-SignTool
    if (-not $signtool) { Write-Host '   FAILED: no signtool.exe found' -ForegroundColor Red; exit 1 }
    $targets = Get-ChildItem -LiteralPath $stage -Filter '*.exe' | ForEach-Object { $_.FullName }
    & $signtool sign /sha1 $CertThumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 @targets
    if ($LASTEXITCODE -ne 0) { Write-Host '   FAILED: signing' -ForegroundColor Red; exit 1 }
    $signed = $true
    Write-Host '   ok' -ForegroundColor DarkGray
} else {
    Write-Host '-> sign' -ForegroundColor Cyan
    Write-Host '   SKIPPED: no -CertThumbprint given. SmartScreen will warn on first run.' -ForegroundColor Yellow
}

Write-Host '-> zip' -ForegroundColor Cyan
$zip = Join-Path $Out "$name.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
# Through .NET, not through Compress-Archive. This project lives in a folder
# called `[Anti-library]`, and those brackets are a wildcard character class to
# PowerShell's path handling — Compress-Archive reads them that way even when
# handed -LiteralPath, finds nothing, and reports that its own argument is null.
# ZipFile takes the string as the name it is.
Add-Type -AssemblyName System.IO.Compression.FileSystem
[System.IO.Compression.ZipFile]::CreateFromDirectory(
    $stage, $zip, [System.IO.Compression.CompressionLevel]::Optimal, $false)

# A package is not finished until it has been opened somewhere else and run.
Write-Host '-> unpack the zip somewhere else and run what comes out' -ForegroundColor Cyan
$proof = Join-Path ([System.IO.Path]::GetTempPath()) ("antilib-dist-" + [System.Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $proof | Out-Null
try {
    [System.IO.Compression.ZipFile]::ExtractToDirectory($zip, $proof)
    $sample = Join-Path $proof 'proof.txt'
    Set-Content -LiteralPath $sample -Value "Chapter 1`n`nA line of text to read.`n" -Encoding UTF8

    # `--recent` opens the library and prints, without needing a terminal to
    # draw into — enough to prove the binary starts and its own code runs.
    $exe = Join-Path $proof 'antilib.exe'
    & $exe --recent | Out-Null
    if ($LASTEXITCODE -ne 0) { throw "the packaged reader would not start (exit $LASTEXITCODE)" }

    # And that it can actually read a document out of the unpacked folder.
    $dump = & (Join-Path $proof 'antilib-bench.exe') --dump $sample
    if ($LASTEXITCODE -ne 0) { throw "the packaged reader could not open a file (exit $LASTEXITCODE)" }
    # Joined first. `-notmatch` against an *array* filters it rather than
    # testing it, and the result is truthy whenever any single line fails to
    # match — which is every run, and which failed this check on a package that
    # was working perfectly well.
    if (($dump -join "`n") -notmatch 'A line of text to read\.') {
        throw 'the packaged reader opened the file but did not read it'
    }

    # What is in the archive is what people download. Signing used to happen
    # after this file was built, so a run could report a signed release and
    # ship an unsigned one; the folder had signatures and the zip did not.
    foreach ($exe in (Get-ChildItem -LiteralPath $proof -Filter '*.exe')) {
        $status = (Get-AuthenticodeSignature -LiteralPath $exe.FullName).Status
        if ($signed -and $status -eq 'NotSigned') {
            throw "$($exe.Name) inside the zip is not signed, though this run signed the build"
        }
        if (-not $signed -and $status -ne 'NotSigned') {
            throw "$($exe.Name) inside the zip carries a signature this run did not make"
        }
    }
    Write-Host '   ok' -ForegroundColor DarkGray
} catch {
    Write-Host "   FAILED: $_" -ForegroundColor Red
    exit 1
} finally {
    Remove-Item -LiteralPath $proof -Recurse -Force -ErrorAction SilentlyContinue
}

# ---------------------------------------------------------------------------
# The installer. A folder of exes is not something most people can be handed.

$setup = ''
if (-not $SkipInstaller) {
    $makensis = (Get-Command makensis -ErrorAction SilentlyContinue).Source
    if (-not $makensis) {
        Write-Host '-> installer' -ForegroundColor Cyan
        Write-Host '   SKIPPED: NSIS is not installed (scoop install nsis)' -ForegroundColor Yellow
    } else {
        Write-Host '-> installer' -ForegroundColor Cyan
        $setup = Join-Path $Out "$name-setup.exe"
        if (Test-Path -LiteralPath $setup) { Remove-Item -LiteralPath $setup -Force }
        $nsi = Join-Path $PSScriptRoot 'installer\anti-library.nsi'
        & $makensis /NOCD `
            "/DVERSION=$version" `
            "/DROOT=$PSScriptRoot" `
            "/DSTAGE=$stage" `
            "/DOUTFILE=$setup" `
            $nsi | Out-Null
        if ($LASTEXITCODE -ne 0) { Write-Host '   FAILED: makensis' -ForegroundColor Red; exit 1 }
        if (-not (Test-Path -LiteralPath $setup)) {
            Write-Host '   FAILED: makensis reported success and produced nothing' -ForegroundColor Red
            exit 1
        }
        if ($signed) {
            $signtool = Get-SignTool
            & $signtool sign /sha1 $CertThumbprint /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 $setup
            if ($LASTEXITCODE -ne 0) { Write-Host '   FAILED: signing the installer' -ForegroundColor Red; exit 1 }
        }
        Write-Host '   ok' -ForegroundColor DarkGray
    }
}

# ---------------------------------------------------------------------------
# Hashes and a manifest. Without a signature these are the only way somebody
# can tell that what they downloaded is what was built.

Write-Host '-> hashes and manifest' -ForegroundColor Cyan
$artifacts = @($zip)
if ($setup -ne '' -and (Test-Path -LiteralPath $setup)) { $artifacts += $setup }

$sums = foreach ($a in $artifacts) {
    $h = (Get-FileHash -LiteralPath $a -Algorithm SHA256).Hash.ToLower()
    "{0}  {1}" -f $h, [System.IO.Path]::GetFileName($a)
}
$sumsFile = Join-Path $Out 'SHA256SUMS.txt'
Set-Content -LiteralPath $sumsFile -Value $sums -Encoding ASCII

$files = foreach ($a in $artifacts) {
    [ordered]@{
        name   = [System.IO.Path]::GetFileName($a)
        bytes  = (Get-Item -LiteralPath $a).Length
        sha256 = (Get-FileHash -LiteralPath $a -Algorithm SHA256).Hash.ToLower()
    }
}
$manifest = [ordered]@{
    version = $version
    signed  = $signed
    url     = $ReleaseUrl
    files   = $files
}
$manifestFile = Join-Path $Out 'latest.json'
$manifest | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $manifestFile -Encoding UTF8
Write-Host '   ok' -ForegroundColor DarkGray

# ---------------------------------------------------------------------------
# An installer nobody installed is an installer nobody knows is broken.

if ($VerifyInstaller -and $setup -ne '' -and (Test-Path -LiteralPath $setup)) {
    Write-Host '-> install it, run it, remove it' -ForegroundColor Cyan
    $probe = Join-Path ([System.IO.Path]::GetTempPath()) ("antilib-setup-" + [System.Guid]::NewGuid().ToString('N'))
    $marks = Join-Path $env:APPDATA 'anti-library\library.json'
    $marksBefore = Test-Path -LiteralPath $marks
    try {
        # /D must be last and unquoted; that is NSIS's rule, not a choice.
        $p = Start-Process -FilePath $setup -ArgumentList "/S /D=$probe" -PassThru -Wait
        if ($p.ExitCode -ne 0) { throw "the installer exited $($p.ExitCode)" }
        if (-not (Test-Path -LiteralPath (Join-Path $probe 'antilib.exe'))) {
            throw 'the installer reported success and put nothing down'
        }

        $out = & (Join-Path $probe 'antilib.exe') --version
        if ($LASTEXITCODE -ne 0) { throw "the installed reader would not start (exit $LASTEXITCODE)" }
        if (($out -join '') -notmatch [regex]::Escape($version)) {
            throw "the installed reader reported '$out', not $version"
        }

        $reg = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\Anti-library'
        if (-not (Test-Path $reg)) { throw 'nothing was added to the uninstall list' }

        $u = Start-Process -FilePath (Join-Path $probe 'uninstall.exe') -ArgumentList '/S' -PassThru -Wait
        if ($u.ExitCode -ne 0) { throw "the uninstaller exited $($u.ExitCode)" }
        Start-Sleep -Seconds 2

        if (Test-Path -LiteralPath $probe) { throw 'the uninstaller left its own files behind' }
        if (Test-Path $reg) { throw 'the uninstaller left its entry in the uninstall list' }
        # The one thing it must NOT take.
        if ($marksBefore -and -not (Test-Path -LiteralPath $marks)) {
            throw 'the uninstaller deleted the reader''s bookmarks and highlights'
        }
        Write-Host '   ok' -ForegroundColor DarkGray
    } catch {
        Write-Host "   FAILED: $_" -ForegroundColor Red
        exit 1
    }
}

$size = [math]::Round((Get-Item -LiteralPath $zip).Length / 1MB, 1)
Write-Host ("{0:-<58}" -f "")
Write-Host "$zip  ($size MB)" -ForegroundColor Green
if ($setup -ne '' -and (Test-Path -LiteralPath $setup)) {
    $ssize = [math]::Round((Get-Item -LiteralPath $setup).Length / 1MB, 1)
    Write-Host "$setup  ($ssize MB)" -ForegroundColor Green
}
Write-Host "$sumsFile"
Write-Host "$manifestFile"
if (-not $signed) {
    Write-Host 'NOT SIGNED. Windows will warn on first run; the hashes above are what a reader can check instead.' -ForegroundColor Yellow
}
Write-Host 'It still does not update itself: latest.json is the manifest to serve, and the reader only opens the page you set.' -ForegroundColor DarkGray
