// CLOISON Mobile — iOS : écran de configuration (endpoint LLM + clé + modèle).

import SwiftUI

struct SettingsView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var baseUrl = ""
    @State private var apiKey = ""
    @State private var model = ""

    var body: some View {
        NavigationView {
            Form {
                Section(header: Text("Fournisseur LLM")) {
                    TextField("Base URL (ex. https://openrouter.ai/api/v1)", text: $baseUrl)
                        .keyboardType(.URL)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                    SecureField("Clé API", text: $apiKey)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                    TextField("Modèle (ex. openai/gpt-4o-mini)", text: $model)
                        .autocapitalization(.none)
                        .disableAutocorrection(true)
                }
                Section(footer: Text("Votre clé est stockée dans les préférences de l'app et n'est envoyée qu'au fournisseur configuré.")) {
                    Button("Enregistrer") {
                        AppPrefs.save(LlmConfig(
                            baseUrl: baseUrl.trimmingCharacters(in: .whitespacesAndNewlines),
                            apiKey: apiKey.trimmingCharacters(in: .whitespacesAndNewlines),
                            model: model.trimmingCharacters(in: .whitespacesAndNewlines)
                        ))
                        dismiss()
                    }
                }
            }
            .navigationTitle("Configuration")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Fermer") { dismiss() }
                }
            }
            .onAppear {
                let c = AppPrefs.load()
                baseUrl = c.baseUrl
                apiKey = c.apiKey
                model = c.model
            }
        }
    }
}
