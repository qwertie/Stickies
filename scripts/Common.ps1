# Dot-source this from other scripts in the repo.

# Follows directory junctions (including volume-GUID junctions such as a mount point at C:\Dev)
# to the path's real location. Vite refuses to build when the project root differs from its
# realpath, and a scheduled task should not depend on a junction being resolvable at wake time.
function Resolve-RealPath([string]$Path) {
    $resolved = ''
    foreach ($part in $Path -split '\\') {
        $resolved = if ($resolved) { Join-Path $resolved $part } else { "$part\" }
        $item = Get-Item -LiteralPath $resolved -ErrorAction SilentlyContinue
        if ($item -and $item.LinkType -eq 'Junction' -and $item.Target) {
            $target = ([string]$item.Target) -replace '^\\\\\?\\', ''
            if ($target -like 'Volume{*') {
                $volume = Get-Volume | Where-Object { ($_.UniqueId -replace '^\\\\\?\\', '') -eq $target } | Select-Object -First 1
                if ($volume.DriveLetter) { $target = "$($volume.DriveLetter):\" }
            }
            $resolved = $target.TrimEnd('\')
            if ($resolved.Length -eq 2) { $resolved += '\' }
        }
    }
    return $resolved
}
