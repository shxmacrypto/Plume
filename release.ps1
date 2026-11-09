# Publishes a new version of Plume on GitHub Releases - one command.
#
# First write what's new under "## 0.6.0" in CHANGELOG.md: those lines become the release notes.
# (If the section is missing, the script adds it with a placeholder and stops so you can fill it in.)
#
#   .\release.ps1 0.6.0                      # bump, build, commit, tag, push, publish
#   .\release.ps1 0.6.0 -Notes "Fixes ..."     # custom release notes (default: the CHANGELOG section)
#   .\release.ps1 0.6.0 -DryRun              # show what would happen, change nothing
#
# The download link https://github.com/<owner>/<repo>/releases/latest/download/PlumeSetup.exe
# always points to the newest release, so the website never needs to change.
# Plume itself checks this release (at most once a day) and installs it in one click.
param(
    [Parameter(Mandatory = $true, Position = 0)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [string]$Notes = "",
    [switch]$DryRun
)
$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$tag = "v$Version"
$utf8 = New-Object System.Text.UTF8Encoding($false)
function Read-Text($p) { [IO.File]::ReadAllText((Join-Path $PSScriptRoot $p)) }
function Write-Text($p, $s) { if (-not $DryRun) { [IO.File]::WriteAllText((Join-Path $PSScriptRoot $p), $s, $utf8) } }
function Step($s) { Write-Host "`n==> $s" -ForegroundColor Cyan }

# GitHub repository: taken from the git remote, otherwise from src/main.rs (const REPO)
$repo = ([regex]::Match((Read-Text "src\main.rs"), 'const REPO: &str = "([^"]+)"')).Groups[1].Value
$isGit = Test-Path (Join-Path $PSScriptRoot ".git")
# git runs through cmd (like cargo in build.ps1): it prints progress on stderr, which
# Windows PowerShell would otherwise turn into a terminating error under "Stop".
if ($isGit) {
    $remote = cmd /c "git remote get-url origin 2>nul"
    if ("$remote" -match 'github\.com[:/]([^/]+/[^/.]+?)(\.git)?$') { $repo = $Matches[1] }
}
Write-Host "Plume $tag  ->  github.com/$repo$(if ($DryRun) { '   (dry run: nothing is changed)' })" -ForegroundColor Green

Step "1/5 Version $Version in Cargo.toml + launcher/Cargo.toml (+ REPO in src/main.rs)"
foreach ($f in "Cargo.toml", "launcher\Cargo.toml") {
    $re = [regex]'(?m)^version = "[^"]+"'
    Write-Text $f ($re.Replace((Read-Text $f), "version = `"$Version`"", 1))
}
Write-Text "src\main.rs" ([regex]::Replace((Read-Text "src\main.rs"), 'const REPO: &str = "[^"]+"', "const REPO: &str = `"$repo`""))

Step "2/5 Release notes"
$placeholder = "- (describe the changes)"
$changelog = Read-Text "CHANGELOG.md"
$section = [regex]::Match($changelog, "(?ms)^## \[?$([regex]::Escape($Version))\]?[^\n]*\n(.*?)(?=^## |\z)")
if (-not $Notes -and -not $section.Success) {
    # No section yet: add one for the user to fill in
    $date = Get-Date -Format "yyyy-MM-dd"
    Write-Text "CHANGELOG.md" (([regex]'(?m)^## ').Replace($changelog, "## $Version - $date`n`n$placeholder`n`n## ", 1))
    $Notes = $placeholder
} elseif (-not $Notes) { $Notes = $section.Groups[1].Value.Trim() }
if ($Notes.Contains($placeholder)) {
    Write-Host "Write what's new under '## $Version' in CHANGELOG.md (replace the placeholder line), then run this again." -ForegroundColor Yellow
    if (-not $DryRun) { exit 1 }
}
Write-Host $Notes

Step "3/5 Build"
if (-not $DryRun) {
    & "$PSScriptRoot\build.ps1"
    if ($LASTEXITCODE) { Write-Host "Build failed, nothing published." -ForegroundColor Red; exit 1 }
} else { Write-Host "(skipped) .\build.ps1 -> dist\PlumeSetup.exe + dist\PlumeSetup.exe.sha256" }

Step "4/5 Commit, tag, push"
if (-not $isGit) {
    Write-Host "Not a git repository yet: skipped. (git init; git remote add origin https://github.com/$repo.git)" -ForegroundColor Yellow
} elseif ($DryRun) {
    Write-Host "(skipped) git add -A; git commit -m 'Release $tag'; git tag $tag; git push -u origin HEAD; git push origin $tag"
} else {
    cmd /c "git add -A 2>&1"
    cmd /c "git commit -m `"Release $tag`" 2>&1"          # nothing to commit on a re-run: fine
    cmd /c "git tag -a $tag -m `"Plume $Version`" 2>&1"   # tag already there on a re-run: fine
    cmd /c "git push -u origin HEAD 2>&1"
    if ($LASTEXITCODE) { Write-Host "git push failed: does github.com/$repo exist, and is it 'origin' (git remote -v)? Fix it, then run this again." -ForegroundColor Red; exit 1 }
    cmd /c "git push origin $tag 2>&1"
    if ($LASTEXITCODE) { Write-Host "Pushing the tag $tag failed, nothing published." -ForegroundColor Red; exit 1 }
}

Step "5/5 Publish on GitHub Releases"
$assets = "dist\PlumeSetup.exe", "dist\PlumeSetup.exe.sha256"
if ($DryRun) {
    Write-Host "(skipped) gh release create $tag $($assets -join ' ') --title 'Plume $Version'"
} elseif (Get-Command gh -ErrorAction SilentlyContinue) {
    gh release create $tag @assets --repo $repo --title "Plume $Version" --notes $Notes
    Write-Host "Published: https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
} else {
    # Without the GitHub CLI: open the prefilled release page, the user drops the two files from dist\
    $url = "https://github.com/$repo/releases/new?tag=$tag&title=" + [uri]::EscapeDataString("Plume $Version")
    $full = $null
    try { $full = $url + "&body=" + [uri]::EscapeDataString($Notes) } catch { }   # .NET refuses very long strings
    if ($full -and $full.Length -le 7000) {
        $url = $full
    } else {
        # Notes too long for an address (GitHub refuses it): tag + title in the page, notes on the clipboard
        try {
            Set-Clipboard -Value $Notes
            Write-Host "The release notes are too long for the page address, so they are NOT in the page." -ForegroundColor Yellow
            Write-Host "They are on the clipboard: click in the description box and press Ctrl+V." -ForegroundColor Yellow
        } catch {
            $nf = Join-Path $PSScriptRoot "dist\release-notes-$Version.md"
            [IO.File]::WriteAllText($nf, $Notes, $utf8)
            Write-Host "The release notes are too long for the page address and the clipboard is not available." -ForegroundColor Yellow
            Write-Host "Copy them from $nf into the description box." -ForegroundColor Yellow
        }
    }
    Start-Process $url
    Start-Process explorer.exe (Join-Path $PSScriptRoot "dist")
    Write-Host "GitHub CLI (gh) not installed: the release page is open - drag PlumeSetup.exe and PlumeSetup.exe.sha256 from the dist folder, then click 'Publish release'." -ForegroundColor Yellow
}
