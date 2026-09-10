// CLOISON Mobile — iOS : WebView (page de chat + moteur WASM) + pont natif
// WKScriptMessageHandler. Le corps envoyé au fournisseur est TOKENISÉ côté JS
// avant l'appel natif — le fournisseur ne voit jamais la PII en clair.

import SwiftUI
import WebKit

/// Contrôleur WKWebView : charge la page locale (assets/Web) et sert le pont
/// JS ↔ natif (config + appel LLM — aucun secret dans la page).
final class WebViewController: NSObject, WKNavigationDelegate, WKScriptMessageHandler {

    private let webView: WKWebView
    /// Callback SwiftUI pour ouvrir l'écran de configuration (bouton ⚙ de la page).
    var onOpenSettings: (() -> Void)?

    override init() {
        let config = WKWebViewConfiguration()
        let contentController = WKUserContentController()
        config.userContentController = contentController
        // Fichiers locaux : autoriser le WASM (fetch + modules depuis file://).
        config.preferences.setValue(true, forKey: "allowFileAccessFromFileURLs")
        config.setValue(true, forKey: "allowUniversalAccessFromFileURLs")
        webView = WKWebView(frame: .zero, configuration: config)
        super.init()
        webView.navigationDelegate = self
        contentController.add(self, name: "CloisonIOS")
        loadLocalPage()
    }

    func view() -> WKWebView { webView }

    private func loadLocalPage() {
        guard let dir = Bundle.main.url(forResource: "Web", withExtension: nil),
              let page = Bundle.main.url(forResource: "index", withExtension: "html", subdirectory: "Web") else {
            // Jamais silencieux : la page est embarquée dans l'app.
            fatalError("CLOISON: assets Web introuvables dans le bundle")
        }
        // allowingReadAccessTo = le dossier Web entier (WASM + glue JS).
        webView.loadFileURL(page, allowingReadAccessTo: dir)
    }

    // MARK: - WKScriptMessageHandler (pont JS → natif)

    func userContentController(_ userContentController: WKUserContentController,
                               didReceive message: WKScriptMessage) {
        guard message.name == "CloisonIOS",
              let body = message.body as? [String: Any],
              let id = body["id"] as? Int,
              let method = body["method"] as? String else { return }

        switch method {
        case "getConfig":
            let json = AppPrefs.load().toJson()
            respond(id, json)

        case "openSettings":
            DispatchQueue.main.async { [weak self] in
                self?.onOpenSettings?()
            }
            respond(id, "{}")

        case "sendToLlm":
            let bodyJson = body["arg"] as? String ?? "{}"
            Task { [weak self] in
                let result = await Self.callLlm(bodyJson: bodyJson)
                self?.respond(id, result)
            }

        default:
            respond(id, "{\"error\":{\"message\":\"méthode inconnue: \(method)\"}}")
        }
    }

    /// Appel natif : erreur → JSON d'erreur lisible par le JS ; succès → corps brut.
    private static func callLlm(bodyJson: String) async -> String {
        do {
            return try await LlmClient.post(cfg: AppPrefs.load(), bodyJson: bodyJson)
        } catch let error as LlmError {
            return "{\"error\":{\"message\":\"\(Self.jsonEscape(error.errorDescription ?? "erreur"))\"}}"
        } catch {
            return "{\"error\":{\"message\":\"\(Self.jsonEscape(error.localizedDescription))\"}}"
        }
    }

    /// Réponse au JS : window.__cloisonResolve(id, jsonString).
    private func respond(_ id: Int, _ json: String) {
        let escaped = Self.jsonEscape(json)
        let js = "window.__cloisonResolve && window.__cloisonResolve(\(id), \"\(escaped)\");"
        DispatchQueue.main.async { [weak self] in
            self?.webView.evaluateJavaScript(js, completionHandler: nil)
        }
    }

    private static func jsonEscape(_ s: String) -> String {
        s.replacingOccurrences(of: "\\", with: "\\\\")
            .replacingOccurrences(of: "\"", with: "\\\"")
            .replacingOccurrences(of: "\n", with: "\\n")
            .replacingOccurrences(of: "\r", with: "\\r")
    }
}

/// Représentable SwiftUI autour de la WebView + bouton ⚙ dans la barre.
struct WebViewContainer: UIViewRepresentable {
    var onOpenSettings: () -> Void

    func makeCoordinator() -> WebViewController {
        let vc = WebViewController()
        vc.onOpenSettings = onOpenSettings
        return vc
    }

    func makeUIView(context: Context) -> WKWebView {
        context.coordinator.view()
    }

    func updateUIView(_ uiView: WKWebView, context: Context) {}
}
