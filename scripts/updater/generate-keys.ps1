#requires -Version 7.0
<#
.SYNOPSIS
  Generate a new minisign keypair for the BentoDesk Tauri updater.

.DESCRIPTION
  Runs `tauri signer generate` to mint a fresh keypair, writes the
  private key (passphrase-protected) to `~/.tauri/bentodesk.key` by
  default, and prints the base64-encoded public key that should be
  pasted into `src-tauri/tauri.conf.json` under `plugins.updater.pubkey`.

  Private key material is written by the Tauri CLI itself; this script
  never touches it. The passphrase is prompted securely.

  The public key output is copied to the clipboard for convenience so
  the operator can paste it into `tauri.conf.json` without risking
  accidental truncation when scrolling through terminal output.

.EXAMPLE
  ./generate-keys.ps1

.EXAMPLE
  # Store the key at a custom location for repo-specific CI secrets.
  ./generate-keys.ps1 -PrivateKeyPath "D:\secrets\bentodesk-updater.key"

.NOTES
  After the keypair is generated:
    1. Upload the contents of the private key file to GitHub Actions
       secret `TAURI_SIGNING_PRIVATE_KEY` (base64 encode first if the
       file contains newlines Tauri CI cannot parse).
    2. Upload the passphrase to GitHub Actions secret
       `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
    3. Paste the printed public key into `tauri.conf.json` →
       `plugins.updater.pubkey`.
    4. Commit the updated `tauri.conf.json` and cut a tagged release
       using `.github/workflows/release-with-updater.yml`.

  Rotating keys: delete the old secrets, regenerate with this script,
  publish a new release signed with the new key, AND ship an in-app
  forced-update notice explaining why the old binary cannot auto-update
  anymore (the embedded pubkey no longer matches the signature).
#>
param(
    [string]$PrivateKeyPath = "$HOME/.tauri/bentodesk.key"
)

$ErrorActionPreference = "Stop"

Write-Host "BentoDesk Tauri updater — key generation" -ForegroundColor Cyan
Write-Host "--------------------------------------------"
Write-Host "Private key path: $PrivateKeyPath"
Write-Host ""

# Ensure the target directory exists.
$parent = Split-Path -Parent $PrivateKeyPath
if (-not (Test-Path $parent)) {
    Write-Host "Creating directory: $parent"
    New-Item -Path $parent -ItemType Directory -Force | Out-Null
}

if (Test-Path $PrivateKeyPath) {
    $answer = Read-Host "File exists at $PrivateKeyPath. Overwrite? [y/N]"
    if ($answer -notmatch '^[yY]$') {
        Write-Host "Aborted. Existing key left in place." -ForegroundColor Yellow
        exit 1
    }
}

Write-Host "Generating keypair via @tauri-apps/cli..."
Write-Host "You will be prompted to set a passphrase that protects the private key."
Write-Host ""

npx --yes @tauri-apps/cli signer generate -w $PrivateKeyPath

if ($LASTEXITCODE -ne 0) {
    Write-Host "Key generation failed (exit code $LASTEXITCODE)." -ForegroundColor Red
    exit $LASTEXITCODE
}

$pubKeyPath = "$PrivateKeyPath.pub"
if (-not (Test-Path $pubKeyPath)) {
    Write-Host "Expected public key at $pubKeyPath but it was not produced." -ForegroundColor Red
    exit 1
}

$pubKey = Get-Content -Raw $pubKeyPath
Write-Host ""
Write-Host "Public key (copy into tauri.conf.json → plugins.updater.pubkey):" -ForegroundColor Green
Write-Host "--------------------------------------------------------------"
Write-Host $pubKey
Write-Host "--------------------------------------------------------------"

try {
    Set-Clipboard -Value $pubKey
    Write-Host "(Public key copied to clipboard.)" -ForegroundColor Green
} catch {
    Write-Host "Clipboard copy failed: $($_.Exception.Message)" -ForegroundColor Yellow
}

Write-Host ""
Write-Host "Next steps:"
Write-Host "  1. Paste the public key into tauri.conf.json."
Write-Host "  2. Upload the private key + passphrase to GitHub Secrets."
Write-Host "  3. Tag a release matching the '*-updater' pattern to trigger"
Write-Host "     .github/workflows/release-with-updater.yml."
