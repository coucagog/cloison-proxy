// CLOISON Mobile — configuration de l'app (endpoint LLM + clé + modèle).
// Stockée dans les préférences locales (votre clé amont, jamais transmise
// ailleurs que vers le fournisseur configuré — charte §7.2).
package com.cloison.app

import android.content.Context
import org.json.JSONObject

data class LlmConfig(
    val baseUrl: String,
    val apiKey: String,
    val model: String,
) {
    fun toJson(): String = JSONObject()
        .put("baseUrl", baseUrl)
        .put("apiKey", apiKey)
        .put("model", model)
        .toString()
}

object AppPrefs {
    private const val PREFS = "cloison_prefs"

    fun config(ctx: Context): LlmConfig {
        val p = ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
        return LlmConfig(
            baseUrl = p.getString("base_url", "").orEmpty(),
            apiKey = p.getString("api_key", "").orEmpty(),
            model = p.getString("model", "openai/gpt-4o-mini").orEmpty(),
        )
    }

    fun save(ctx: Context, c: LlmConfig) {
        ctx.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
            .edit()
            .putString("base_url", c.baseUrl)
            .putString("api_key", c.apiKey)
            .putString("model", c.model)
            .apply()
    }
}
