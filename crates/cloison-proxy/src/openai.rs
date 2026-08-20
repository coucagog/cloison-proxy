//! Types OpenAI aller/retour (serde). Les champs inconnus sont conservés
//! (`#[serde(flatten)] extra`) — pass-through conservateur (invariant I6).
//!
//! Côté retour, la réponse est traitée en `serde_json::Value` (mutation en
//! place des seuls champs connus) puis typée pour la restauration ; un corps
//! qui ne correspond pas à la shape attendue est transmis tel quel.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// POST /v1/chat/completions — corps aller.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatCompletionRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<Vec<ToolSpec>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_choice: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Champs inconnus du client : transmis tels quels au fournisseur.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `content` peut être une chaîne OU un tableau de parts (spec OpenAI).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Content {
    /// Chaîne simple.
    Text(String),
    /// Tableau de parts multimodales.
    Parts(Vec<ContentPart>),
}

/// Part de contenu multimodale : seule la part `"text"` est tokenisée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentPart {
    #[serde(rename = "type")]
    pub type_: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// `image_url`, `audio`, … : intouchés.
    #[serde(flatten)]
    pub rest: Map<String, Value>,
}

/// Message de chat (aller).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ChatMessage {
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

/// Appel d'outil (aller).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub function: FunctionCall,
}

/// Fonction d'un appel d'outil. `arguments` est une chaîne JSON brute :
/// la tokenisation agit sur la chaîne (les sentinelles n'introduisent ni
/// guillemet ni échappement) → le JSON reste syntaxiquement valide (I5).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

/// Déclaration d'outil (tool spec).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    #[serde(rename = "type")]
    pub type_: String,
    pub function: FunctionSpec,
}

/// Spécification de fonction d'un outil.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionSpec {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parameters: Option<Value>,
}

/// POST /v1/completions (legacy).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: Prompt,
    #[serde(default)]
    pub stream: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `prompt` : chaîne unique ou tableau de chaînes.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Prompt {
    Single(String),
    Batch(Vec<String>),
}

/// Réponse non-stream `chat.completion` (shape minimale restaurée par le proxy).
///
/// La réponse amont est d'abord analysée en `Value` ; si elle correspond à
/// cette shape, `content` / `arguments` sont restaurés, tout le reste est
/// préservé à l'identique.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub choices: Vec<Choice>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub usage: Option<Value>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Choix de réponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Choice {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    pub message: ResponseMessage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Message de réponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseMessage {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ResponseToolCall>>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Appel d'outil dans une réponse.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseToolCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub type_: Option<String>,
    pub function: ResponseFunctionCall,
}

/// Fonction d'un appel d'outil de réponse — `arguments` peut être absent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseFunctionCall {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<String>,
}
