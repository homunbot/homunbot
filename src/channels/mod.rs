mod cli;
pub mod slack;
mod traits;

#[cfg(all(feature = "channel-telegram", not(feature = "channel-telegram-frankenstein")))]
pub mod telegram;

#[cfg(feature = "channel-telegram-frankenstein")]
pub mod telegram_frankenstein;

#[cfg(feature = "channel-discord")]
pub mod discord;

#[cfg(feature = "channel-whatsapp")]
pub mod whatsapp;

#[cfg(feature = "channel-email")]
pub mod email;

pub use cli::CliChannel;
pub use slack::SlackChannel;

#[cfg(all(feature = "channel-telegram", not(feature = "channel-telegram-frankenstein")))]
pub use telegram::TelegramChannel;

#[cfg(feature = "channel-telegram-frankenstein")]
pub use telegram_frankenstein::TelegramChannelFrankenstein;

#[cfg(feature = "channel-discord")]
pub use discord::DiscordChannel;

#[cfg(feature = "channel-whatsapp")]
pub use whatsapp::WhatsAppChannel;

#[cfg(feature = "channel-email")]
pub use email::EmailChannel;

pub use traits::Channel;
