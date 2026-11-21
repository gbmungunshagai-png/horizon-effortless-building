//! Network messages between the Effortless client and server, as the original's packets:
//! the client works out every block and its state, the server checks and applies them.
use mod_api::{PayloadCodec, ToServer};
use std::sync::OnceLock;

/// `ServerPlaceBlocksPacket` / `ServerBreakBlocksPacket`.
#[derive(PayloadCodec)]
pub struct PlaceBlocks {
    /// True to break every block, false to place.
    pub breaking: bool,
    /// Ticks to wait before placing (the appear animation), 0 for at once.
    pub delay: i32,
    /// Leave the first block to vanilla (Disabled mode with modifiers).
    pub skip_first: bool,
    /// Block states as `/setblock` writes them, indexed by `blocks`.
    pub palette: Vec<String>,
    /// x, y, z, palette index (-1 when breaking), four values per block, in order.
    pub blocks: Vec<i32>,
}

/// `PerformUndoPacket` / `PerformRedoPacket`.
#[derive(PayloadCodec)]
pub struct History {
    pub redo: bool,
}

pub static PLACE: OnceLock<ToServer<PlaceBlocks>> = OnceLock::new();
pub static HISTORY: OnceLock<ToServer<History>> = OnceLock::new();
