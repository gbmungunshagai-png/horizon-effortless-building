//! The two screens: the radial build-mode menu (`RadialMenu`, initially from Chisels and Bits)
//! and Modifier Settings (`ModifiersScreen` and its entries), ported from Effortless Building
//! (Requios, LGPL-2.1). Both are a mod screen over the world, drawn with the original's textures.
use crate::client::{self, Client, Deferred, STATE};
use crate::modes::{argbf, Action, BuildMode};
use crate::modifiers::{Array, Mirror, Modifier, RadialMirror};
use mod_api::{ClientHost, DrawContext, Text};
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Radial,
    Modifiers,
}

const ICONS: &str = "effortless:gui/icons";
const MODIFIERS_TEX: &str = "effortless:gui/modifiers";
const WIDGETS: &str = "effortless:gui/widgets";
const CREATE_ICONS: &str = "effortless:gui/create_icons";

fn icon(draw: &mut dyn DrawContext, (col, row): (u32, u32), x: f32, y: f32) {
    draw.sprite(ICONS, [col as f32 * 16.0, row as f32 * 16.0, 16.0, 16.0], [x, y, 16.0, 16.0], None);
}

/// Text in a named colour.
fn styled(text: &str, color: &str) -> Text {
    let mut t = Text::literal(text);
    t.style.color = Some(color.into());
    t
}

/// Splits `text` into lines no wider than `width` characters (TooltipHelper.cutString).
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if !line.is_empty() && line.len() + 1 + word.len() > width {
            lines.push(std::mem::take(&mut line));
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

// =============================================================================================
// Radial menu

struct Radial {
    visibility: f32,
    switch_to: Option<BuildMode>,
    do_action: Option<Action>,
    performed_by_mouse: bool,
}

static RADIAL: LazyLock<Mutex<Radial>> = LazyLock::new(|| {
    Mutex::new(Radial { visibility: 0.0, switch_to: None, do_action: None, performed_by_mouse: false })
});

const RING_INNER: f64 = 30.0;
const RING_OUTER: f64 = 65.0;
const TEXT_DISTANCE: f64 = 75.0;
const BUTTON_DISTANCE: f64 = 105.0;
const DESCRIPTION_HEIGHT: f32 = 100.0;

pub fn radial_opened(_st: &mut Client) {
    let mut r = RADIAL.lock().unwrap();
    r.visibility = 0.0;
    r.performed_by_mouse = false;
    r.switch_to = None;
    r.do_action = None;
}

/// `RadialMenu.onClose`: what the pointer is over is chosen, unless a click already chose.
pub fn radial_closed(st: &mut Client, host: &dyn ClientHost) {
    let mut r = RADIAL.lock().unwrap();
    if !r.performed_by_mouse {
        perform(&mut r, st, host);
    }
    r.switch_to = None;
    r.do_action = None;
}

fn perform(r: &mut Radial, st: &mut Client, host: &dyn ClientHost) -> bool {
    let mut acted = false;
    if let Some(mode) = r.switch_to {
        client::ui_sound(host, "minecraft:ui.button.click", 0.1, 1.0);
        st.set_mode(mode);
        acted = true;
    }
    if let Some(action) = r.do_action {
        client::ui_sound(host, "minecraft:ui.button.click", 0.1, 1.0);
        st.deferred.push(Deferred::Action(action));
        acted = true;
    }
    acted
}

struct Button {
    action: Action,
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl Button {
    fn new(action: Action, x: f64, y: f64) -> Self {
        Self { action, x1: x - 10.0, x2: x + 10.0, y1: y - 10.0, y2: y + 10.0 }
    }
}

fn in_triangle(x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64, x: f64, y: f64) -> bool {
    let sign = |n: f64| if n > 0.0 { 1 } else { -1 };
    let ab = (x1 - x) * (y2 - y) - (x2 - x) * (y1 - y);
    let bc = (x2 - x) * (y3 - y) - (x3 - x) * (y2 - y);
    let ca = (x3 - x) * (y1 - y) - (x1 - x) * (y3 - y);
    sign(ab) == sign(bc) && sign(bc) == sign(ca)
}

fn buttons(st: &Client) -> Vec<Button> {
    let d = BUTTON_DISTANCE;
    let mut b = Vec::new();
    if st.can_replace() {
        b.push(Button::new(Action::ToggleProtectTileEntities, -d - 78.0, -13.0));
    }
    b.push(Button::new(Action::OpenModifierSettings, -d - 52.0, -13.0));
    b.push(Button::new(Action::Undo, -d - 26.0, -13.0));
    b.push(Button::new(Action::Redo, -d, -13.0));
    if st.can_replace() {
        b.push(Button::new(Action::ReplaceOnlyAir, -d - 78.0, 13.0));
        b.push(Button::new(Action::ReplaceBlocksAndAir, -d - 52.0, 13.0));
        b.push(Button::new(Action::ReplaceOnlyBlocks, -d - 26.0, 13.0));
        b.push(Button::new(Action::ReplaceFilteredByOffhand, -d, 13.0));
    }
    for (i, option) in st.mode.options().iter().enumerate() {
        for (j, action) in option.actions().iter().enumerate() {
            b.push(Button::new(*action, d + j as f64 * 26.0, -13.0 + i as f64 * 39.0));
        }
    }
    b
}

fn is_selected(st: &Client, action: Action) -> bool {
    st.mode.options().iter().any(|o| st.opts.selected(*o) == action)
        || action == st.replace_action()
        || (action == Action::ToggleProtectTileEntities && st.protect_tile_entities)
}

fn key_label(host: &dyn ClientHost, id: &str) -> Option<String> {
    let (name, mods) = host.mod_key(client::MOD_ID, id)?;
    let key = name.rsplit('.').next().unwrap_or(&name).to_uppercase();
    let mut out = String::new();
    if mods.bits & 2 != 0 {
        out.push_str("Ctrl + ");
    }
    if mods.bits & 1 != 0 {
        out.push_str("Shift + ");
    }
    if mods.bits & 4 != 0 {
        out.push_str("Alt + ");
    }
    out.push_str(&key);
    Some(out)
}

pub fn draw_radial(host: &dyn ClientHost, draw: &mut dyn DrawContext, mouse: [f32; 2], partial: f32) {
    let st = STATE.lock().unwrap();
    let mut r = RADIAL.lock().unwrap();
    let [w, h] = draw.size();
    r.visibility = (r.visibility + 0.3 * partial.max(0.05)).min(1.0);
    let start = ((r.visibility * 98.0) as u32) << 24;
    let end = ((r.visibility * 128.0) as u32) << 24;
    draw.fill_gradient([0.0, 0.0, w, h], start, end);

    let (mx, my) = (w as f64 / 2.0, h as f64 / 2.0);
    let (mxc, myc) = (mouse[0] as f64 - mx, mouse[1] as f64 - my);
    let quarter = std::f64::consts::FRAC_PI_2;
    let mut mouse_rad = myc.atan2(mxc);
    if mouse_rad < -quarter {
        mouse_rad += std::f64::consts::TAU;
    }

    let radial_color = argbf(0.0, 0.0, 0.0, 0.5);
    let side_color = argbf(0.5, 0.5, 0.5, 0.5);
    let highlight = argbf(0.6, 0.8, 1.0, 0.6);
    let selected = argbf(0.0, 0.5, 1.0, 0.5);
    let highlight_selected = argbf(0.2, 0.7, 1.0, 0.7);

    r.switch_to = None;
    r.do_action = None;

    // Build mode ring.
    let modes = BuildMode::ALL;
    let total = modes.len().max(3) as f64;
    let frag = std::f64::consts::PI * 0.005;
    let frag2 = std::f64::consts::PI * 0.0025;
    let per = std::f64::consts::TAU / total;
    let mut regions = Vec::new();
    for (i, mode) in modes.iter().enumerate() {
        let begin = i as f64 * per - quarter;
        let end = (i + 1) as f64 * per - quarter;
        let p = |a: f64, rad: f64| [mx + a.cos() * rad, my + a.sin() * rad];
        let in1 = p(begin + frag, RING_INNER);
        let in2 = p(end - frag, RING_INNER);
        let out1 = p(begin + frag2, RING_OUTER);
        let out2 = p(end - frag2, RING_OUTER);
        let rel = |q: [f64; 2]| (q[0] - mx, q[1] - my);
        let (a, b, c, d) = (rel(in1), rel(in2), rel(out1), rel(out2));
        let in_quad = in_triangle(a.0, a.1, d.0, d.1, b.0, b.1, mxc, myc) || in_triangle(a.0, a.1, c.0, c.1, d.0, d.1, mxc, myc);
        let hovered = begin <= mouse_rad && mouse_rad <= end && in_quad;
        let is_sel = *mode == st.mode;
        let color = match (is_sel, hovered) {
            (true, true) => highlight_selected,
            (false, true) => highlight,
            (true, false) => selected,
            _ => radial_color,
        };
        if hovered {
            r.switch_to = Some(*mode);
        }
        let f = |q: [f64; 2]| [q[0] as f32, q[1] as f32];
        draw.quad([f(in1), f(in2), f(out2), f(out1)], color);
        // Category line along the inner edge.
        let line_out = RING_INNER + 1.0;
        let l1 = p(begin + frag, line_out);
        let l2 = p(end - frag, line_out);
        draw.quad([f(in1), f(in2), f(l2), f(l1)], mode.category().color());
        regions.push((*mode, begin.cos(), end.cos(), begin.sin(), end.sin(), hovered));
    }

    // Side buttons.
    let buttons = buttons(&st);
    let mut hovered_button = None;
    for b in &buttons {
        let hovered = b.x1 <= mxc && b.x2 >= mxc && b.y1 <= myc && b.y2 >= myc;
        let is_sel = is_selected(&st, b.action);
        let color = match (is_sel, hovered) {
            (true, true) => highlight_selected,
            (false, true) => highlight,
            (true, false) => selected,
            _ => side_color,
        };
        if hovered {
            r.do_action = Some(b.action);
            hovered_button = Some(b.action);
        }
        draw.fill([(mx + b.x1) as f32, (my + b.y1) as f32, 20.0, 20.0], color);
    }

    // Icons.
    let icon_radius = RING_OUTER * 0.55 + 0.45 * RING_INNER;
    for (mode, x1, x2, y1, y2, _) in &regions {
        let x = (x1 + x2) * 0.5 * icon_radius;
        let y = (y1 + y2) * 0.5 * icon_radius;
        icon(draw, mode.icon(), (mx + x - 8.0) as i32 as f32, (my + y - 8.0) as i32 as f32);
    }
    for b in &buttons {
        let x = (b.x1 + b.x2) / 2.0 + 0.01;
        let y = (b.y1 + b.y2) / 2.0 + 0.01;
        icon(draw, b.action.icon(), (mx + x - 8.0) as i32 as f32, (my + y - 8.0) as i32 as f32);
    }

    // Option titles.
    for (i, option) in st.mode.options().iter().enumerate() {
        draw.text(
            option.name(),
            [(mx + BUTTON_DISTANCE - 9.0) as i32 as f32, (my - 37.0) as f32 + i as f32 * 39.0],
            0xEEEEEEFF,
            true,
        );
    }

    let credits = "Effortless Building";
    let cw = draw.text_width(credits);
    draw.text(credits, [w - cw - 4.0, h - 10.0], 0x88888888, true);
    let power = if st.creative { "Power Level: Creative".to_owned() } else { "Power Level: 0".to_owned() };
    let pw = draw.text_width(&power);
    draw.text(&power, [w - pw - 4.0, h - 22.0], if st.creative { 0x88888888 } else { 0xFFAA00AA }, true);
    let power_hover = mouse[0] >= w - pw - 14.0 && mouse[1] >= h - 24.0;

    // The highlighted mode's name and description.
    for (mode, x1, x2, y1, y2, hovered) in &regions {
        if !hovered {
            continue;
        }
        let x = (x1 + x2) * 0.5;
        let y = (y1 + y2) * 0.5;
        let name = mode.name();
        let tw = draw.text_width(name) as f64;
        let mut fx = (x * TEXT_DISTANCE) as i32 as f64;
        let fy = (y * TEXT_DISTANCE) as i32 as f64 - 4.0;
        if x <= -0.2 {
            fx -= tw;
        } else if (-0.2..=0.2).contains(&x) {
            fx -= (tw as i32 / 2) as f64;
        }
        draw.text(name, [(mx as i32 as f64 + fx) as f32, (my as i32 as f64 + fy) as f32], 0xFFFFFFFF, true);
        let desc = mode.description();
        if !desc.is_empty() {
            let dw = draw.text_width(desc);
            draw.text(desc, [(mx as f32 - dw / 2.0) as i32 as f32, my as f32 + DESCRIPTION_HEIGHT], 0xDD888888, true);
        }
    }

    // Tooltips.
    if power_hover {
        let reach = st.placement_reach();
        let lines = vec![
            styled(&power, "dark_purple"),
            styled(&format!("Placement reach: {}", if reach == 0 { "vanilla".to_owned() } else { format!("{reach} blocks") }), "gray"),
            styled(&format!("Max blocks per axis: {}", st.axis_limit()), "gray"),
            styled(&format!("Max blocks placed at once: {}", if st.creative { 10000 } else { 128 }), "gray"),
            styled(&format!("Max (radial) mirror radius: {} blocks", st.mirror_radius_max()), "gray"),
        ];
        let _ = draw.gui_tooltip(&lines, mouse);
    } else if let Some(action) = hovered_button {
        let mut lines = vec![styled(action.name(), "aqua")];
        let desc = action.description();
        if !desc.is_empty() {
            let shift = host.is_key_down("key.keyboard.left.shift") || host.is_key_down("key.keyboard.right.shift");
            lines.push(styled(&format!("Hold [{}] for Summary", if shift { "Shift" } else { "Shift" }), "dark_gray"));
            if shift {
                for l in wrap(desc, 40) {
                    lines.push(styled(&l, "gray"));
                }
            }
        }
        let key_id = match action {
            Action::OpenModifierSettings => Some("modifiers"),
            Action::Undo => Some("undo"),
            Action::Redo => Some("redo"),
            _ => None,
        };
        if let Some(label) = key_id.and_then(|id| key_label(host, id)) {
            lines.push(styled(&format!("[{label}]"), "dark_gray"));
        }
        let _ = draw.gui_tooltip(&lines, mouse);
    }
}

/// A click on the radial menu acts at once (`performedActionUsingMouse`).
pub fn radial_click(host: &dyn ClientHost) {
    let mut st = STATE.lock().unwrap();
    let mut r = RADIAL.lock().unwrap();
    if perform(&mut r, &mut st, host) {
        r.performed_by_mouse = true;
    }
}

// =============================================================================================
// Modifier Settings

#[derive(Clone, Copy, Debug, PartialEq)]
enum Widget {
    AddArray,
    AddMirror,
    AddRadial,
    Close,
    Enable(usize),
    Up(usize),
    Down(usize),
    Remove(usize),
    Position(usize, usize),
    PlayerPosition(usize),
    ToggleOffset(usize),
    Axis(usize, usize),
    Radius(usize),
    Lines(usize),
    Areas(usize),
    Offset(usize, usize),
    Count(usize),
    Slices(usize),
    Alternate(usize),
}

struct Layout {
    widgets: Vec<([f32; 4], Widget)>,
    list: [f32; 4],
}

static MOD_SCREEN: LazyLock<Mutex<(f32, Layout)>> =
    LazyLock::new(|| Mutex::new((0.0, Layout { widgets: Vec::new(), list: [0.0; 4] })));

const ENTRY_WIDTH: f32 = 226.0;
const ROW: f32 = 68.0;

fn inside(r: [f32; 4], p: [f32; 2]) -> bool {
    p[0] >= r[0] && p[0] < r[0] + r[2] && p[1] >= r[1] && p[1] < r[1] + r[3]
}

fn half_text(v: f64) -> String {
    // DecimalFormat("#.#")
    let r = (v * 10.0).round() / 10.0;
    if r.fract() == 0.0 { format!("{}", r as i64) } else { format!("{r:.1}") }
}

fn tooltip_for(st: &Client, w: Widget) -> Vec<Text> {
    let gray = |s: &str| styled(s, "gray");
    let white = |s: &str| styled(s, "white");
    let scroll = |title: &str, value: String| {
        vec![white(title), styled(&format!("Value: {value}"), "gold"), gray("Scroll to Modify"), styled("Shift to Scroll Faster", "dark_gray")]
    };
    let m = |i: usize| st.modifiers.get(i);
    match w {
        Widget::AddArray => vec![white("Add Array")],
        Widget::AddMirror => vec![white("Add Mirror")],
        Widget::AddRadial => vec![white("Add Radial Mirror")],
        Widget::Close => vec![white("Close")],
        Widget::Enable(i) => {
            vec![white(if m(i).map_or(false, |m| m.enabled()) { "Disable this modifier" } else { "Enable this modifier" })]
        }
        Widget::Up(_) => vec![white("Move up")],
        Widget::Down(_) => vec![white("Move down")],
        Widget::Remove(_) => vec![white("Remove")],
        Widget::Position(i, a) => {
            let pos = match m(i) {
                Some(Modifier::Mirror(x)) => x.position[a],
                Some(Modifier::RadialMirror(x)) => x.position[a],
                _ => 0.0,
            };
            let mut t = scroll(["X Position", "Y Position", "Z Position"][a], half_text(pos));
            t.push(styled("Control to Scroll Slower", "dark_gray"));
            t
        }
        Widget::PlayerPosition(_) => vec![white("Set to player position")],
        Widget::ToggleOffset(i) => {
            let corner = match m(i) {
                Some(Modifier::Mirror(x)) => x.position[0] == x.position[0].floor(),
                Some(Modifier::RadialMirror(x)) => x.position[0] == x.position[0].floor(),
                _ => true,
            };
            vec![white(if corner {
                "Set position to center of block, for uneven numbered builds."
            } else {
                "Set position to corner of block, for even numbered builds."
            })]
        }
        Widget::Axis(_, a) => vec![white(["X Axis", "Y Axis", "Z Axis"][a])],
        Widget::Radius(i) => {
            let r = match m(i) {
                Some(Modifier::Mirror(x)) => x.radius,
                Some(Modifier::RadialMirror(x)) => x.radius,
                _ => 0,
            };
            let title = if st.creative { "Radius" } else { "Radius. Use Reach Upgrade items to increase maximum." };
            scroll(title, r.to_string())
        }
        Widget::Lines(_) => vec![white("Show mirror lines")],
        Widget::Areas(_) => vec![white("Show mirror areas")],
        Widget::Offset(i, a) => {
            let v = if let Some(Modifier::Array(x)) = m(i) { x.offset[a] } else { 0 };
            scroll(["X Offset", "Y Offset", "Z Offset"][a], v.to_string())
        }
        Widget::Count(i) => {
            let v = if let Some(Modifier::Array(x)) = m(i) { x.count } else { 0 };
            scroll("Count", v.to_string())
        }
        Widget::Slices(i) => {
            let v = if let Some(Modifier::RadialMirror(x)) = m(i) { x.slices } else { 0 };
            scroll("Slices", v.to_string())
        }
        Widget::Alternate(i) => {
            let on = matches!(m(i), Some(Modifier::RadialMirror(x)) if x.alternate);
            vec![white(if on {
                "Alternating the direction of every other slice."
            } else {
                "Alternate the direction of every other slice. Currently off."
            })]
        }
    }
}

/// Create's `IconButton`: the 18-pixel button of `widgets.png` with an icon on it.
fn icon_button(draw: &mut dyn DrawContext, x: f32, y: f32, ic: (u32, u32), hovered: bool) {
    let u = if hovered { 18.0 } else { 0.0 };
    draw.sprite(WIDGETS, [u, 0.0, 18.0, 18.0], [x, y, 18.0, 18.0], None);
    icon(draw, ic, x + 1.0, y + 1.0);
}

/// Create's `BoxWidget` with an icon of `create_icons.png`.
fn box_button(draw: &mut dyn DrawContext, x: f32, y: f32, ic: (u32, u32), hovered: bool) {
    let border = if hovered { 0xFFB6D8F2 } else { 0xFF5F6C80 };
    draw.fill([x, y, 20.0, 20.0], border);
    draw.fill_gradient([x + 1.0, y + 1.0, 18.0, 18.0], 0xEE1E1E2E, 0xEE12121C);
    draw.sprite(CREATE_ICONS, [ic.0 as f32 * 16.0, ic.1 as f32 * 16.0, 16.0, 16.0], [x + 2.0, y + 2.0, 16.0, 16.0], None);
}

fn value_label(draw: &mut dyn DrawContext, rect: [f32; 4], text: &str, color: u32) {
    let tw = draw.text_width(text);
    draw.text(text, [(rect[0] + rect[2] / 2.0 - tw / 2.0).floor(), rect[1] + rect[3] / 2.0 - 4.0], color, true);
}

pub fn draw_modifiers(host: &dyn ClientHost, draw: &mut dyn DrawContext, mouse: [f32; 2]) {
    let st = STATE.lock().unwrap();
    let mut screen = MOD_SCREEN.lock().unwrap();
    let [w, h] = draw.size();
    host.apply_screen_blur();
    draw.draw_menu_background([0.0, 0.0, w, h], true);

    let list_w = (w - 80.0).min(300.0);
    let list_l = w / 2.0 - list_w / 2.0;
    let list_r = w / 2.0 + list_w / 2.0;
    let list = [list_l, 45.0, list_w, h - 80.0];
    let n = st.modifiers.len();
    let max_scroll = (n as f32 * ROW + 4.0 - list[3]).max(0.0);
    screen.0 = screen.0.clamp(0.0, max_scroll);
    let scroll = screen.0;
    let mut widgets: Vec<([f32; 4], Widget)> = Vec::new();

    let top_buttons = [
        (list_r - 90.0, Widget::AddArray),
        (list_r - 60.0, Widget::AddMirror),
        (list_r - 30.0, Widget::AddRadial),
    ];
    for (x, wdg) in top_buttons {
        let r = [x, 10.0, 20.0, 20.0];
        box_button(draw, x, 10.0, (0, 0), inside(r, mouse));
        widgets.push((r, wdg));
    }
    let close = [list_l - 30.0, h / 2.0 - 10.0, 20.0, 20.0];
    box_button(draw, close[0], close[1], (5, 10), inside(close, mouse));
    widgets.push((close, Widget::Close));

    let _ = draw.push_clip(list);
    for (i, m) in st.modifiers.iter().enumerate() {
        let top = list[1] + 4.0 + i as f32 * ROW - scroll;
        if top + 64.0 < list[1] || top > list[1] + list[3] {
            continue;
        }
        let left = (w / 2.0 - ENTRY_WIDTH / 2.0).floor();
        let right = left + ENTRY_WIDTH;
        let bg_v = match m {
            Modifier::Array(_) => 0.0,
            Modifier::Mirror(_) => 64.0,
            Modifier::RadialMirror(_) => 128.0,
        };
        draw.sprite(MODIFIERS_TEX, [0.0, bg_v, 226.0, 64.0], [left, top, 226.0, 64.0], None);
        let mut add = |r: [f32; 4], wdg: Widget| {
            if inside(list, mouse) {
                widgets.push((r, wdg));
            }
        };

        // BaseModifierEntry: enable box, name, move and remove.
        draw.sprite(MODIFIERS_TEX, [234.0, 0.0, 9.0, 9.0], [left + 4.0, top + 3.0, 9.0, 9.0], None);
        if m.enabled() {
            draw.sprite(MODIFIERS_TEX, [243.0, 0.0, 10.0, 9.0], [left + 5.0, top + 3.0, 10.0, 9.0], None);
        }
        add([left + 4.0, top + 3.0, 100.0, 9.0], Widget::Enable(i));
        draw.text(m.name(), [left + 18.0, top + 4.0], 0xFFFFFFFF, false);
        if i > 0 {
            draw.sprite(MODIFIERS_TEX, [234.0, 9.0, 9.0, 9.0], [right - 31.0, top + 3.0, 9.0, 9.0], None);
            add([right - 31.0, top + 3.0, 9.0, 9.0], Widget::Up(i));
        }
        if i + 1 < n {
            draw.sprite(MODIFIERS_TEX, [243.0, 9.0, 9.0, 9.0], [right - 22.0, top + 3.0, 9.0, 9.0], None);
            add([right - 22.0, top + 3.0, 9.0, 9.0], Widget::Down(i));
        }
        draw.sprite(MODIFIERS_TEX, [234.0, 18.0, 9.0, 9.0], [right - 13.0, top + 3.0, 9.0, 9.0], None);
        add([right - 13.0, top + 3.0, 9.0, 9.0], Widget::Remove(i));

        let hov = |r: [f32; 4]| inside(r, mouse) && inside(list, mouse);
        match m {
            Modifier::Mirror(mm) => {
                for a in 0..3 {
                    let r = [left + 49.0 + 38.0 * a as f32, top + 19.0, 36.0, 18.0];
                    value_label(draw, r, &half_text(mm.position[a]), 0xFFFFFFFF);
                    add(r, Widget::Position(i, a));
                }
                position_buttons(draw, left, top, mm.position, &mut add, &hov, i);
                for a in 0..3 {
                    let r = [left + 49.0 + 18.0 * a as f32, top + 41.0, 18.0, 18.0];
                    let ic = (7 + 2 * a as u32 + mm.axes[a] as u32, 3);
                    icon_button(draw, r[0], r[1], ic, hov(r));
                    add(r, Widget::Axis(i, a));
                }
                let r = [left + 134.0, top + 41.0, 27.0, 18.0];
                value_label(draw, r, &mm.radius.to_string(), 0xFFFFFFFF);
                add(r, Widget::Radius(i));
                lines_areas(draw, left, top, mm.draw_lines, mm.draw_planes, &mut add, &hov, i);
            }
            Modifier::RadialMirror(rm) => {
                for a in 0..3 {
                    let r = [left + 49.0 + 38.0 * a as f32, top + 19.0, 36.0, 18.0];
                    value_label(draw, r, &half_text(rm.position[a]), 0xFFFFFFFF);
                    add(r, Widget::Position(i, a));
                }
                position_buttons(draw, left, top, rm.position, &mut add, &hov, i);
                let r = [left + 49.0, top + 41.0, 27.0, 18.0];
                value_label(draw, r, &rm.slices.to_string(), 0xFFFFFFFF);
                add(r, Widget::Slices(i));
                let r = [left + 78.0, top + 41.0, 18.0, 18.0];
                icon_button(draw, r[0], r[1], (if rm.alternate { 14 } else { 13 }, 3), hov(r));
                add(r, Widget::Alternate(i));
                let r = [left + 134.0, top + 41.0, 27.0, 18.0];
                value_label(draw, r, &rm.radius.to_string(), 0xFFFFFFFF);
                add(r, Widget::Radius(i));
                lines_areas(draw, left, top, rm.draw_lines, rm.draw_planes, &mut add, &hov, i);
            }
            Modifier::Array(am) => {
                for a in 0..3 {
                    let r = [left + 49.0 + 20.0 * a as f32, top + 19.0, 18.0, 18.0];
                    value_label(draw, r, &am.offset[a].to_string(), 0xFFFFFFFF);
                    add(r, Widget::Offset(i, a));
                }
                let r = [left + 49.0, top + 41.0, 18.0, 18.0];
                value_label(draw, r, &am.count.to_string(), 0xFFFFFFFF);
                add(r, Widget::Count(i));
                let reach = am.reach().max(-1);
                let max = st.axis_limit();
                let valid = reach <= max && reach > -1;
                let text = format!("{reach}/{max}");
                let tw = draw.text_width(&text);
                let x = right - 8.0 - tw;
                let rw = draw.text_width(&reach.to_string());
                draw.text(&reach.to_string(), [x, top + 24.0], if valid { 0xFFAAAAAA } else { 0xFFFF5555 }, true);
                draw.text(&format!("/{max}"), [x + rw, top + 24.0], 0xFFAAAAAA, true);
            }
        }
    }
    let _ = draw.pop_clip();

    if let Some((_, wdg)) = widgets.iter().rev().find(|(r, _)| inside(*r, mouse)) {
        let lines = tooltip_for(&st, *wdg);
        let _ = draw.gui_tooltip(&lines, mouse);
    }
    screen.1 = Layout { widgets, list };
}

fn position_buttons(
    draw: &mut dyn DrawContext,
    left: f32,
    top: f32,
    pos: [f64; 3],
    add: &mut dyn FnMut([f32; 4], Widget),
    hov: &dyn Fn([f32; 4]) -> bool,
    i: usize,
) {
    let r = [left + 163.0, top + 19.0, 18.0, 18.0];
    icon_button(draw, r[0], r[1], (0, 3), hov(r));
    add(r, Widget::PlayerPosition(i));
    let r = [left + 183.0, top + 19.0, 18.0, 18.0];
    let corner = pos[0] == pos[0].floor();
    icon_button(draw, r[0], r[1], (if corner { 1 } else { 2 }, 3), hov(r));
    add(r, Widget::ToggleOffset(i));
}

fn lines_areas(
    draw: &mut dyn DrawContext,
    left: f32,
    top: f32,
    lines: bool,
    areas: bool,
    add: &mut dyn FnMut([f32; 4], Widget),
    hov: &dyn Fn([f32; 4]) -> bool,
    i: usize,
) {
    let r = [left + 163.0, top + 41.0, 18.0, 18.0];
    icon_button(draw, r[0], r[1], (if lines { 4 } else { 3 }, 3), hov(r));
    add(r, Widget::Lines(i));
    let r = [left + 183.0, top + 41.0, 18.0, 18.0];
    icon_button(draw, r[0], r[1], (if areas { 6 } else { 5 }, 3), hov(r));
    add(r, Widget::Areas(i));
}

fn hit(mouse: [f32; 2]) -> Option<Widget> {
    let screen = MOD_SCREEN.lock().unwrap();
    screen.1.widgets.iter().rev().find(|(r, _)| inside(*r, mouse)).map(|(_, w)| *w)
}

pub fn modifiers_click(host: &dyn ClientHost, mouse: [f32; 2]) {
    let Some(wdg) = hit(mouse) else { return };
    let mut st = STATE.lock().unwrap();
    let player = st.player_block;
    let creative_max = st.mirror_radius_max();
    let _ = creative_max;
    client::ui_sound(host, "minecraft:ui.button.click", 0.25, 1.0);
    let n = st.modifiers.len();
    match wdg {
        Widget::AddArray => st.modifiers.push(Modifier::Array(Array::new())),
        Widget::AddMirror => st.modifiers.push(Modifier::Mirror(Mirror::new(player))),
        Widget::AddRadial => st.modifiers.push(Modifier::RadialMirror(RadialMirror::new(player))),
        Widget::Close => st.deferred.push(Deferred::CloseScreen),
        Widget::Enable(i) => {
            if let Some(m) = st.modifiers.get_mut(i) {
                let on = m.enabled();
                m.set_enabled(!on);
            }
        }
        Widget::Up(i) if i > 0 && i < n => st.modifiers.swap(i, i - 1),
        Widget::Down(i) if i + 1 < n => st.modifiers.swap(i, i + 1),
        Widget::Remove(i) if i < n => {
            st.modifiers.remove(i);
        }
        Widget::PlayerPosition(i) => {
            let p = [player[0] as f64, player[1] as f64, player[2] as f64];
            match st.modifiers.get_mut(i) {
                Some(Modifier::Mirror(m)) => m.position = p,
                Some(Modifier::RadialMirror(m)) => m.position = p,
                _ => {}
            }
        }
        Widget::ToggleOffset(i) => {
            let toggle = |p: &mut [f64; 3]| {
                let corner = p[0] == p[0].floor();
                for v in p.iter_mut() {
                    *v = v.floor() + if corner { 0.5 } else { 0.0 };
                }
            };
            match st.modifiers.get_mut(i) {
                Some(Modifier::Mirror(m)) => toggle(&mut m.position),
                Some(Modifier::RadialMirror(m)) => toggle(&mut m.position),
                _ => {}
            }
        }
        Widget::Axis(i, a) => {
            if let Some(Modifier::Mirror(m)) = st.modifiers.get_mut(i) {
                m.axes[a] = !m.axes[a];
            }
        }
        Widget::Lines(i) => match st.modifiers.get_mut(i) {
            Some(Modifier::Mirror(m)) => m.draw_lines = !m.draw_lines,
            Some(Modifier::RadialMirror(m)) => m.draw_lines = !m.draw_lines,
            _ => {}
        },
        Widget::Areas(i) => match st.modifiers.get_mut(i) {
            Some(Modifier::Mirror(m)) => m.draw_planes = !m.draw_planes,
            Some(Modifier::RadialMirror(m)) => m.draw_planes = !m.draw_planes,
            _ => {}
        },
        Widget::Alternate(i) => {
            if let Some(Modifier::RadialMirror(m)) = st.modifiers.get_mut(i) {
                m.alternate = !m.alternate;
            }
        }
        _ => {}
    }
}

/// `ScrollInput.mouseScrolled`, or the list scrolling when the pointer is over no input.
pub fn modifiers_scroll(mouse: [f32; 2], amount: f64, modifiers: u32) {
    let shift = modifiers & 1 != 0;
    let control = modifiers & 2 != 0;
    let dir = if amount > 0.0 { 1 } else if amount < 0.0 { -1 } else { 0 };
    if dir == 0 {
        return;
    }
    let wdg = hit(mouse);
    let mut st = STATE.lock().unwrap();
    let max_radius = st.mirror_radius_max();
    // Create's ScrollInput: shift steps by 5 (snapped), else 1, within the input's range.
    let step = |v: i32, min: i32, max: i32| {
        let mut s = v + dir * if shift { 5 } else { 1 };
        if shift {
            s -= s % 5;
        }
        s.clamp(min, max)
    };
    match wdg {
        Some(Widget::Position(i, a)) => {
            let by = if shift { 20 } else if control { 1 } else { 2 };
            let bump = |p: &mut [f64; 3]| {
                let state = (p[a] * 2.0) as i32 + dir * by;
                p[a] = state as f64 / 2.0;
            };
            match st.modifiers.get_mut(i) {
                Some(Modifier::Mirror(m)) => bump(&mut m.position),
                Some(Modifier::RadialMirror(m)) => bump(&mut m.position),
                _ => {}
            }
        }
        Some(Widget::Radius(i)) => match st.modifiers.get_mut(i) {
            Some(Modifier::Mirror(m)) => m.radius = step(m.radius, 0, max_radius),
            Some(Modifier::RadialMirror(m)) => m.radius = step(m.radius, 0, max_radius),
            _ => {}
        },
        Some(Widget::Offset(i, a)) => {
            if let Some(Modifier::Array(m)) = st.modifiers.get_mut(i) {
                m.offset[a] = step(m.offset[a], i32::MIN / 2, i32::MAX / 2);
            }
        }
        Some(Widget::Count(i)) => {
            if let Some(Modifier::Array(m)) = st.modifiers.get_mut(i) {
                m.count = step(m.count, 1, 100);
            }
        }
        Some(Widget::Slices(i)) => {
            if let Some(Modifier::RadialMirror(m)) = st.modifiers.get_mut(i) {
                m.slices = step(m.slices, 3, 1000);
            }
        }
        _ => {
            drop(st);
            let mut screen = MOD_SCREEN.lock().unwrap();
            if inside(screen.1.list, mouse) {
                screen.0 -= dir as f32 * ROW / 2.0;
            }
        }
    }
}
