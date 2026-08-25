// CLOISON Mobile — écran de configuration (endpoint LLM + clé + modèle).
package com.cloison.app

import android.os.Bundle
import android.widget.Button
import android.widget.EditText
import androidx.appcompat.app.AppCompatActivity

class SettingsActivity : AppCompatActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContentView(R.layout.activity_settings)

        val baseUrl = findViewById<EditText>(R.id.base_url)
        val apiKey = findViewById<EditText>(R.id.api_key)
        val model = findViewById<EditText>(R.id.model)
        val save = findViewById<Button>(R.id.save)

        val c = AppPrefs.config(this)
        baseUrl.setText(c.baseUrl)
        apiKey.setText(c.apiKey)
        model.setText(c.model)

        save.setOnClickListener {
            AppPrefs.save(
                this,
                LlmConfig(
                    baseUrl = baseUrl.text.toString().trim(),
                    apiKey = apiKey.text.toString().trim(),
                    model = model.text.toString().trim().ifEmpty { "openai/gpt-4o-mini" },
                ),
            )
            finish()
        }
    }
}
