mod cli;
pub mod discord;
pub mod telegram;
pub mod traits;
pub mod whatsapp;

pub use cli::CliChannel;
pub use discord::DiscordChannel;
pub use telegram::TelegramChannel;
pub use traits::Channel;
pub use whatsapp::WhatsAppChannel;
