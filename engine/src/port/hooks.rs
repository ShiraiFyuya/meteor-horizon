//! The Horizon hook surface: the only file that implements `Hooks`.

use std::sync::Mutex;

use crate::horizon_api_shim::mod_api::{
    ClientTickPostEvent, ClientTickPreEvent, GuiRenderPostEvent, Hooks, Host, InputKeyEvent,
    NativeScreenKind, WorldPrimitivesEvent,
};

use crate::port::env_impl::DrawQueue;
use crate::port::shared::Shared;

/// The mod's single `Hooks` object. Horizon keeps it for the session; the state
/// lives behind mutexes because the hooks take `&self` and are called from the
/// client and server threads alike.
pub struct MeteorHooks {
    shared: Mutex<Shared>,
    gui: Mutex<crate::port::gui::ClickGui>,
    last_screen: Mutex<Option<NativeScreenKind>>,
}

impl Default for MeteorHooks {
    fn default() -> Self {
        Self::new()
    }
}

impl MeteorHooks {
    pub fn new() -> Self {
        let mut shared = Shared::new();
        shared.load();
        Self {
            shared: Mutex::new(shared),
            gui: Mutex::new(crate::port::gui::ClickGui::new()),
            last_screen: Mutex::new(None),
        }
    }

    fn screen_of(kind: NativeScreenKind) -> crate::meteor_core::env::Screen {
        match kind {
            NativeScreenKind::Playing => crate::meteor_core::env::Screen::Playing,
            NativeScreenKind::Chat => crate::meteor_core::env::Screen::Chat,
            NativeScreenKind::Death => crate::meteor_core::env::Screen::Death,
            NativeScreenKind::Other => crate::meteor_core::env::Screen::Other,
        }
    }
}

impl Hooks for MeteorHooks {
    fn client_tick_pre(&self, host: &mut dyn Host, _event: &mut ClientTickPreEvent) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.in_world = host.player().is_some();
            shared.tick(host, None, None);
        }
    }

    fn client_tick_post(&self, host: &mut dyn Host, _event: &mut ClientTickPostEvent) {
        let screen = host
            .client_state()
            .map(|s| s.screen)
            .unwrap_or(NativeScreenKind::Playing);
        let mut previous = self.last_screen.lock().unwrap();
        if let (Ok(mut shared), Ok(mut gui)) = (self.shared.lock(), self.gui.lock()) {
            let opened = *previous != Some(screen) && screen == NativeScreenKind::Other;
            gui.on_screen_change(Self::screen_of(screen), opened);
            gui.tick(&mut shared);
        }
        *previous = Some(screen);
    }

    fn gui_render_post(&self, _host: &dyn Host, event: &mut GuiRenderPostEvent<'_>) {
        // Stage 1 of the Horizon seam: the window's batch is reached through
        // Horizon's own HUD geometry (`HudBatch::quad`), and drawing the ClickGUI
        // and the HUD text on top of it needs the font atlas. Until that adapter
        // exists the port runs with the world primitives only.
        let _ = (event.gui_hidden, event.focused);
    }

    fn world_primitives(&self, _host: &dyn Host, event: &mut WorldPrimitivesEvent<'_>) {
        let mut queue = DrawQueue::new((0.0, 0.0));
        if let Ok(mut shared) = self.shared.lock() {
            shared.render_world(&mut queue);
        }
        queue.flush_3d(event.primitives);
    }

    fn input_key(&self, _host: &mut dyn Host, _event: &mut InputKeyEvent<'_>) {
        // The bind flow needs a GLFW key code; `InputKeyEvent` carries a winit
        // `KeyEvent`, so the translation lands with the GUI adapter (stage 2).
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hooks_constructs() {
        let _ = super::MeteorHooks::new();
    }
}
