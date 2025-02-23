pub mod gui_types;
pub mod helpers;
pub mod standardized_types;
pub mod messages;
pub mod strategies;
pub(crate) mod tests;

pub mod apis;
pub mod product_maps;

/// The `stream_name` is just the u16 port number of the strategy which the server is connecting to,
/// it is used to link the streaming port to a async port, you just need to know it represents a single strategy instance.
/// This allows you to create logic per connecting strategy, so you can drop objects from memory when a strategy goes offline.
pub type StreamName = u16;
pub mod database;
pub mod server_launch_options;
