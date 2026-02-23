mod traits;
mod openai_compat;
mod anthropic;
mod ollama;
pub mod xml_dispatcher;

pub use traits::*;
pub use openai_compat::OpenAICompatProvider;
pub use anthropic::AnthropicProvider;
pub use ollama::OllamaProvider;
