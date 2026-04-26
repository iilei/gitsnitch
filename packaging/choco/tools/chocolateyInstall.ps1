$ErrorActionPreference = 'Stop'

$version  = '{{VERSION}}'
$url64    = "https://github.com/iilei/gitsnitch/releases/download/v${version}/gitsnitch-Windows-msvc-x86_64.zip"
$checksum = '{{SHA256_WIN_X64}}'

$packageArgs = @{
  packageName    = 'gitsnitch'
  unzipLocation  = "$(Split-Path -parent $MyInvocation.MyCommand.Definition)"
  url64bit       = $url64
  checksum64     = $checksum
  checksumType64 = 'sha256'
}

Install-ChocolateyZipPackage @packageArgs
