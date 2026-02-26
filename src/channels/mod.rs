mod cli;
mod traits;
pub mod slack;

#[cfg(feature = "channel-telegram")]
pub mod telegram;

#[cfg(feature = "channel-discord")]
pub mod discord;

#[cfg(feature = "channel-whatsapp")]
pub mod whatsapp;

pub use cli::CliChannel;
pub use slack::SlackChannel;

#[cfg(feature = "channel-telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "channel-discord")]
pub use discord::DiscordChannel;

#[cfg(feature = "channel-whatsapp")]
pub use whatsapp::WhatsAppChannel;

pub use traits::Channel;
