//! The Horizon hook surface: the only file that implements `Hooks`.
//!
//! Each method does the same three things: pull the game's state out of `Host`
//! into the port's own types, run the matching dispatch on [`Shared`], and (for
//! the drawing hooks) flush the commands the modules queued into Horizon's batch
//! or primitive list.

use std::sync::Mutex;

use horizon_api_shim::mod_api::{
    Flow, GuiRenderEvent, Hand as ShimHand, Hooks, Host, NativeScreenKind, WorldPrimitivesEvent,
};
use meteor_core::env::{
    EntityInfo, GameMode, ItemStackInfo, PacketInfo, PlayerInfo, Screen, WorldInfo,
};
use meteor_core::math::Vec3d;

use crate::env_impl::DrawQueue;
use crate::shared::Shared;

pub struct MeteorHooks {
    shared: Mutex<Shared>,
    /// The ClickGUI's own state (open windows, scroll, search text).
    gui: Mutex<crate::gui::ClickGui>,
    /// The last screen the client reported, so the GUI opens on Escape.
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
            gui: Mutex::new(crate::gui::ClickGui::new()),
            last_screen: Mutex::new(None),
        }
    }

    /// Reads the game into the port's `PlayerInfo`.
    fn player_info(host: &dyn Host) -> Option<PlayerInfo> {
        let p = host.player()?;
        Some(PlayerInfo {
            name: p.name.clone(),
            uuid: uuid_bytes(&p.uuid),
            pos: Vec3d::new(p.position[0], p.position[1], p.position[2]),
            prev_pos: Vec3d::new(p.position[0], p.position[1], p.position[2]),
            yaw: p.yaw,
            pitch: p.pitch,
            head_yaw: p.yaw,
            on_ground: true,
            dimension: p.dimension.clone(),
            health: p.health,
            max_health: 20.0,
            food: p.food,
            max_air: 300,
            air: 300,
            selected_slot: p.selected_slot,
            eye_height: 1.62,
            game_mode: GameMode::Survival,
            ..Default::default()
        })
    }

    fn world_info(host: &dyn Host) -> Option<WorldInfo> {
        let (game_time, day_time) = host.clock()?;
        let dimension = host
            .player()
            .map(|p| p.dimension)
            .unwrap_or_else(|| "minecraft:overworld".to_string());
        Some(WorldInfo {
            dimension,
            game_time,
            day_time,
            min_y: -64,
            height: 384,
            difficulty: "normal".into(),
            ..Default::default()
        })
    }

    fn entity_infos(host: &dyn Host) -> Vec<EntityInfo> {
        host.client_entities()
            .into_iter()
            .map(|e| EntityInfo {
                id: e.id,
                kind: e.kind,
                pos: Vec3d::new(e.position[0], e.position[1], e.position[2]),
                prev_pos: Vec3d::new(
                    e.previous_position[0],
                    e.previous_position[1],
                    e.previous_position[2],
                ),
                yaw: e.yaw,
                head_yaw: e.head_yaw,
                health: e.health,
                max_health: e.max_health,
                alive: e.alive,
                invisible: e.invisible,
                baby: e.baby,
                is_player: e.player_name.is_some(),
                is_mob: e.player_name.is_none(),
                name: e.name.or(e.player_name.clone()),
                ping: e.ping,
                ..Default::default()
            })
            .collect()
    }

    fn screen_of(kind: NativeScreenKind) -> Screen {
        match kind {
            NativeScreenKind::Playing => Screen::Playing,
            NativeScreenKind::Chat => Screen::Chat,
            NativeScreenKind::Death => Screen::Death,
            NativeScreenKind::Other => Screen::Other,
        }
    }
}

fn uuid_bytes(text: &str) -> [u8; 16] {
    let mut out = [0u8; 16];
    let hex: Vec<u8> = text
        .chars()
        .filter(|c| c.is_ascii_hexdigit())
        .map(|c| c.to_digit(16).unwrap_or(0) as u8)
        .collect();
    for (i, b) in out.iter_mut().enumerate() {
        let hi = hex.get(i * 2).copied().unwrap_or(0);
        let lo = hex.get(i * 2 + 1).copied().unwrap_or(0);
        *b = (hi << 4) | lo;
    }
    out
}

impl Hooks for MeteorHooks {
    fn client_tick_pre(&self, host: &mut dyn Host) {
        let player = Self::player_info(host);
        let world = Self::world_info(host);
        let screen = host
            .client_state()
            .map(|s| s.screen)
            .unwrap_or(NativeScreenKind::Playing);
        if let Ok(mut shared) = self.shared.lock() {
            shared.in_world = player.is_some();
            if screen != NativeScreenKind::Playing && shared.in_world {
                // A screen opening releases the bind-toggled modules, as
                // `Modules.onOpenScreen` does.
                let feedback = shared.config.chat_feedback;
                let _ = feedback;
            }
            shared.tick(host, player, world);
        }
    }

    fn client_tick_post(&self, host: &mut dyn Host) {
        // The GUI's own tick: the ClickGUI opens on the bind key and closes on
        // Escape, and its windows keep their drag state between frames.
        let screen = host
            .client_state()
            .map(|s| s.screen)
            .unwrap_or(NativeScreenKind::Playing);
        let mut previous = self.last_screen.lock().unwrap();
        if let Ok(mut shared) = self.shared.lock() {
            if let Ok(mut gui) = self.gui.lock() {
                let opened = *previous != Some(screen) && screen == NativeScreenKind::Other;
                gui.on_screen_change(Self::screen_of(screen), opened);
                gui.tick(&mut shared);
            }
        }
        *previous = Some(screen);
    }

    fn world_primitives(&self, _host: &mut dyn Host, event: &mut WorldPrimitivesEvent<'_>) {
        let mut queue = DrawQueue::new((1920.0, 1080.0));
        if let Ok(mut shared) = self.shared.lock() {
            shared.render_world(&mut queue);
            queue.flush_3d(event.primitives);
        }
        let _ = event.partial;
    }

    fn gui_render_post(&self, host: &mut dyn Host, event: &mut GuiRenderEvent<'_>) {
        let size = event.batch.size();
        let mut queue = DrawQueue::new((size.width, size.height));

        // 1. the HUD, then the modules' own 2D.
        if let Ok(mut shared) = self.shared.lock() {
            {
                let snapshot = shared.snapshot.clone();
                let mut pending = Vec::new();
                let mut env = crate::env_impl::HorizonEnv::new_headless(
                    &snapshot,
                    &mut pending,
                    shared.ticks,
                );
                env.set_player(Self::player_info(host));
                shared.hud.render(&mut env, &mut queue);
                shared.modules.render_2d(&mut env, &mut queue);
            }
            // 2. the ClickGUI, drawn over everything.
            if let Ok(mut gui) = self.gui.lock() {
                if gui.is_open() {
                    gui.render(&mut shared, &mut queue, event.gui_hidden);
                }
            }
        }
        queue.flush_2d(event.batch);
        let _ = event.focused;
    }

    fn input_key(&self, host: &mut dyn Host, key: i32, pressed: bool, modifiers: u8) {
        if let Ok(mut gui) = self.gui.lock() {
            if let Ok(mut shared) = self.shared.lock() {
                if gui.on_key(&mut shared, key, pressed, modifiers) {
                    return;
                }
            }
        }
        if pressed {
            if let Ok(mut shared) = self.shared.lock() {
                shared.on_input(host, true, key, pressed, modifiers);
            }
        }
    }

    fn screen_char_typed_pre(
        &self,
        _host: &mut dyn Host,
        character: char,
        _modifiers: u8,
    ) -> Flow {
        if let Ok(mut gui) = self.gui.lock() {
            if let Ok(mut shared) = self.shared.lock() {
                gui.on_char(&mut shared, character);
            }
        }
        Flow::Continue
    }

    fn mouse_moved(&self, host: &mut dyn Host, x: f64, y: f64) {
        let _ = host;
        if let Ok(mut gui) = self.gui.lock() {
            gui.on_mouse_move(x, y);
        }
    }

    fn mouse_button(&self, _host: &mut dyn Host, button: i32, pressed: bool, _modifiers: u8) -> bool {
        let mut consumed = false;
        if let Ok(mut gui) = self.gui.lock() {
            if gui.is_open() {
                if let Ok(mut shared) = self.shared.lock() {
                    consumed = gui.on_mouse(&mut shared, button, pressed);
                }
            }
        }
        if consumed {
            return true;
        }
        // A module bind on a mouse button still works over the world.
        if pressed {
            if let Ok(mut shared) = self.shared.lock() {
                let mut pending = Vec::new();
                let snapshot = shared.snapshot.clone();
                let ticks = shared.ticks;
                let feedback = shared.config.chat_feedback;
                let mut env =
                    crate::env_impl::HorizonEnv::new_headless(&snapshot, &mut pending, ticks);
                shared
                    .modules
                    .on_input(&mut env, false, button, true, 0, feedback);
            }
        }
        false
    }

    fn mouse_scrolling(&self, _host: &mut dyn Host, _x: f64, y: f64) -> bool {
        // The ClickGUI scrolls the window under the pointer; the mod applies it as
        // a window scroll offset (per-category, so it never moves the world).
        if let Ok(mut gui) = self.gui.lock() {
            if gui.is_open() {
                gui.scroll(y);
                return true;
            }
        }
        false
    }

    fn chat_sent(&self, host: &mut dyn Host, message: &str) -> bool {
        if let Ok(mut shared) = self.shared.lock() {
            return shared.on_chat(host, message);
        }
        false
    }

    fn client_packet(&self, _host: &mut dyn Host, name: &str) -> Flow {
        let mut packet = PacketInfo {
            name: name.to_string(),
            clientbound: true,
            fields: Vec::new(),
        };
        if let Ok(mut shared) = self.shared.lock() {
            let snapshot = shared.snapshot.clone();
            let mut pending = Vec::new();
            let mut env =
                crate::env_impl::HorizonEnv::new_headless(&snapshot, &mut pending, shared.ticks);
            if shared.on_packet(&mut env, &mut packet, true) {
                Flow::Cancel
            } else {
                Flow::Continue
            }
        } else {
            Flow::Continue
        }
    }

    fn movement_input(&self, _host: &mut dyn Host, forward: f32, strafe: f32) -> (f32, f32) {
        (forward, strafe)
    }

    fn compute_fov(&self, _host: &mut dyn Host, degrees: f32) -> f32 {
        degrees
    }

    fn health_hud(&self, _host: &mut dyn Host) -> bool {
        false
    }
}

/// The `Hand` the shim and the port both use.
pub fn shim_hand(hand: meteor_core::env::Hand) -> ShimHand {
    match hand {
        meteor_core::env::Hand::Main => ShimHand::Main,
        meteor_core::env::Hand::Off => ShimHand::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uuid_bytes_reads_hex() {
        let b = uuid_bytes("00112233445566778899aabbccddeeff");
        assert_eq!(b[0], 0x00);
        assert_eq!(b[1], 0x11);
        assert_eq!(b[15], 0xFF);
    }

    #[test]
    fn screen_mapping() {
        assert_eq!(MeteorHooks::screen_of(NativeScreenKind::Chat), Screen::Chat);
        assert_eq!(
            MeteorHooks::screen_of(NativeScreenKind::Other),
            Screen::Other
        );
    }
}
