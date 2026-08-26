// CLOISON Mobile — client HTTP minimal (POST {base}/chat/completions).
// Le corps reçu est DÉJÀ tokenisé par le WASM côté JS : le fournisseur ne
// voit jamais la PII en clair. Réponse brute renvoyée au JS pour restauration.
//
// Swift Concurrency : tâche async, résultat via Task { @MainActor }.

import Foundation

enum LlmError: LocalizedError {
    case invalidUrl
    case http(Int, String)

    var errorDescription: String? {
        switch self {
        case .invalidUrl:
            return "Base URL invalide"
        case .http(let code, let text):
            return "HTTP \(code): \(text)"
        }
    }
}

enum LlmClient {

    /// POST {base}/chat/completions avec le corps JSON (déjà tokenisé).
    static func post(cfg: LlmConfig, bodyJson: String) async throws -> String {
        guard let base = URL(string: cfg.baseUrl.trimmingCharacters(in: .whitespacesAndNewlines)),
              base.scheme != nil, base.host != nil else {
            throw LlmError.invalidUrl
        }
        let url = base.appendingPathComponent("chat/completions")
        var req = URLRequest(url: url)
        req.httpMethod = "POST"
        req.timeoutInterval = 60
        req.setValue("application/json", forHTTPHeaderField: "Content-Type")
        req.setValue("Bearer \(cfg.apiKey)", forHTTPHeaderField: "Authorization")
        req.httpBody = bodyJson.data(using: .utf8)

        let (data, resp) = try await URLSession.shared.data(for: req)
        guard let http = resp as? HTTPURLResponse else {
            throw LlmError.http(0, "réponse non-HTTP")
        }
        let text = String(data: data, encoding: .utf8) ?? ""
        guard (200...299).contains(http.statusCode) else {
            throw LlmError.http(http.statusCode, text)
        }
        return text
    }
}
