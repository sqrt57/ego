# Pre-commit hook: blocks temp files (_-prefixed) and binary assets from being committed.
# Invoked via git-hooks/pre-commit. Activated by core.hooksPath = git-hooks (set in .git/config).

$ErrorActionPreference = 'SilentlyContinue'

$repoRoot = (git rev-parse --show-toplevel).Trim()
if (-not $repoRoot) { exit 0 }

$staged = git diff --cached --name-only

# Block temp files (_-prefixed basenames) from being committed.
$stagedTempFiles = $staged | Where-Object { $_ -match '(^|/)_' -and $_ -notmatch '^incoming/' }
if ($stagedTempFiles) {
    Write-Host 'Temporary files must not be committed:'
    $stagedTempFiles | ForEach-Object { Write-Host "  $_" }
    Write-Host '  Fix: git restore --staged <file>'
    exit 1
}

# Block binary assets (screenshots, archives, etc.) from being committed.
$binaryExtensions = @('.png', '.jpg', '.jpeg', '.gif', '.webp', '.bmp', '.ico', '.zip', '.7z', '.tar', '.gz', '.rar', '.pdf')
$stagedBinaryFiles = $staged | Where-Object {
    $ext = [System.IO.Path]::GetExtension($_).ToLower()
    $binaryExtensions -contains $ext
}
if ($stagedBinaryFiles) {
    Write-Host 'Binary files must not be committed (see conventions.md#binary-files):'
    $stagedBinaryFiles | ForEach-Object { Write-Host "  $_" }
    Write-Host '  Fix: git restore --staged <file>'
    exit 1
}

exit 0
