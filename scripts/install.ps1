$PreviousErrorActionPreference = $ErrorActionPreference
$ErrorActionPreference = "Stop"
$PreviousTaliDataDir = $env:TALI_DATA_DIR

$Repo = if ($env:TALI_REPO) { $env:TALI_REPO } else { "hjun1052/tali" }
$Version = if ($env:TALI_VERSION) { $env:TALI_VERSION } else { "latest" }
$InstallDir = if ($env:TALI_INSTALL_DIR) { $env:TALI_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "Programs\tali\bin" }
$InstallSkill = if ($env:TALI_INSTALL_SKILL) { $env:TALI_INSTALL_SKILL } else { "1" }

function Add-SkillDir {
    param(
        [System.Collections.Generic.List[string]] $Dirs,
        [string] $Path
    )
    if ([string]::IsNullOrWhiteSpace($Path)) {
        return
    }
    if (-not $Dirs.Contains($Path)) {
        $Dirs.Add($Path)
    }
}

function Get-AgentSkillDirs {
    $Dirs = [System.Collections.Generic.List[string]]::new()

    if ($env:TALI_SKILL_DIRS) {
        foreach ($Path in ($env:TALI_SKILL_DIRS -split ";")) {
            Add-SkillDir $Dirs $Path
        }
        return $Dirs
    }

    if ($env:CODEX_HOME) {
        Add-SkillDir $Dirs (Join-Path $env:CODEX_HOME "skills")
    } elseif ((Test-Path (Join-Path $HOME ".codex")) -or (Get-Command codex -ErrorAction SilentlyContinue)) {
        Add-SkillDir $Dirs (Join-Path $HOME ".codex\skills")
    }

    if (Test-Path (Join-Path $HOME ".agents\skills")) {
        Add-SkillDir $Dirs (Join-Path $HOME ".agents\skills")
    }

    if ($env:CLAUDE_CONFIG_DIR) {
        Add-SkillDir $Dirs (Join-Path $env:CLAUDE_CONFIG_DIR "skills")
    } elseif ((Test-Path (Join-Path $HOME ".claude")) -or (Get-Command claude -ErrorAction SilentlyContinue)) {
        Add-SkillDir $Dirs (Join-Path $HOME ".claude\skills")
    }

    return $Dirs
}

function Install-AgentSkill {
    param([string] $ExtractDir)

    if (($InstallSkill -eq "0") -or ($InstallSkill -eq "false")) {
        Write-Host "Skipping Tali agent skill installation."
        return
    }

    $SkillSource = Get-ChildItem -Path $ExtractDir -Directory -Recurse |
        Where-Object { $_.Name -eq "tali-agent" -and $_.Parent.Name -eq "skills" } |
        Select-Object -First 1
    if (-not $SkillSource) {
        Write-Warning "Release archive did not contain the tali-agent skill."
        return
    }

    $SkillDirs = Get-AgentSkillDirs
    if ($SkillDirs.Count -eq 0) {
        Write-Host "No supported agent skill directory detected."
        Write-Host "Set TALI_SKILL_DIRS=C:\Path\To\skills to install the tali-agent skill manually."
        return
    }

    foreach ($SkillDir in $SkillDirs) {
        New-Item -ItemType Directory -Force $SkillDir | Out-Null
        $Destination = Join-Path $SkillDir "tali-agent"
        if (Test-Path $Destination) {
            $Backup = "$Destination.bak-$(Get-Date -Format yyyyMMddHHmmss)"
            Move-Item $Destination $Backup
            Write-Host "Backed up existing tali-agent skill to $Backup"
        }
        Copy-Item $SkillSource.FullName $Destination -Recurse
        Write-Host "Installed tali-agent skill to $Destination"
    }
}

if (-not [Environment]::Is64BitOperatingSystem) {
    throw "tali installer: only Windows x86_64 release archives are available"
}

$Archive = "tali-windows-x86_64.zip"
if ($Version -eq "latest") {
    $BaseUrl = "https://github.com/$Repo/releases/latest/download"
} else {
    if ($Version.StartsWith("v")) {
        $Tag = $Version
    } else {
        $Tag = "v$Version"
    }
    $BaseUrl = "https://github.com/$Repo/releases/download/$Tag"
}
if ($env:TALI_BASE_URL) {
    $BaseUrl = $env:TALI_BASE_URL
}

$TempDir = Join-Path ([System.IO.Path]::GetTempPath()) ("tali-install-" + [System.Guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force $TempDir | Out-Null

try {
    $ArchivePath = Join-Path $TempDir $Archive
    $ChecksumPath = Join-Path $TempDir "$Archive.sha256"

    Write-Host "Downloading $Archive from $Repo..."
    Invoke-WebRequest -Uri "$BaseUrl/$Archive" -OutFile $ArchivePath
    Invoke-WebRequest -Uri "$BaseUrl/$Archive.sha256" -OutFile $ChecksumPath

    $Expected = (Get-Content $ChecksumPath).Split(" ")[0].Trim().ToLowerInvariant()
    $Actual = (Get-FileHash $ArchivePath -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Expected -ne $Actual) {
        throw "tali installer: checksum mismatch"
    }

    $ExtractDir = Join-Path $TempDir "extract"
    Expand-Archive -Path $ArchivePath -DestinationPath $ExtractDir -Force
    $Binary = Get-ChildItem -Path $ExtractDir -Filter "tali.exe" -Recurse | Select-Object -First 1
    if (-not $Binary) {
        throw "tali installer: archive did not contain tali.exe"
    }

    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    $Destination = Join-Path $InstallDir "tali.exe"
    Copy-Item $Binary.FullName $Destination -Force

    Write-Host "Installed tali to $Destination"
    & $Destination --version
    $env:TALI_DATA_DIR = Join-Path $TempDir "tali-self-test"
    & $Destination self-test | Out-Null
    Write-Host "tali self-test passed."
    Install-AgentSkill $ExtractDir

    $PathEntries = ($env:PATH -split ";") | Where-Object { $_ }
    if ($PathEntries -notcontains $InstallDir) {
        Write-Warning "$InstallDir is not on PATH. Add it to PATH or run $Destination directly."
    }
} finally {
    if ($null -eq $PreviousTaliDataDir) {
        Remove-Item Env:TALI_DATA_DIR -ErrorAction SilentlyContinue
    } else {
        $env:TALI_DATA_DIR = $PreviousTaliDataDir
    }
    $ErrorActionPreference = $PreviousErrorActionPreference
    Remove-Item -Recurse -Force $TempDir -ErrorAction SilentlyContinue
}
