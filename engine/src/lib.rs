//! The bundle's `[api]` package: what `horizon-launcher native-build` links
//! into the game and calls `hooks()` on.
//!
//! Everything substantial lives in `meteor-mod`; this file only re-exports it and
//! pins the trait object type so a bundle build fails loudly if Horizon's
//! `Hooks` trait and the port drift apart.

pub use meteor_mod::*;

/// Horizon calls this once per process and keeps the object for the session.
pub fn hooks() -> Box<dyn horizon_api_shim::mod_api::Hooks> {
    meteor_mod::hooks()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hooks_links() {
        let _ = super::hooks();
    }
}
