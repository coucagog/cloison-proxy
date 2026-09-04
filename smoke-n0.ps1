<#
=============================================================================
CLOISON N0 — Smoke test du daemon local (Windows).

Valide un binaire `cloison-proxy.exe` installé : boot avec coffre, roundtrip
aller/retour contre un faux LLM (mock_llm.py), masquage amont prouvé (le mock
reçoit des sentinelles ⟦ et PAS la PII) et restauration côté client (zéro
sentinelle résiduelle). Optionnellement, active le NER léger embarqué et
vérifie le masquage d'un nom HORS gazetteer.

Usage :
  powershell -ExecutionPolicy Bypass -File deploy\smoke-n0.ps1 `
      -Binary C:\Users\...\.cloison\cloison-proxy.exe `
      [-NerPrefix C:\Users\...\.cloison] `
      [-ListenPort 18787] [-MockPort 8799]

Résultat : sortie 0 = SUCCÈS (masquage amont + restauration client prouvés).
=============================================================================
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory=$true)][string]$Binary,
  [string]$NerPrefix = "",
  [int]$ListenPort = 18787,
  [int]$MockPort = 8799
)
$ErrorActionPreference = "Stop"

$python = (Get-Command python -ErrorAction SilentlyContinue).Source
if (-not $python) { throw "python requis pour le mock LLM (mock_llm.py)" }

$work = Join-Path $env:TEMP ("cloison-smoke-" + [guid]::NewGuid().ToString("N"))
New-Item -ItemType Directory -Force -Path $work | Out-Null
$mockLog = Join-Path $work "last_body.json"
$vault = Join-Path $work "vault.redb"

# Clé locataire aléatoire (synthétique — jamais une vraie clé).
$rng = [System.Security.Cryptography.RandomNumberGenerator]::Create()
$bytes = New-Object byte[] 32; $rng.GetBytes($bytes)
$tenantKey = -join ($bytes | ForEach-Object { $_.ToString("x2") })

$mock = $null; $daemon = $null
try {
  # --- 1. Faux LLM (echo, journalise le corps reçu) -------------------------
  $env:MOCK_PORT = "$MockPort"
  $env:MOCK_LOG_FILE = $mockLog
  $mock = Start-Process -FilePath $python -ArgumentList @(
    (Join-Path $PSScriptRoot "mock_llm.py")) -PassThru -WindowStyle Hidden
  Start-Sleep -Seconds 2

  # --- 2. Daemon N0 ---------------------------------------------------------
  $env:CLOISON_ROLE = "edge"
  $env:CLOISON_LISTEN_ADDR = "127.0.0.1:$ListenPort"
  $env:CLOISON_UPSTREAM_BASE_URL = "http://127.0.0.1:$MockPort"
  $env:CLOISON_VAULT_PATH = $vault
  # Mêmes valeurs que les tests e2e_n0 validés (portage exact — STACK-N0V13).
  $env:CLOISON_VAULT_PASSPHRASE = "passphrase-n0-locale-de-test"
  $env:CLOISON_EXPECTED_ACCESS_TOKEN = "mn_testtoken"
  $env:CLOISON_TENANT_KEY_HEX = "42" * 32
  $env:CLOISON_MOCK_MODE = "1"
  if ($NerPrefix) {
    $env:CLOISON_NER_MODEL_ONNX = Join-Path $NerPrefix "ner\model-int8.onnx"
    $env:CLOISON_NER_TOKENIZER = Join-Path $NerPrefix "ner\tokenizer.json"
    $env:CLOISON_ONNX_LIB = Join-Path $NerPrefix "ner\onnxruntime.dll"
  }
  $daemon = Start-Process -FilePath $Binary -PassThru -WindowStyle Hidden `
    -RedirectStandardOutput (Join-Path $work "daemon.log") `
    -RedirectStandardError (Join-Path $work "daemon.err")
  $up = $false
  for ($i = 0; $i -lt 40; $i++) {
    Start-Sleep -Seconds 1
    if ($daemon.HasExited) { break }
    # Invoke-WebRequest LÈVE une exception sur 401 (non-2xx) — ici le 401 est
    # le signe que l'auth composite est active (daemon prêt à servir).
    try {
      Invoke-WebRequest -Uri "http://127.0.0.1:$ListenPort/v1/models" `
        -UseBasicParsing -TimeoutSec 2 -ErrorAction Stop | Out-Null
      $up = $true; break           # 200 : auth absente (mock) — prêt aussi
    } catch {
      $resp = $_.Exception.Response
      if ($resp -and [int]$resp.StatusCode -eq 401) { $up = $true; break }
    }
  }
  if (-not $up) {
    Write-Host "❌ daemon non démarré (log ci-dessous) :" -ForegroundColor Red
    Get-Content (Join-Path $work "daemon.log") -ErrorAction SilentlyContinue
    Get-Content (Join-Path $work "daemon.err") -ErrorAction SilentlyContinue
    throw "boot du daemon N0 a échoué"
  }
  Write-Host "==> daemon N0 démarré sur 127.0.0.1:$ListenPort"

  # --- 3. Requête avec PII SYNTHÉTIQUE ---------------------------------------
  $pii = "Contact: Aminata Diop, user@example.com, tel +221 77 123 45 67"
  if ($NerPrefix) { $pii = "Appelez Xolani Ndlovu au 77 123 45 67, il habite à Ziguinchor." }
  $body = @{
    model = "mock-model"
    messages = @(@{ role = "user"; content = $pii })
    max_tokens = 200
  } | ConvertTo-Json -Depth 5
  $resp = Invoke-RestMethod -Uri "http://127.0.0.1:$ListenPort/v1/chat/completions" `
    -Method Post -ContentType "application/json" `
    -Headers @{ Authorization = "Bearer mn_testtoken.sk-test.key.with.dots" } `
    -Body ([System.Text.Encoding]::UTF8.GetBytes($body))
  $content = $resp.choices[0].message.content
  Write-Host "réponse client : $content"

  # --- 4. Assertions ----------------------------------------------------------
  $fail = 0
  $sentinel = [string][char]0x27e6        # ⟦ (PS 5.1 : pas de \uXXXX en regex)
  # 4a. Le corps reçu par le MOCK contient des sentinelles ⟦ et PAS la PII.
  if (-not (Test-Path $mockLog)) { throw "le mock n'a pas journalisé le corps" }
  $upstream = Get-Content $mockLog -Raw -Encoding UTF8
  if ($upstream -notmatch [regex]::Escape($sentinel)) {
    Write-Host "❌ le mock n'a reçu AUCUNE sentinelle — proxy pass-through ?" -ForegroundColor Red
    $fail++
  }
  foreach ($p in @("Aminata Diop", "user@example.com", "77 123 45 67", "Xolani Ndlovu", "Ziguinchor")) {
    if ($upstream -match [regex]::Escape($p)) {
      Write-Host "❌ PII en clair reçue par l'amont : $p" -ForegroundColor Red
      $fail++
    }
  }
  # 4b. La réponse client : PII restaurée, aucune sentinelle. La VILLE est
  # généralisée par design en N0 (Policy::n0_for — faible cardinalité, jamais
  # de jeton, docs/N0.md §3) → `[VILLE_SN]`, pas le toponyme en clair.
  if ($content -match [regex]::Escape($sentinel)) {
    Write-Host "❌ sentinelle résiduelle dans la réponse client" -ForegroundColor Red
    $fail++
  }
  if ($NerPrefix) {
    if ($content -notmatch "Xolani") { Write-Host "❌ NER : 'Xolani' non restauré" -ForegroundColor Red; $fail++ }
    if ($content -notmatch "77 123 45 67") { Write-Host "❌ téléphone non restauré" -ForegroundColor Red; $fail++ }
    if ($content -notmatch "\[VILLE_SN\]") { Write-Host "❌ ville non généralisée ([VILLE_SN] attendu)" -ForegroundColor Red; $fail++ }
    if ($content -match "Ziguinchor") { Write-Host "❌ toponyme 'Ziguinchor' en clair (généralisation attendue)" -ForegroundColor Red; $fail++ }
  } else {
    foreach ($p in @("Aminata Diop", "user@example.com", "77 123 45 67")) {
      if ($content -notmatch [regex]::Escape($p)) {
        Write-Host "❌ valeur non restaurée côté client : $p" -ForegroundColor Red
        $fail++
      }
    }
  }

  # Arrêt du daemon AVANT le scan du coffre (redb verrouille les plages du
  # fichier sur Windows — lecture impossible tant qu'il est ouvert, constat
  # CI test-n0-os / journal STACK-N0V13).
  if ($daemon -and -not $daemon.HasExited) {
    Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue
    $daemon = $null
    Start-Sleep -Milliseconds 500
  }

  # 4c. Aucun clair dans le coffre.
  $vaultBytes = [System.IO.File]::ReadAllBytes($vault)
  $hay = [System.Text.Encoding]::ASCII.GetString($vaultBytes)
  if ($hay -match "Aminata") { Write-Host "❌ clair trouvé dans le coffre" -ForegroundColor Red; $fail++ }

  if ($fail -eq 0) {
    Write-Host "✅ SMOKE TEST N0 RÉUSSI — masquage amont + restauration client prouvés" -ForegroundColor Green
    exit 0
  } else {
    Write-Host "❌ SMOKE TEST ÉCHOUÉ ($fail assertion(s))" -ForegroundColor Red
    exit 1
  }
} finally {
  if ($daemon -and -not $daemon.HasExited) { Stop-Process -Id $daemon.Id -Force -ErrorAction SilentlyContinue }
  if ($mock -and -not $mock.HasExited) { Stop-Process -Id $mock.Id -Force -ErrorAction SilentlyContinue }
  Remove-Item Env:CLOISON_* -ErrorAction SilentlyContinue
  Remove-Item Env:MOCK_* -ErrorAction SilentlyContinue
  Remove-Item $work -Recurse -Force -ErrorAction SilentlyContinue
}
