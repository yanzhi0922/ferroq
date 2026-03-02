//! Inbound protocol server implementations.

pub mod onebot_v11;
pub mod onebot_v12;

pub use onebot_v11::OneBotV11Server;
pub use onebot_v12::OneBotV12Server;
