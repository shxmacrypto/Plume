# Posts the release announcement in the Plume Discord server (#announcements), through its webhook:
# the English notes (CHANGELOG.md) first, then the French ones (CHANGELOG.fr.md) in a second embed.
# Run it once the GitHub release is published:
#
#   .\tools\discord-release.ps1 1.1.1            # post
#   .\tools\discord-release.ps1 1.1.1 -DryRun    # print the messages, post nothing
#
# The webhook address is a secret: it lives in %USERPROFILE%\.plume\discord-releases-webhook.txt,
# never in this repository. Always the same webhook ("Plume Releases").
param(
    [Parameter(Mandatory = $true, Position = 0)][ValidatePattern('^\d+\.\d+\.\d+$')][string]$Version,
    [switch]$DryRun
)
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$repo = "shxmacrypto/Plume"
$site = "https://plumeweb.netlify.app"
$release = "https://github.com/$repo/releases/tag/v$Version"

function Notes($file) {
    $path = Join-Path $root $file
    if (-not (Test-Path $path)) { return $null }
    $log = [IO.File]::ReadAllText($path, [Text.Encoding]::UTF8)
    $m = [regex]::Match($log, "(?ms)^## \[?$([regex]::Escape($Version))\]?[^\n]*\n(.*?)(?=^## |\z)")
    if (-not $m.Success) { return $null }
    $n = $m.Groups[1].Value.Trim() -replace '\r', ''
    # Discord markdown: "### Title" -> bold line; tidy blank lines
    $n = [regex]::Replace($n, '(?m)^#{2,4}\s*(.+)$', '**$1**')
    return [regex]::Replace($n, '\n{3,}', "`n`n")
}

function Embed($notes, $title, $links, $foot) {
    # Embed description limit is 4096 characters
    $max = 4096 - $links.Length - 8
    if ($notes.Length -gt $max) {
        $cut = $notes.LastIndexOf("`n", $max)
        if ($cut -lt 1) { $cut = $max }
        $notes = $notes.Substring(0, $cut).TrimEnd() + "`n..."
    }
    return @{ title = $title; url = $release; color = 0x0A84FF; description = "$notes`n`n$links"; footer = @{ text = $foot } }
}

$en = Notes "CHANGELOG.md"
if (-not $en) { Write-Host "No '## $Version' section in CHANGELOG.md." -ForegroundColor Red; exit 1 }
$messages = @(Embed $en "Plume Suite $Version is out" "[Download]($site)  |  [Full notes]($release)" `
        "Already installed? Plume Suite offers the update: one click.")

$fr = Notes "CHANGELOG.fr.md"
if ($fr) {
    # French text built from char codes: .ps1 files stay ASCII
    $e = [char]0xE9; $a = [char]0xE0
    $messages += Embed $fr "Plume Suite $Version est disponible (fran$([char]0xE7)ais)" `
        "[T$($e)l$($e)charger]($site)  |  [Toutes les notes]($release)" `
        "D$($e)j$a install$($e) ? Plume Suite propose la mise $a jour : un clic."
} else {
    Write-Host "CHANGELOG.fr.md has no '## $Version' section: English only." -ForegroundColor Yellow
}

$hook = $null
if (-not $DryRun) {
    $hookFile = Join-Path $env:USERPROFILE ".plume\discord-releases-webhook.txt"
    if (-not (Test-Path $hookFile)) { Write-Host "Missing $hookFile (the #announcements webhook address)." -ForegroundColor Red; exit 1 }
    $hook = ([IO.File]::ReadAllText($hookFile)).Trim()
}
foreach ($embed in $messages) {
    # One message per language: English first, then French
    $body = @{ username = "Plume Releases"; avatar_url = "$site/assets/plume-suite-round.png"; embeds = @($embed) } |
        ConvertTo-Json -Depth 6
    if ($DryRun) { Write-Host $body; continue }
    Invoke-RestMethod -Uri $hook -Method Post -ContentType "application/json; charset=utf-8" `
        -Body ([Text.Encoding]::UTF8.GetBytes($body)) | Out-Null
}
if (-not $DryRun) { Write-Host "Posted in #announcements: Plume Suite $Version ($($messages.Count) messages)" -ForegroundColor Green }
