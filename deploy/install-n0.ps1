<#
=============================================================================
CLOISON N0 — Installation du daemon desktop (Windows), DEPUIS LES RELEASES.

Télécharge le binaire `cloison-proxy.exe` + le NER léger embarqué (ONNX int8)
+ la lib onnxruntime.dll depuis la release GitHub, vérifie les checksums,
génère la clé locataire et affiche la configuration minimale.

Aucune toolchain Rust, aucun torch (charte §4 : N0 = moteur Rust seul).

Usage :
  powershell -ExecutionPolicy Bypass -File install-n0.ps1
  powershell -ExecutionPolicy Bypass -File install-n0.ps1 -Version v0.3.0 -Prefix "$env:USERPROFILE\.cloison" -SkipNer

Après installation : configurez l'environnement (affiché à la fin) puis
lancez <prefix>\cloison-proxy.exe — voir docs/N0.md §3.
=============================================================================
#>
[CmdletBinding()]
param(
  [string]$Version = "latest",
  [string]$Prefix  = (Join-Path $env:USERPROFILE ".cloison"),
  [switch]$SkipNer
)
$ErrorActionPreference = "Stop"

# --- Cible de release ---------------------------------------------------------
$arch = $env:PROCESSOR_ARCHITECTURE
if ($arch -eq "AMD64") { $Target = "x86_64-pc-windows-msvc" }
elseif ($arch -eq "ARM64") { $Target = "aarch64-pc-windows-msvc" }
else { Write-Error "Architecture non supportée : $arch (AMD64 attendu pour la v1)" }

$Base  = "https://github.com/coucagog/cloison/releases/download"
$Latest = "https://github.com/coucagog/cloison/releases/latest/download"
function Get-AssetUrl([string]$name) {
  if ($Version -eq "latest") { return "$Latest/$name" }
  return "$Base/$Version/$name"
}

Write-Host "==> CLOISON N0 — installation daemon desktop (moteur Rust seul)" -ForegroundColor Cyan
Write-Host "    cible : $Target   ->   $Prefix"
New-Item -ItemType Directory -Force -Path $Prefix, (Join-Path $Prefix "ner") | Out-Null

# curl.exe (Windows 10+) ; repli Invoke-WebRequest
function Get-Download([string]$url, [string]$out) {
  if (Get-Command curl.exe -ErrorAction SilentlyContinue) {
    & curl.exe -fsSL -o $out $url
    if ($LASTEXITCODE -ne 0) { throw "curl a échoué sur $url (exit $LASTEXITCODE)" }
  } else {
    Invoke-WebRequest -Uri $url -OutFile $out -UseBasicParsing
  }
}

# --- 1. Binaire ----------------------------------------------------------------
$bin = Join-Path $Prefix "cloison-proxy.exe"
Write-Host "==> téléchargement du binaire ($Target)…"
Get-Download (Get-AssetUrl "cloison-proxy-$Target") $bin

# --- 2. Checksums ---------------------------------------------------------------
$sumFile = Join-Path $Prefix "checksums.txt"
Write-Host "==> vérification d'intégrité (checksums.txt)…"
Get-Download (Get-AssetUrl "checksums.txt") $sumFile
$sums = @{}
Get-Content $sumFile | ForEach-Object {
  $parts = $_ -split '\s+'
  if ($parts.Count -ge 2) { $sums[$parts[1]] = $parts[0] }
}
if (-not $sums.ContainsKey("cloison-proxy-$Target")) {
  throw "checksums.txt ne référence pas le binaire $Target — release incomplète ?"
}
function Assert-Checksum([string]$file, [string]$name) {
  if (-not $sums.ContainsKey($name)) { throw "$name absent de checksums.txt" }
  $actual = (Get-FileHash -Algorithm SHA256 -Path $file).Hash.ToLowerInvariant()
  if ($actual -ne $sums[$name]) {
    throw "checksum invalide pour $name (attendu $($sums[$name]), obtenu $actual)"
  }
}
Assert-Checksum $bin "cloison-proxy-$Target"

# --- 3. NER léger + lib onnxruntime ----------------------------------------------
if ($SkipNer) {
  Write-Host "==> -SkipNer : NER léger non installé (limite « texte libre » assumée)" -ForegroundColor Yellow
} else {
  $nerTgz = Join-Path $Prefix "ner-lite.tar.gz"
  Write-Host "==> téléchargement du NER léger (modèle ONNX int8, ~135 Mo)…"
  Get-Download (Get-AssetUrl "cloison-n0-ner-lite.tar.gz") $nerTgz
  Assert-Checksum $nerTgz "cloison-n0-ner-lite.tar.gz"
  & tar.exe -xzf $nerTgz -C (Join-Path $Prefix "ner")
  if ($LASTEXITCODE -ne 0) { throw "extraction du NER léger a échoué" }
  Remove-Item $nerTgz -Force

  $ortTgz = Join-Path $Prefix "ort.tar.gz"
  Write-Host "==> téléchargement de la lib onnxruntime (Windows)…"
  Get-Download (Get-AssetUrl "cloison-n0-onnxruntime-$Target.tar.gz") $ortTgz
  Assert-Checksum $ortTgz "cloison-n0-onnxruntime-$Target.tar.gz"
  & tar.exe -xzf $ortTgz -C (Join-Path $Prefix "ner")
  if ($LASTEXITCODE -ne 0) { throw "extraction de la lib onnxruntime a échoué" }
  Remove-Item $ortTgz -Force

  if (-not (Test-Path (Join-Path $Prefix "ner\model-int8.onnx"))) {
    throw "modèle introuvable après extraction"
  }
}
Remove-Item $sumFile -Force -ErrorAction SilentlyContinue

# --- 4. Clé locataire (affichée UNE fois — dérive les jetons) --------------------
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$bytes = New-Object byte[] 32
$rng.GetBytes($bytes)
$tenantKey = -join ($bytes | ForEach-Object { $_.ToString("x2") })
Write-Host "==> clé locataire (à conserver précieusement, JAMAIS à committer) :" -ForegroundColor Green
Write-Host "    CLOISON_TENANT_KEY_HEX=$tenantKey"

# --- 5. Configuration minimale affichée -------------------------------------------
$vault = Join-Path $Prefix "vault.redb"
$nerModel = Join-Path $Prefix "ner\model-int8.onnx"
$nerTok = Join-Path $Prefix "ner\tokenizer.json"
$ortLib = Join-Path $Prefix "ner\onnxruntime.dll"
$quoted = @'
$env:CLOISON_ROLE = "edge"
$env:CLOISON_LISTEN_ADDR = "127.0.0.1:8787"
$env:CLOISON_UPSTREAM_BASE_URL = "https://openrouter.ai/api/v1"   # votre fournisseur
$env:CLOISON_VAULT_PATH = "<VAULT>"
$env:CLOISON_VAULT_PASSPHRASE = "<VOTRE passphrase — choisie par vous, jamais stockée>"
$env:CLOISON_EXPECTED_ACCESS_TOKEN = "<votre jeton mn_ local>"
$env:CLOISON_TENANT_KEY_HEX = "<la clé affichée ci-dessus>"
'@
$quoted = $quoted.Replace("<VAULT>", $vault)
Write-Host "`n==> Configuration N0 (docs/N0.md §3 — à placer dans votre profil / service) :" -ForegroundColor Cyan
Write-Host $quoted
if (-not $SkipNer) {
  Write-Host @"

# NER léger embarqué (PERSON/LOC in-core — N0 v1.2, ARBITRAGE-04) :
`$env:CLOISON_NER_MODEL_ONNX = "$nerModel"
`$env:CLOISON_NER_TOKENIZER = "$nerTok"
`$env:CLOISON_ONNX_LIB = "$ortLib"
# `$env:CLOISON_NER_THRESHOLD = "0.70"   # défaut 0.70 (calibration GO)

# Passphrase via le keychain Windows (recommandé — jamais en clair par CLOISON) :
# `$env:CLOISON_VAULT_KEYCHAIN_SERVICE = "cloison-n0"
"@
}
Write-Host "`n==> Lancement : $bin"
Write-Host "    Vérification : docs/N0.md §5. Installation terminée ✅"
