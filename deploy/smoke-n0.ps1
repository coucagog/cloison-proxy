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
  $env:CLOISON_VAULT_PASSPHRASE = "passphrase-de-test-synthetique"
  $env:CLOISON_EXPECTED_ACCESS_TOKEN = "testtoken"
  $env:CLOISON_TENANT_KEY_HEX = $tenantKey
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
  for ($i = 0; $i -lt 30; $i++) {
    Start-Sleep -Seconds 1
    if ($daemon.HasExited) { break }
    try {
      $r = Invoke-WebRequest -Uri "http://127.0.0.1:$ListenPort/v1/models" `
        -UseBasicParsing -TimeoutSec 2 -ErrorAction SilentlyContinue
      if ($r.StatusCode -eq 401) { $up = $true; break }   # auth composite active
    } catch { }
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
    -Headers @{ Authorization = "Bearer testtoken.sk-test" } -Body $body
  $content = $resp.choices[0].message.content
  Write-Host "réponse client : $content"

  # --- 4. Assertions ----------------------------------------------------------
  $fail = 0
  # 4a. Le corps reçu par le MOCK contient des sentinelles ⟦ et PAS la PII.
  if (-not (Test-Path $mockLog)) { throw "le mock n'a pas journalisé le corps" }
  $upstream = Get-Content $mockLog -Raw
  if ($upstream -notmatch "\u27e6") {
    Write-Host "❌ le mock n'a reçu AUCUNE sentinelle — proxy pass-through ?" -ForegroundColor Red
    $fail++
  }
  foreach ($p in @("Aminata Diop", "user@example.com", "77 123 45 67", "Xolani Ndlovu", "Ziguinchor")) {
    if ($upstream -match [regex]::Escape($p)) {
      Write-Host "❌ PII en clair reçue par l'amont : $p" -ForegroundColor Red
      $fail++
    }
  }
  # 4b. La réponse client contient la PII restaurée, aucune sentinelle.
  if ($content -match "\u27e6") {
    Write-Host "❌ sentinelle résiduelle dans la réponse client" -ForegroundColor Red
    $fail++
  }
  foreach ($p in @("Aminata Diop", "user@example.com", "77 123 45 67")) {
    if ($content -notmatch [regex]::Escape($p)) {
      Write-Host "❌ valeur non restaurée côté client : $p" -ForegroundColor Red
      $fail++
    }
  }
  if ($NerPrefix) {
    if ($content -notmatch "Xolani") { Write-Host "❌ NER : 'Xolani' non restauré" -ForegroundColor Red; $fail++ }
    if ($content -notmatch "Ziguinchor") { Write-Host "❌ ville 'Ziguinchor' non restaurée" -ForegroundColor Red; $fail++ }
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
