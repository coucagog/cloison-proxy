// CLOISON Mobile — activité principale : WebView (page de chat + moteur WASM)
// + pont natif pour l'appel LLM (le corps est TOKENISÉ côté JS avant l'envoi).
package com.cloison.app

import android.annotation.SuppressLint
import android.content.Intent
import android.os.Bundle
import android.webkit.JavascriptInterface
import android.webkit.WebView
import android.webkit.WebViewClient
import androidx.appcompat.app.AppCompatActivity

class MainActivity : AppCompatActivity() {

    private lateinit var webView: WebView

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        webView = WebView(this)
        setContentView(webView)

        webView.settings.javaScriptEnabled = true
        webView.settings.domStorageEnabled = true
        webView.settings.allowFileAccess = true
        webView.webViewClient = WebViewClient()

        // Pont JS ↔ natif : config + appel LLM (aucun secret dans la page).
        webView.addJavascriptInterface(CloisonBridge(), "CloisonAndroid")

        // La page et le WASM sont des ASSETS locaux — aucun réseau vers CLOISON.
        webView.loadUrl("file:///android_asset/cloison/index.html")
    }

    @SuppressLint("SetJavaScriptEnabled")
    private inner class CloisonBridge {
        @JavascriptInterface
        fun getConfig(): String = AppPrefs.config(this@MainActivity).toJson()

        @JavascriptInterface
        fun openSettings() {
            startActivity(Intent(this@MainActivity, SettingsActivity::class.java))
        }

        /** Corps JSON déjà TOKENISÉ (JS) → réponse brute du fournisseur. */
        @JavascriptInterface
        fun sendToLlm(bodyJson: String): String =
            LlmClient.post(AppPrefs.config(this@MainActivity), bodyJson)
    }

    override fun onBackPressed() {
        if (webView.canGoBack()) webView.goBack() else super.onBackPressed()
    }
}
