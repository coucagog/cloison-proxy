// CLOISON Mobile (iOS) — glue WASM + logique de chat (tokenize in-app /
// restore). Le moteur @cloison/core tourne ici (wasm-bindgen). L'appel HTTP
// vers le fournisseur LLM passe par le pont natif (CloisonIOS) — le corps
// envoyé contient le texte TOKENISÉ (jamais la PII en clair).
//
// Différence avec Android : le pont WKScriptMessageHandler est ASYNCHRONE
// (postMessage → réponse via window.__cloisonResolve), contrairement au pont
// synchrone JavascriptInterface. La logique de chat est identique.

const $ = (id) => document.getElementById(id);
const messages = $('messages'), status = $('status'), input = $('input');
const btnSend = $('btn-send');

let wasm = null, sessionId = null;

// ===== Pont natif (iOS : WKScriptMessageHandler asynchrone) =====
const bridge = {
  _seq: 0,
  _pending: new Map(),
  call(method, arg) {
    return new Promise((resolve, reject) => {
      const id = ++this._seq;
      this._pending.set(id, { resolve, reject });
      window.webkit.messageHandlers.CloisonIOS.postMessage({ id, method, arg: arg ?? null });
      setTimeout(() => {
        if (this._pending.has(id)) { this._pending.delete(id); reject(new Error('timeout pont natif')); }
      }, 65000);
    });
  },
  getConfig() { return this.call('getConfig'); },
  sendToLlm(body) { return this.call('sendToLlm', body); },
  openSettings() { return this.call('openSettings'); },
};

// Le natif répond via window.__cloisonResolve(id, jsonString).
window.__cloisonResolve = (id, json) => {
  const p = bridge._pending.get(id);
  if (!p) return;
  bridge._pending.delete(id);
  try {
    const obj = JSON.parse(json);
    if (obj && obj.error) p.reject(new Error(obj.error.message || 'erreur native'));
    else p.resolve(json);
  } catch (e) { p.reject(e); }
};

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

async function send() {
  const text = input.value.trim();
  if (!text || !sessionId) return;

  let cfg = null;
  try { cfg = JSON.parse(await bridge.getConfig()); }
  catch (e) { addMsg('system', 'Configuration illisible : ' + e.message); return; }
  if (!cfg || !cfg.baseUrl || !cfg.apiKey) {
    addMsg('system', 'Configurez d\'abord le fournisseur LLM (⚙).');
    bridge.openSettings().catch(() => {});
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
    const raw = await bridge.sendToLlm(body);
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
  status.textContent = 'Module WASM introuvable — voir mobile/ios/README.md (wasm-pack).';
  console.error(e);
}

$('composer').addEventListener('submit', (ev) => { ev.preventDefault(); send(); });
$('btn-settings').addEventListener('click', () => { bridge.openSettings().catch(() => {}); });
