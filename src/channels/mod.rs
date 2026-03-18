pub mod capabilities;
mod cli;
pub mod health;
pub mod slack;
mod traits;

#[cfg(feature = "channel-telegram")]
pub mod telegram;

#[cfg(feature = "channel-discord")]
pub mod discord;

#[cfg(feature = "channel-whatsapp")]
pub mod whatsapp;

#[cfg(feature = "channel-email")]
pub mod email;

pub use cli::CliChannel;
pub use slack::SlackChannel;

#[cfg(feature = "channel-telegram")]
pub use telegram::TelegramChannel;

#[cfg(feature = "channel-discord")]
pub use discord::DiscordChannel;

#[cfg(feature = "channel-whatsapp")]
pub use whatsapp::WhatsAppChannel;

#[cfg(feature = "channel-email")]
pub use email::EmailChannel;

pub use capabilities::{capabilities_for, ChannelCapabilities};
pub use health::ChannelHealthTracker;
pub use traits::Channel;
