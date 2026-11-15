//! The one seam between the port and Horizon's generated API.
//!
//! `meteor-mod` imports Horizon's API as `horizon_api_shim::mod_api`. Locally that
//! name is the hand-written `crates/horizon-api-shim`; in a bundle it is this
//! crate, which re-exports the real thing. Keeping the port's imports behind one
//! name is what lets the same module code build against the shim (fast, no
//! Horizon tree) and against `horizon_client` (the bundle).
//!
//! The generated API is Horizon's, not the port's: `Hooks`, `Host` and the
//! `-Event` structs come from `native-hooks.json` and their exact shapes are
//! pinned per commit. Where the port needs a name the generated trait spells
//! differently, the adapter belongs here, as a `pub trait` extension the port's
//! `env_impl.rs` calls - never inside a module.

pub use horizon_client::mod_api::*;

// The port's own `Host` additions, expressed in terms of Horizon's helpers.
// Horizon's `Host` already offers `gui_size`, `gui_fill`, `gui_text`,
// `client_entities`, `player`, `block`, `set_block`, `send_chat`, `tell`,
// `set_rotation` and the packet helpers the port's `env_impl.rs` uses; the ones
// below are the few calls that need a small composition.
pub trait HostExt {
    /// The GUI size in GUI pixels, `native_physical_window_size` divided by the
    /// GUI scale when the generated API only reports physical pixels.
    fn meteor_gui_size(&self) -> (f64, f64);
}

impl<T: ?Sized + Host> HostExt for T {
    fn meteor_gui_size(&self) -> (f64, f64) {
        let s = self.gui_size();
        (s.width, s.height)
    }
}
