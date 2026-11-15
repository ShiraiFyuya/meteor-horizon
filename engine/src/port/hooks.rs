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

    fn gui_render_post(&self, host: &dyn Host, event: &mut GuiRenderPostEvent<'_>) {
        let mut queue = DrawQueue::new((event.batch.width as f64, event.batch.height as f64));
        if let (Ok(mut shared), Ok(mut gui)) = (self.shared.lock(), self.gui.lock()) {
            let snapshot = shared.snapshot.clone();
            let mut pending = Vec::new();
            let ticks = shared.ticks;
            let mut env =
                crate::port::env_impl::HorizonEnv::new_headless(&snapshot, &mut pending, ticks);
            shared.hud.render(&mut env, &mut queue);
            shared.modules.render_2d(&mut env, &mut queue);
            if gui.is_open() {
                gui.render(&mut shared, &mut queue, event.gui_hidden);
            }
        }
        queue.flush_to_hud(host, event.batch);
    }

    fn world_primitives(&self, _host: &dyn Host, event: &mut WorldPrimitivesEvent<'_>) {
        let mut queue = DrawQueue::new((0.0, 0.0));
        if let Ok(mut shared) = self.shared.lock() {
            shared.render_world(&mut queue);
        }
        queue.flush_3d(event.primitives);
    }

    fn input_key(&self, host: &mut dyn Host, event: &mut InputKeyEvent<'_>) {
        // The generated event carries winit's `KeyEvent`; the port maps the
        // physical key by its debug name so it does not have to depend on winit.
        let code = format!("{:?}", event.event.physical_key);
        let pressed = format!("{:?}", event.event.state).contains("Pressed");
        if !pressed {
            return;
        }
        let Ok(mut gui) = self.gui.lock() else { return };
        if code.contains("ShiftRight") {
            gui.toggle();
            return;
        }
        if !gui.is_open() {
            return;
        }
        let Ok(mut shared) = self.shared.lock() else { return };
        if code.contains("Escape") {
            gui.on_key(&mut shared, 256, true, 0);
        } else if code.contains("ArrowUp") {
            gui.nav(&shared, -1);
        } else if code.contains("ArrowDown") {
            gui.nav(&shared, 1);
        } else if code.contains("Enter") {
            gui.activate_selected(&mut shared);
        } else if code.contains("Tab") {
            gui.toggle_settings_of_selected();
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hooks_constructs() {
        let _ = super::MeteorHooks::new();
    }
}
