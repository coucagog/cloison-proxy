// CLOISON Mobile — configuration de l'app (endpoint LLM + clé + modèle).
// Stockée dans UserDefaults (votre clé amont, jamais transmise ailleurs que
// vers le fournisseur configuré — charte §7.2).

import Foundation

struct LlmConfig: Codable, Equatable {
    var baseUrl: String
    var apiKey: String
    var model: String

    static let `default` = LlmConfig(
        baseUrl: "",
        apiKey: "",
        model: "openai/gpt-4o-mini"
    )

    /// JSON sérialisé vers la page Web (aucun secret supplémentaire).
    func toJson() -> String {
        let obj: [String: String] = [
            "baseUrl": baseUrl,
            "apiKey": apiKey,
            "model": model,
        ]
        let data = (try? JSONSerialization.data(withJSONObject: obj)) ?? Data("{}".utf8)
        return String(data: data, encoding: .utf8) ?? "{}"
    }
}

enum AppPrefs {
    private static let kBaseUrl = "cloison_base_url"
    private static let kApiKey = "cloison_api_key"
    private static let kModel = "cloison_model"

    static func load() -> LlmConfig {
        let d = UserDefaults.standard
        return LlmConfig(
            baseUrl: d.string(forKey: kBaseUrl) ?? "",
            apiKey: d.string(forKey: kApiKey) ?? "",
            model: d.string(forKey: kModel) ?? LlmConfig.default.model
        )
    }

    static func save(_ c: LlmConfig) {
        let d = UserDefaults.standard
        d.set(c.baseUrl, forKey: kBaseUrl)
        d.set(c.apiKey, forKey: kApiKey)
        d.set(c.model.isEmpty ? LlmConfig.default.model : c.model, forKey: kModel)
    }
}
