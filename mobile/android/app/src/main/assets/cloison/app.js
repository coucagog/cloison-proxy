// CLOISON Mobile — glue WASM + logique de chat (tokenize in-app / restore).
// Le moteur @cloison/core tourne ici (wasm-bindgen). L'appel HTTP vers le
// fournisseur LLM passe par le pont natif (CloisonAndroid.sendToLlm) — le
// corps envoyé contient le texte TOKENISÉ (jamais la PII en clair).

const $ = (id) => document.getElementById(id);
const messages = $('messages'), status = $('status'), input = $('input');
const btnSend = $('btn-send');

let wasm = null, sessionId = null;

function b64(bytes) {
  let s = '';
  for (const b of bytes) s += String.fromCharCode(b);
  return btoa(s);
}

function addMsg(role, text) {
  const div = document.createElement('div');
  div.className = 'msg ' + role;
  div.textContent = text;
  messages.appendChild(div);
  messages.scrollTop = messages.scrollHeight;
}

async function newSession() {
  // Clé locataire aléatoire locale (crypto.getRandomValues — jamais embarquée).
  const key = new Uint8Array(32);
  crypto.getRandomValues(key);
  sessionId = wasm.cloisonInitSession(b64(key));
  status.textContent = 'moteur prêt — session WASM #' + sessionId + ' (clé locale, coffre in-memory)';
  btnSend.disabled = false;
}

function config() {
  // Lue via le pont natif (SharedPreferences) : {baseUrl, apiKey, model}.
  return window.CloisonAndroid.getConfig ? JSON.parse(window.CloisonAndroid.getConfig()) : null;
}

async function send() {
  const text = input.value.trim();
  if (!text || !sessionId) return;
  const cfg = config();
  if (!cfg || !cfg.baseUrl || !cfg.apiKey) {
    addMsg('system', 'Configurez d\'abord le fournisseur LLM (⚙).');
    return;
  }
  input.value = '';
  addMsg('user', text);
  btnSend.disabled = true;
  status.textContent = 'pseudonymisation in-app…';

  try {
    // 1. Tokenisation DANS l'app : le clair ne sort jamais.
    const tok = JSON.parse(wasm.cloisonTokenize(sessionId, text));
    status.textContent = tok.tokens.length + ' jeton(s) émis — envoi au fournisseur…';

    // 2. Appel natif au fournisseur (corps TOKENISÉ, jamais la PII).
    const body = JSON.stringify({
      model: cfg.model || 'openai/gpt-4o-mini',
      messages: [{ role: 'user', content: tok.text }],
      max_tokens: 1024,
    });
    const raw = window.CloisonAndroid.sendToLlm(body);
    const resp = JSON.parse(raw);
    if (resp.error) throw new Error(resp.error.message || 'erreur fournisseur');

    // 3. Restauration in-app de la réponse (registre de la requête + MAC).
    const content = (resp.choices && resp.choices[0] && resp.choices[0].message && resp.choices[0].message.content) || '';
    const restored = wasm.cloisonRestore(sessionId, content);
    addMsg('assistant', restored);
    status.textContent = 'restauré — aucun jeton résiduel.';
  } catch (e) {
    addMsg('system', 'Erreur : ' + e.message);
    status.textContent = 'échec — la session reste utilisable.';
  } finally {
    btnSend.disabled = false;
  }
}

try {
  wasm = await import('./pkg/cloison_wasm.js');
  await wasm.default();
  await newSession();
} catch (e) {
  status.textContent = 'Module WASM introuvable — voir mobile/android/README.md (wasm-pack).';
  console.error(e);
}

$('composer').addEventListener('submit', (ev) => { ev.preventDefault(); send(); });
$('btn-settings').addEventListener('click', () => {
  if (window.CloisonAndroid.openSettings) window.CloisonAndroid.openSettings();
});
