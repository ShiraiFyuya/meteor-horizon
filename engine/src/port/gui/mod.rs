//! The ClickGUI.
//!
//! Meteor's ClickGUI is a row of per-category windows; each module is a button
//! that fills with `moduleBackground` while it is on (or hovered) and grows a
//! 2-px accent bar on its left edge, and right-clicking a button opens its
//! settings. The layout maths here is the Java theme's:
//! `pad = scale(4)`, `height = pad + textHeight + pad`, the window header painted
//! with `accentColor` and the body with `backgroundColor`.

use std::collections::BTreeMap;

use crate::meteor_core::category::Category;
use crate::meteor_core::color::Color;
use crate::meteor_core::env::Screen;
use crate::meteor_core::setting::Value;
use crate::meteor_core::util::{name_to_title, search_score};

use crate::port::env_impl::DrawQueue;
use crate::port::shared::Shared;

struct Window {
    category: Category,
    x: f64,
    y: f64,
    width: f64,
    collapsed: bool,
    scroll: f64,
}

#[derive(Clone, Copy, Default)]
struct ButtonAnim {
    background: f64,
    bar: f64,
}

pub struct ClickGui {
    open: bool,
    windows: Vec<Window>,
    search: String,
    search_focused: bool,
    mouse: (f64, f64),
    dragging: Option<(usize, f64, f64)>,
    anims: BTreeMap<&'static str, ButtonAnim>,
    expanded: BTreeMap<String, bool>,
    laid_out: bool,
    caret: f64,
    /// The row the keyboard is on (`ClickGui`'s key navigation).
    selected: Option<&'static str>,
    /// `HudEditorScreen`: the HUD elements are draggable while this is on.
    hud_editor: bool,
    /// The element being dragged, by index into `Hud::elements`.
    dragging_hud: Option<usize>,
    /// Mouse position at the previous frame, for the HUD editor's deltas.
    last_drag_mouse: (f64, f64),
}

impl Default for ClickGui {
    fn default() -> Self {
        Self::new()
    }
}

impl ClickGui {
    pub fn new() -> Self {
        Self {
            open: false,
            windows: Category::ALL
                .iter()
                .map(|c| Window {
                    category: *c,
                    x: 0.0,
                    y: 0.0,
                    width: 200.0,
                    collapsed: false,
                    scroll: 0.0,
                })
                .collect(),
            search: String::new(),
            search_focused: false,
            mouse: (0.0, 0.0),
            dragging: None,
            anims: BTreeMap::new(),
            expanded: BTreeMap::new(),
            laid_out: false,
            caret: 0.0,
            selected: None,
            hud_editor: false,
            dragging_hud: None,
            last_drag_mouse: (0.0, 0.0),
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) {
        self.open = true;
        self.laid_out = false;
        self.search_focused = true;
    }

    pub fn close(&mut self) {
        self.open = false;
        self.hud_editor = false;
        self.dragging = None;
        self.search_focused = false;
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close()
        } else {
            self.open()
        }
    }

    /// `WidgetScreen`'s lifecycle hook.
    pub fn on_screen_change(&mut self, screen: Screen, _opened: bool) {
        // Another screen taking over closes the GUI, as `WidgetScreen` does; the
        // mod calls this when Horizon reports a different screen id.
        if self.open && screen == Screen::Death {
            self.close();
        }
    }

    /// Advances the two animation progress values (`WMeteorModule.onRender`).
    pub fn tick(&mut self, shared: &mut Shared) {
        let delta = 1.0 / 60.0;
        self.caret = (self.caret + delta) % 1.0;
        if !self.open {
            return;
        }
        // HUD editor: move the element under the pointer as it drags.
        if let Some(i) = self.dragging_hud {
            let dx = self.mouse.0 - self.last_drag_mouse.0;
            let dy = self.mouse.1 - self.last_drag_mouse.1;
            if let Some(element) = shared.hud.elements.get_mut(i) {
                element.move_by(dx.round() as i32, dy.round() as i32);
            }
        }
        self.last_drag_mouse = self.mouse;
        let hovered_name = self.hovered_module(shared);
        let names: Vec<&'static str> = shared.modules.all().map(|m| m.base().name).collect();
        for name in names {
            let active = shared.modules.is_active(name);
            let hovered = hovered_name == Some(name);
            let a = self.anims.entry(name).or_default();
            a.background = approach(a.background, if active || hovered { 1.0 } else { 0.0 }, delta * 4.0);
            a.bar = approach(a.bar, if active { 1.0 } else { 0.0 }, delta * 6.0);
        }
    }

    /// Moves the keyboard selection by `delta` rows over every window in order.
    pub fn nav(&mut self, shared: &Shared, delta: i32) {
        let mut rows: Vec<&'static str> = Vec::new();
        for category in Category::ALL {
            rows.extend(self.filtered(shared, category));
        }
        if rows.is_empty() {
            self.selected = None;
            return;
        }
        let current = self
            .selected
            .and_then(|name| rows.iter().position(|row| *row == name));
        let next = match current {
            None if delta >= 0 => 0,
            None => rows.len() - 1,
            Some(i) => (i as i32 + delta).rem_euclid(rows.len() as i32) as usize,
        };
        self.selected = Some(rows[next]);
    }

    /// Toggles the selected row (`Enter`).
    pub fn activate_selected(&mut self, shared: &mut Shared) {
        let Some(name) = self.selected else { return };
        let feedback = shared.config.chat_feedback;
        let mut env = crate::port::env_impl::HeadlessEnv::new();
        shared
            .modules
            .toggle_with_message_by_name(name, &mut env, feedback);
    }

    /// Opens/closes the settings of the selected row (`Tab`).
    pub fn toggle_settings_of_selected(&mut self) {
        let Some(name) = self.selected else { return };
        let open = self.expanded.get(name).copied().unwrap_or(false);
        self.expanded.insert(name.to_string(), !open);
    }

    fn hovered_module(&self, shared: &Shared) -> Option<&'static str> {
        let (mx, my) = self.mouse;
        for (name, (x, y, w, h), _panel) in self.row_rects(shared).into_iter().filter(|r| !r.2) {
            let (x2, y2, w2, h2) = (x, y, w, h);
            if mx >= x2 && mx <= x2 + w2 && my >= y2 && my <= y2 + h2 {
                return Some(name);
            }
        }
        None
    }

    /// Keyboard: Right Shift opens, Escape closes, typing filters.
    pub fn on_key(&mut self, shared: &mut Shared, key: i32, pressed: bool, _modifiers: u8) -> bool {
        let _ = shared;
        if !self.open {
            if pressed && key == 344 {
                self.open();
                return true;
            }
            return false;
        }
        if !pressed {
            return false;
        }
        match key {
            72 => {
                // `H` toggles the HUD editor, as `HudEditorScreen`'s bind does.
                self.hud_editor = !self.hud_editor;
                true
            }
            256 => {
                if self.hud_editor {
                    self.hud_editor = false;
                    return true;
                }
                if self.expanded.values().any(|v| *v) {
                    self.expanded.clear();
                } else {
                    self.close();
                }
                true
            }
            259 => {
                self.search.pop();
                true
            }
            257 => {
                self.search_focused = false;
                true
            }
            _ => false,
        }
    }

    pub fn on_char(&mut self, _shared: &mut Shared, c: char) {
        if self.open && self.search_focused && !c.is_control() && self.search.chars().count() < 32 {
            self.search.push(c);
        }
    }

    pub fn on_mouse_move(&mut self, x: f64, y: f64) {
        self.mouse = (x, y);
        if let Some((i, last_x, last_y)) = self.dragging {
            let dx = x - last_x;
            let dy = y - last_y;
            if let Some(w) = self.windows.get_mut(i) {
                w.x += dx;
                w.y += dy;
            }
            self.dragging = Some((i, x, y));
        }
    }

    /// Wheel over a window scrolls its module list (`WView.onMouseScrolled`).
    pub fn scroll(&mut self, amount: f64) {
        if !self.open {
            return;
        }
        let (mx, my) = self.mouse;
        for w in self.windows.iter_mut() {
            if mx >= w.x && mx <= w.x + w.width && my >= w.y {
                w.scroll = (w.scroll - amount * 12.0).max(0.0);
                break;
            }
        }
    }

    /// A mouse button; returns true when the GUI consumed it.
    pub fn on_mouse(&mut self, shared: &mut Shared, button: i32, pressed: bool) -> bool {
        if !self.open {
            return false;
        }
        let (mx, my) = self.mouse;
        if !pressed {
            self.dragging = None;
            self.dragging_hud = None;
            return true;
        }
        // Window headers.
        if self.hud_editor {
            // `HudEditorScreen`: a click picks up the element under the pointer.
            self.dragging_hud = self.hud_element_at(shared, mx, my);
            self.last_drag_mouse = self.mouse;
            if self.dragging_hud.is_some() {
                return true;
            }
        }
        for i in 0..self.windows.len() {
            let (x, y, w) = {
                let win = &self.windows[i];
                (win.x, win.y, win.width)
            };
            if hit(mx, my, x, y, w, header_height(shared)) {
                if button == 1 {
                    self.windows[i].collapsed = !self.windows[i].collapsed;
                } else {
                    self.dragging = Some((i, mx, my));
                }
                return true;
            }
        }
        // Module rows.
        for (name, (x, y, w, h), _panel) in self.row_rects(shared).into_iter().filter(|r| !r.2) {
            if hit(mx, my, x, y, w, h) {
                if button == 0 {
                    let feedback = shared.config.chat_feedback;
                    let mut env = crate::port::env_impl::HeadlessEnv::new();
                    shared
                        .modules
                        .toggle_with_message_by_name(name, &mut env, feedback);
                } else if button == 1 {
                    let open = self.expanded.get(name).copied().unwrap_or(false);
                    self.expanded.insert(name.to_string(), !open);
                }
                return true;
            }
        }
        // Settings rows.
        for (module, settings) in self.setting_rects(shared) {
            for (setting, sx, sy, sw, sh) in settings {
                if !hit(mx, my, sx, sy, sw, sh) {
                    continue;
                }
                self.click_setting(shared, module, &setting, mx);
                return true;
            }
        }
        // Search box focuses on click.
        if let Some((x, y, w, h)) = self.search_box(shared) {
            if hit(mx, my, x, y, w, h) {
                self.search_focused = true;
            }
        }
        true
    }

    /// `.setting`'s GUI equivalent: toggle a bool, drag a slider, cycle an enum.
    fn click_setting(&mut self, shared: &mut Shared, module: &str, setting: &str, mx: f64) {
        let s = shared.theme.scale.get();
        let Some(base) = shared.modules.base(module) else {
            return;
        };
        let Some(entry) = base.settings.get(setting) else {
            return;
        };
        match &entry.value {
            Value::Bool(cell) => {
                let v = !cell.load(std::sync::atomic::Ordering::Relaxed);
                entry.parse(if v { "true" } else { "false" });
            }
            Value::Int { min, max, .. } => {
                let (lo, hi) = (*min, *max);
                let (bx, bw) = self.slider_rect(shared, module);
                let t = ((mx - bx) / bw).clamp(0.0, 1.0);
                let v = lo as f64 + (hi - lo) as f64 * t;
                entry.parse(&format!("{}", v.round() as i32));
            }
            Value::Double { min, max, .. } => {
                let (lo, hi) = (*min, *max);
                let (bx, bw) = self.slider_rect(shared, module);
                let t = ((mx - bx) / bw).clamp(0.0, 1.0);
                let v = lo + (hi - lo) * t;
                entry.parse(&format!("{v:.3}"));
            }
            Value::Enum { cell, names } => {
                if names.is_empty() {
                    return;
                }
                let cur = cell.load(std::sync::atomic::Ordering::Relaxed) as usize;
                let next = (cur + 1) % names.len();
                entry.parse(names[next]);
            }
            _ => {}
        }
        let _ = s;
    }

    /// The HUD element under the pointer, in `Hud::elements` order (last drawn
    /// wins, as `HudEditorScreen` picks the top one).
    fn hud_element_at(&self, shared: &Shared, mx: f64, my: f64) -> Option<usize> {
        let mut found = None;
        for (i, e) in shared.hud.elements.iter().enumerate() {
            if !e.is_active() {
                continue;
            }
            let (x, y) = e.render_pos();
            if hit(mx, my, x as f64, y as f64, e.width() as f64, e.height() as f64) {
                found = Some(i);
            }
        }
        found
    }

    fn slider_rect(&self, shared: &Shared, module: &str) -> (f64, f64) {        let s = shared.theme.scale.get();
        for w in &self.windows {
            if shared.group(w.category).iter().any(|n| *n == module) {
                return (w.x + w.width - 74.0 * s, 60.0 * s);
            }
        }
        (0.0, 60.0 * s)
    }

    fn search_box(&self, shared: &Shared) -> Option<(f64, f64, f64, f64)> {
        let s = shared.theme.scale.get();
        Some((8.0 * s, 4.0 * s, 200.0 * s, 16.0 * s))
    }

    /// `(name, (x, y, w, h), is_settings_panel)` for every row, in draw order.
    fn row_rects(&self, shared: &Shared) -> Vec<(&'static str, (f64, f64, f64, f64), bool)> {
        let mut out = Vec::new();
        let s = shared.theme.scale.get();
        let header = header_height(shared);
        let row_h = module_height(shared);
        for w in &self.windows {
            if w.collapsed {
                continue;
            }
            let mut y = w.y + header + 4.0 * s - w.scroll;
            for name in self.filtered(shared, w.category) {
                out.push((name, (w.x, y, w.width, row_h), false));
                y += row_h;
                if self.expanded.get(name).copied().unwrap_or(false) {
                    let rows = shared.setting_rows(name);
                    let panel_h = rows.len() as f64 * setting_row_height(shared);
                    out.push((name, (w.x, y, w.width, panel_h), true));
                    y += panel_h;
                }
            }
        }
        out
    }

    /// `(module, [(setting, x, y, w, h)])` for the settings that are open.
    fn setting_rects(&self, shared: &Shared) -> Vec<(&'static str, Vec<(String, f64, f64, f64, f64)>)> {
        let s = shared.theme.scale.get();
        let row_h = setting_row_height(shared);
        let mut out = Vec::new();
        for w in &self.windows {
            if w.collapsed {
                continue;
            }
            let mut y = w.y + header_height(shared) + 4.0 * s - w.scroll;
            for name in self.filtered(shared, w.category) {
                y += module_height(shared);
                if !self.expanded.get(name).copied().unwrap_or(false) {
                    continue;
                }
                let mut rows = Vec::new();
                for (setting, _display) in shared.setting_rows(name) {
                    rows.push((
                        setting,
                        w.x + 2.0 * s,
                        y,
                        w.width - 4.0 * s,
                        row_h,
                    ));
                    y += row_h;
                }
                out.push((name, rows));
            }
        }
        out
    }

    fn filtered(&self, shared: &Shared, category: Category) -> Vec<&'static str> {
        let all = shared.group(category);
        if self.search.is_empty() {
            return all;
        }
        let mut hits: Vec<(usize, &'static str)> = all
            .into_iter()
            .filter_map(|n| {
                let score = search_score(&shared.title_of(n), &self.search);
                if score <= 3 {
                    Some((score, n))
                } else {
                    None
                }
            })
            .collect();
        hits.sort_by_key(|(score, name)| (*score, *name));
        hits.into_iter().map(|(_, n)| n).collect()
    }

    fn layout(&mut self, shared: &Shared) {
        let s = shared.theme.scale.get();
        let pad = 8.0 * s;
        let width = 200.0 * s;
        let gap = 6.0 * s;
        let mut x = pad;
        let mut y = pad + 20.0 * s;
        let screen_w = 1920.0;
        for w in self.windows.iter_mut() {
            if x + width > screen_w - pad {
                x = pad;
                y += 240.0 * s;
            }
            w.x = x;
            w.y = y;
            w.width = width;
            x += width + gap;
        }
    }

    fn window_body_height(&self, shared: &Shared, category: Category) -> f64 {
        let s = shared.theme.scale.get();
        let mut h = 4.0 * s;
        for name in self.filtered(shared, category) {
            h += module_height(shared);
            if self.expanded.get(name).copied().unwrap_or(false) {
                h += shared.setting_rows(name).len() as f64 * setting_row_height(shared);
            }
        }
        h + 4.0 * s
    }

    /// Draws the GUI: dim layer, windows, module buttons, settings, search box.
    pub fn render(&mut self, shared: &mut Shared, q: &mut DrawQueue, _hud_hidden: bool) {
        if !self.laid_out {
            self.layout(shared);
            self.laid_out = true;
        }
        let s = shared.theme.scale.get();
        let (gw, gh) = q.gui_size;
        q.fill(0.0, 0.0, gw, gh, Color::new(0, 0, 0, 120).argb());

        for i in 0..self.windows.len() {
            let (x, y, w, collapsed) = {
                let win = &self.windows[i];
                (win.x, win.y, win.width, win.collapsed)
            };
            let header = header_height(shared);
            if !collapsed {
                let body = self.window_body_height(shared, self.windows[i].category);
                q.fill(
                    x,
                    y + header,
                    w,
                    body,
                    shared.theme.background_color(false, false).argb(),
                );
            }
            q.fill(x, y, w, header, shared.theme.accent().argb());
            let title = self.windows[i].category.name();
            let tw = shared.theme.text_width(title);
            q.text(
                x + w / 2.0 - tw / 2.0,
                y + header / 2.0 - shared.theme.text_height() / 2.0,
                title,
                shared.theme.text_color().argb(),
                1.0,
            );
            if collapsed {
                continue;
            }
            self.render_rows(shared, i, q);
        }

        if let Some((x, y, w, h)) = self.search_box(shared) {
            q.fill(x, y, w, h, shared.theme.background_color(false, false).argb());
            let (text, color) = if self.search.is_empty() {
                ("Search...".to_string(), shared.theme.text_secondary().argb())
            } else {
                (self.search.clone(), shared.theme.text_color().argb())
            };
            q.text(x + 4.0 * s, y + h / 2.0 - shared.theme.text_height() / 2.0, &text, color, 1.0);
            if self.search_focused && self.caret < 0.5 {
                let caret_x = x + 4.0 * s + shared.theme.text_width(&self.search) + 1.0;
                q.fill(caret_x, y + 3.0 * s, 1.0, h - 6.0 * s, shared.theme.text_color().argb());
            }
        }

        // `HudEditorScreen`: outline every element and name it, so a drag has a
        // visible target.
        if self.hud_editor {
            let boxes: Vec<(String, f64, f64, f64, f64)> = shared
                .hud
                .elements
                .iter()
                .filter(|e| e.is_active())
                .map(|e| {
                    let (x, y) = e.render_pos();
                    (
                        e.info().title.clone(),
                        x as f64,
                        y as f64,
                        e.width() as f64,
                        e.height() as f64,
                    )
                })
                .collect();
            for (title, x, y, w, h) in boxes {
                let hovered = hit(self.mouse.0, self.mouse.1, x, y, w.max(1.0), h.max(1.0));
                q.outline(
                    x - 1.0,
                    y - 1.0,
                    w + 2.0,
                    h + 2.0,
                    if hovered {
                        shared.theme.accent().argb()
                    } else {
                        shared.theme.separator_edges.color().argb()
                    },
                    1.0,
                );
                q.text(x, y - 10.0 * s, &title, shared.theme.text_color().argb(), 1.0);
            }
        }
    }

    fn render_rows(&self, shared: &mut Shared, window: usize, q: &mut DrawQueue) {
        let s = shared.theme.scale.get();
        let theme = &shared.theme;
        let (x, start_y, w, scroll) = {
            let win = &self.windows[window];
            (win.x, win.y + header_height(shared) + 4.0 * s, win.width, win.scroll)
        };
        let mut y = start_y - scroll;
        let row_h = module_height(shared);
        let modules = self.filtered(shared, self.windows[window].category);
        for name in modules {
            let hovered = self.hovered_module(shared) == Some(name);
            let active = shared.modules.is_active(name);
            let anim = self.anims.get(name).copied().unwrap_or_default();
            let progress = if active || hovered {
                anim.background.max(if hovered { 0.35 } else { 0.0 })
            } else {
                anim.background
            };
            if self.selected == Some(name) {
                q.outline(x, y, w, row_h, theme.accent().argb(), 1.0);
            }
            if progress > 0.0 {
                q.fill(
                    x,
                    y,
                    w * progress.min(1.0),
                    row_h,
                    theme.module_background.color().argb(),
                );
            }
            if anim.bar > 0.0 {
                let bar_h = row_h * anim.bar;
                q.fill(
                    x,
                    y + row_h - bar_h,
                    theme.scale(2.0),
                    bar_h,
                    theme.accent().argb(),
                );
            }
            let title = shared.title_of(name);
            let title_w = theme.text_width(&title);
            let pad = theme.scale(4.0);
            let tx = x + pad + (w - pad * 2.0) / 2.0 - title_w / 2.0;
            q.text(tx, y + pad, &title, theme.text_color().argb(), 1.0);
            y += row_h;

            if self.expanded.get(name).copied().unwrap_or(false) {
                y = self.render_settings(shared, name, q, x, y, w);
            }
        }
    }

    fn render_settings(
        &self,
        shared: &Shared,
        module: &str,
        q: &mut DrawQueue,
        x: f64,
        mut y: f64,
        w: f64,
    ) -> f64 {
        let s = shared.theme.scale.get();
        let row_h = setting_row_height(shared);
        for (name, display) in shared.setting_rows(module) {
            q.fill(x + 2.0 * s, y, w - 4.0 * s, row_h, Color::new(0, 0, 0, 70).argb());
            q.text(
                x + 6.0 * s,
                y + row_h / 2.0 - shared.theme.text_height() / 2.0,
                &name_to_title(&name),
                shared.theme.text_secondary().argb(),
                1.0,
            );
            let wx = x + w - 74.0 * s;
            match shared
                .modules
                .base(module)
                .and_then(|b| b.settings.get(&name))
                .map(|s| &s.value)
            {
                Some(Value::Bool(cell)) => {
                    let on = cell.load(std::sync::atomic::Ordering::Relaxed);
                    let box_w = 10.0 * s;
                    q.fill(
                        wx,
                        y + 2.0 * s,
                        box_w,
                        box_w,
                        shared.theme.background_color(false, false).argb(),
                    );
                    if on {
                        q.fill(
                            wx + 2.0 * s,
                            y + 4.0 * s,
                            box_w - 4.0 * s,
                            box_w - 4.0 * s,
                            shared.theme.checkbox_color.color().argb(),
                        );
                    }
                }
                Some(Value::Int { .. }) | Some(Value::Double { .. }) => {
                    let t = slider_fraction(shared, module, &name);
                    q.fill(
                        wx,
                        y + 4.0 * s,
                        60.0 * s,
                        6.0 * s,
                        shared.theme.slider_right.color().argb(),
                    );
                    q.fill(
                        wx,
                        y + 4.0 * s,
                        60.0 * s * t,
                        6.0 * s,
                        shared.theme.slider_left.color().argb(),
                    );
                    q.fill(
                        wx + 60.0 * s * t - 1.0 * s,
                        y + 2.0 * s,
                        2.0 * s,
                        10.0 * s,
                        shared.theme.slider_handle.get().argb(),
                    );
                }
                _ => {
                    q.text(
                        wx,
                        y + row_h / 2.0 - shared.theme.text_height() / 2.0,
                        &display,
                        shared.theme.text_color().argb(),
                        1.0,
                    );
                }
            }
            y += row_h;
        }
        y
    }
}

fn hit(mx: f64, my: f64, x: f64, y: f64, w: f64, h: f64) -> bool {
    mx >= x && mx <= x + w && my >= y && my <= y + h
}

fn approach(current: f64, target: f64, rate: f64) -> f64 {
    if current < target {
        (current + rate).min(target)
    } else {
        (current - rate).max(target)
    }
}

fn module_height(shared: &Shared) -> f64 {
    let pad = shared.theme.scale(4.0);
    pad + shared.theme.text_height() + pad
}

fn header_height(shared: &Shared) -> f64 {
    let pad = shared.theme.scale(4.0);
    pad + shared.theme.title_text_height() + pad
}

fn setting_row_height(shared: &Shared) -> f64 {
    shared.theme.scale(14.0)
}

fn slider_fraction(shared: &Shared, module: &str, setting: &str) -> f64 {
    let Some(s) = shared
        .modules
        .base(module)
        .and_then(|b| b.settings.get(setting))
    else {
        return 0.0;
    };
    match &s.value {
        Value::Int { cell, min, max } => {
            if *max == *min {
                return 0.0;
            }
            let v = cell.load(std::sync::atomic::Ordering::Relaxed);
            ((v - *min) as f64) / ((*max - *min) as f64)
        }
        Value::Double { cell, min, max, .. } => {
            if (*max - *min).abs() < f64::EPSILON {
                return 0.0;
            }
            let v = f64::from_bits(cell.load(std::sync::atomic::Ordering::Relaxed) as u64);
            (v - *min) / (*max - *min)
        }
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_layout_metrics() {
        let shared = Shared::new();
        assert_eq!(module_height(&shared), 17.0);
        assert_eq!(header_height(&shared), 4.0 + 9.0 * 1.25 + 4.0);
        assert_eq!(setting_row_height(&shared), 14.0);
    }

    #[test]
    fn opens_and_closes() {
        let mut gui = ClickGui::new();
        assert!(!gui.is_open());
        gui.toggle();
        assert!(gui.is_open());
        gui.toggle();
        assert!(!gui.is_open());
    }

    #[test]
    fn escape_closes_even_with_settings_open() {
        let mut gui = ClickGui::new();
        let mut shared = Shared::new();
        gui.open();
        gui.expanded.insert("example".to_string(), true);
        assert!(gui.on_key(&mut shared, 256, true, 0));
        assert!(gui.is_open(), "first Escape closes the settings panel");
        assert!(gui.on_key(&mut shared, 256, true, 0));
        assert!(!gui.is_open());
    }

    #[test]
    fn search_filters_by_score() {
        let shared = Shared::new();
        let mut gui = ClickGui::new();
        gui.search = "exam".to_string();
        assert!(gui.filtered(&shared, Category::Misc).contains(&"example"));
        gui.search = "zzzzz".to_string();
        assert!(gui.filtered(&shared, Category::Misc).is_empty());
    }

    #[test]
    fn animation_steps_toward_the_target() {
        assert_eq!(approach(0.0, 1.0, 0.25), 0.25);
        assert_eq!(approach(0.9, 1.0, 0.25), 1.0);
        assert_eq!(approach(1.0, 0.0, 0.25), 0.75);
    }

    #[test]
    fn clicking_a_module_row_toggles_it() {
        let mut gui = ClickGui::new();
        let mut shared = Shared::new();
        gui.open();
        gui.layout(&shared);
        // Find the example module's row.
        let rows = gui.row_rects(&shared);
        let (name, (x, y, w, h), _panel) = rows
            .into_iter()
            .find(|(n, _, panel)| *n == "example" && !panel)
            .expect("example row");
        gui.mouse = (x + w / 2.0, y + h / 2.0);
        assert!(gui.on_mouse(&mut shared, 0, true));
        assert!(shared.modules.is_active(name));
    }
}
