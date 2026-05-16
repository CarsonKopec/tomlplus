<#
.SYNOPSIS
    Download a Gradle distribution into .gradle-local/ and print its path.

.DESCRIPTION
    Allows the release script to use Gradle on a machine where it isn't
    installed. The distribution is cached so subsequent calls are instant.

.PARAMETER Version
    Gradle version. Defaults to 8.10.2 (latest stable as of writing).
#>
param([string] $Version = '8.10.2')

$ErrorActionPreference = 'Stop'

$root      = $PSScriptRoot
$cacheDir  = Join-Path $root '.gradle-local'
$gradleDir = Join-Path $cacheDir "gradle-$Version"
$gradleBat = Join-Path $gradleDir 'bin\gradle.bat'

if (Test-Path $gradleBat) {
    Write-Output $gradleBat
    exit 0
}

if (-not (Test-Path $cacheDir)) {
    New-Item -ItemType Directory -Force $cacheDir | Out-Null
}

$zip = Join-Path $cacheDir "gradle-$Version-bin.zip"
$url = "https://services.gradle.org/distributions/gradle-$Version-bin.zip"

if (-not (Test-Path $zip)) {
    Write-Host "Downloading Gradle $Version from $url ..." -ForegroundColor Cyan
    # Use BITS if available (faster), fall back to WebRequest.
    try {
        Start-BitsTransfer -Source $url -Destination $zip -ErrorAction Stop
    } catch {
        Invoke-WebRequest -Uri $url -OutFile $zip -UseBasicParsing
    }
}

Write-Host "Extracting Gradle $Version ..." -ForegroundColor Cyan
Expand-Archive -Path $zip -DestinationPath $cacheDir -Force

if (-not (Test-Path $gradleBat)) {
    throw "Gradle $Version did not unpack to expected path: $gradleBat"
}

Write-Output $gradleBat
