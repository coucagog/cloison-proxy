# =============================================================================
# CLOISON N0 — configure-n0.ps1 : configuration du daemon local (Windows)
# =============================================================================
# Prérequis : cloison-proxy.exe installé via install-n0.ps1 (ou dans le PATH).
# Ce script génère %USERPROFILE%\.cloison\n0.env.ps1 + start-n0.ps1, et
# affiche la clé composite à saisir dans votre interface IA.
# Idempotent : rejouable ; il ne démarre le daemon qu'avec -Start.
#
# Usage :
#   powershell -ExecutionPolicy Bypass -File .\configure-n0.ps1
#   powershell -ExecutionPolicy Bypass -File .\configure-n0.ps1 -Start
# =============================================================================
param([switch]$Start)
$ErrorActionPreference = 'Stop'

$Prefix = if ($env:CLOISON_PREFIX) { $env:CLOISON_PREFIX } else { Join-Path $env:USERPROFILE '.cloison' }
$EnvFile     = Join-Path $Prefix 'n0.env.ps1'
$StartScript = Join-Path $Prefix 'start-n0.ps1'

if (-not (Get-Command cloison-proxy.exe -ErrorAction SilentlyContinue)) {
  Write-Error "ERREUR : 'cloison-proxy.exe' introuvable dans le PATH.`n  Installez d'abord :`n  powershell -ExecutionPolicy Bypass -File https://raw.githubusercontent.com/coucagog/cloison-proxy/main/install-n0.ps1"
  exit 1
}

New-Item -ItemType Directory -Force -Path $Prefix | Out-Null

$Upstream = Read-Host 'Base URL amont [https://openrouter.ai/api/v1]'
if (-not $Upstream) { $Upstream = 'https://openrouter.ai/api/v1' }

$Pass = Read-Host 'Passphrase du coffre (vide = générer)'
if (-not $Pass) {
  $Pass = -join (1..24 | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })
  Write-Host "  Passphrase générée (stockée dans $EnvFile)."
}

$Token = Read-Host "Jeton d'acces local mn_ (vide = générer)"
if (-not $Token) { $Token = 'mn_' + (-join (1..16 | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })) }

$TenantKey = -join (1..32 | ForEach-Object { '{0:x2}' -f (Get-Random -Max 256) })

# NER léger embarqué : ajouté à la config si le bundle est présent.
$NerBlock = ''
$NerModel = Join-Path $Prefix 'ner\model-int8.onnx'
$NerTok   = Join-Path $Prefix 'ner\tokenizer.json'
if ((Test-Path $NerModel) -and (Test-Path $NerTok)) {
  $NerBlock = "`$env:CLOISON_NER_MODEL_ONNX='$NerModel'`n`$env:CLOISON_NER_TOKENIZER='$NerTok'`n`$env:CLOISON_ONNX_LIB='onnxruntime.dll'"
}

$Stamp = (Get-Date).ToUniversalTime().ToString('s')
@"
# CLOISON N0 — configuration locale générée par configure-n0.ps1 ($Stamp)
# NE JAMAIS COMMITER, NE JAMAIS PUBLIER.
# Clé composite à saisir côté client :  $Token.<votre clé fournisseur>
`$env:CLOISON_ROLE='edge'
`$env:CLOISON_LISTEN_ADDR='127.0.0.1:8787'
`$env:CLOISON_UPSTREAM_BASE_URL='$Upstream'
`$env:CLOISON_VAULT_PATH='$(Join-Path $Prefix 'vault.redb')'
`$env:CLOISON_VAULT_PASSPHRASE='$Pass'
`$env:CLOISON_EXPECTED_ACCESS_TOKEN='$Token'
`$env:CLOISON_TENANT_KEY_HEX='$TenantKey'
$NerBlock
"@ | Set-Content -Encoding UTF8 $EnvFile

@"
# Démarre le daemon CLOISON N0 avec la configuration locale.
. "$EnvFile"
& cloison-proxy.exe
"@ | Set-Content -Encoding UTF8 $StartScript

Write-Host ""
Write-Host "Configuration écrite : $EnvFile"
Write-Host ""
Write-Host "Clé composite pour votre interface IA :"
Write-Host "  Base URL : http://localhost:8787/v1"
Write-Host "  Clé      : $Token.<votre clé fournisseur>"
Write-Host ""
if ($Start) {
  Write-Host "Démarrage du daemon (Ctrl-C pour arrêter)..."
  & $StartScript
} else {
  Write-Host "Pour démarrer : $StartScript"
}
