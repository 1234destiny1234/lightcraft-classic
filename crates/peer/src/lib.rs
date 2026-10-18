//! Finding, pairing with and talking to the app on other devices on the local network.
//!
//! - [`beacon`]: the UDP broadcast that says "the app runs here" (and whether it is looking for a
//!   device to pair with).
//! - [`identity`]: this device (id, name, Ed25519 key) and the devices it is paired with.
//! - [`secure`]: the encrypted, mutually authenticated channel, and the pairing code.
//! - [`service`]: the threads that tie these together, and what Settings ▸ Sync shows.
//!
//! The catalog exchange between paired devices is built on top of this (in the engine).
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
pub mod beacon;
#[cfg(not(target_arch = "wasm32"))]
pub mod identity;
#[cfg(not(target_arch = "wasm32"))]
pub mod secure;
#[cfg(not(target_arch = "wasm32"))]
pub mod service;

#[cfg(not(target_arch = "wasm32"))]
pub use identity::{Identity, Peer, Store};
#[cfg(not(target_arch = "wasm32"))]
pub use secure::Channel;
#[cfg(not(target_arch = "wasm32"))]
pub use service::{HelloHandler, Service, Stage, Status};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Io(String),
    /// A stored file that can't be used.
    Corrupt(String),
    /// The other side said or sent something unusable.
    Protocol(String),
    /// A pairing is already in progress.
    Busy,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(m) | Error::Corrupt(m) | Error::Protocol(m) => f.write_str(m),
            Error::Busy => f.write_str("a pairing is already in progress"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
