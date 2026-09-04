// CLOISON Mobile — client HTTP minimal (POST /chat/completions).
// Le corps reçu est DÉJÀ tokenisé par le WASM côté JS : le fournisseur ne
// voit jamais la PII en clair. Réponse brute renvoyée au JS pour restauration.
package com.cloison.app

import java.io.BufferedReader
import java.io.InputStreamReader
import java.net.HttpURLConnection
import java.net.URL

object LlmClient {

    fun post(cfg: LlmConfig, bodyJson: String): String {
        val base = cfg.baseUrl.trimEnd('/')
        val url = URL("$base/chat/completions")
        val conn = url.openConnection() as HttpURLConnection
        try {
            conn.requestMethod = "POST"
            conn.connectTimeout = 15_000
            conn.readTimeout = 60_000
            conn.setRequestProperty("Content-Type", "application/json")
            conn.setRequestProperty("Authorization", "Bearer ${cfg.apiKey}")
            conn.doOutput = true
            conn.outputStream.use { it.write(bodyJson.toByteArray(Charsets.UTF_8)) }

            val stream = if (conn.responseCode in 200..299) conn.inputStream else conn.errorStream
            val text = stream?.let {
                BufferedReader(InputStreamReader(it, Charsets.UTF_8)).use { r -> r.readText() }
            }.orEmpty()
            if (conn.responseCode !in 200..299) {
                // Renvoie une erreur JSON lisible par le JS.
                return "{\"error\":{\"message\":\"HTTP ${conn.responseCode}: $text\"}}"
            }
            return text
        } finally {
            conn.disconnect()
        }
    }
}
