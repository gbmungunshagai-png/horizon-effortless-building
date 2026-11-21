//! Client side, after Effortless Building's `BuilderChain`, `BuildModes`, `ClientEvents`,
//! `BuilderFilter`, `BlockPreviews` and `ModifierRenderer` (Requios, LGPL-3.0).
use crate::modes::{self, Action, BuildMode, ModeOptions, Pos, Vec3};
use crate::modifiers::{BlockSet, Entry, Modifier};
use crate::proto::{History, PlaceBlocks, HISTORY, PLACE};
use crate::ui::{self, Screen};
use mod_api::{
    BlockKey, BlockState, ClientHost, ClientModifiers, Flow, Hand, Hit, HostBlockStates, InteractionAttempt, InteractionKey,
    InteractionKeyMappingTriggeredEvent, ItemKey, KeyRegistration, PlayerRef, Registrar, SoundPlay, SoundSource,
    WorldDepth, WorldDrawContext, WorldLine, WorldQuad,
};
use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex};

pub const MOD_ID: &str = "effortless";

// CommonConfig defaults: creative, and survival at power level 0.
const CREATIVE_REACH: i32 = 200;
const SURVIVAL_REACH: i32 = 0;
const CREATIVE_AXIS: i32 = 1000;
const SURVIVAL_AXIS: i32 = 8;
const CREATIVE_AT_ONCE: usize = 10_000;
const SURVIVAL_AT_ONCE: usize = 128;
pub const CREATIVE_MIRROR_RADIUS: i32 = 200;
pub const SURVIVAL_MIRROR_RADIUS: i32 = 16;
// ClientConfig defaults.
const MAX_BLOCK_PREVIEWS: usize = 400;
const APPEAR_ANIMATION: u64 = 5;
const BREAK_ANIMATION: u64 = 10;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReplaceMode {
    OnlyAir,
    BlocksAndAir,
    OnlyBlocks,
    FilteredByOffhand,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Building {
    Idle,
    Placing,
    Breaking,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Abilities {
    CanPlaceAndBreak,
    CanBreak,
    None,
}

/// A block set shown with the appear or break animation after it was sent.
struct Placed {
    time: u64,
    breaking: bool,
    blocks: Vec<(Pos, Option<BlockState>)>,
    first: Pos,
    last: Pos,
}

/// Something a screen hook asked for that needs the mutable host: done next tick.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Deferred {
    Action(Action),
    CloseScreen,
}

pub struct Client {
    pub mode: BuildMode,
    previous_mode: BuildMode,
    before_disabled: BuildMode,
    pub opts: ModeOptions,
    pub replace: ReplaceMode,
    pub protect_tile_entities: bool,
    pub modifiers: Vec<Modifier>,

    // BuilderChain
    blocks: BlockSet,
    new_states: Vec<Option<BlockState>>,
    invalid: Vec<bool>,
    building: Building,
    abilities: Abilities,
    clicks: u8,
    first: Option<Pos>,
    second: Option<Pos>,
    start_place: Option<Pos>,
    start_break: Option<Pos>,
    looking_near: Option<Hit>,
    looking_at: Option<Hit>,
    previous_coords: HashSet<Pos>,
    previous_item: Option<ItemKey>,
    sound_time: u64,
    place_cooldown: i32,
    break_cooldown: i32,
    /// A use or attack press the game reported (a tap can start and end inside one tick).
    pressed_use: bool,
    pressed_attack: bool,

    pub ticks: u64,
    placed: Vec<Placed>,
    action_bar: Option<(String, u64)>,
    pub screen: Option<Screen>,
    pub deferred: Vec<Deferred>,
    pub creative: bool,
    pub player_block: Pos,
    map_colors: HashMap<BlockState, u32>,
}

pub static STATE: LazyLock<Mutex<Client>> = LazyLock::new(|| {
    Mutex::new(Client {
        mode: BuildMode::Disabled,
        previous_mode: BuildMode::Disabled,
        before_disabled: BuildMode::Single,
        opts: ModeOptions::default(),
        replace: ReplaceMode::OnlyAir,
        protect_tile_entities: true,
        modifiers: Vec::new(),
        blocks: BlockSet::default(),
        new_states: Vec::new(),
        invalid: Vec::new(),
        building: Building::Idle,
        abilities: Abilities::None,
        clicks: 0,
        first: None,
        second: None,
        start_place: None,
        start_break: None,
        looking_near: None,
        looking_at: None,
        previous_coords: HashSet::new(),
        previous_item: None,
        sound_time: 0,
        place_cooldown: 0,
        break_cooldown: 0,
        pressed_use: false,
        pressed_attack: false,
        ticks: 0,
        placed: Vec::new(),
        action_bar: None,
        screen: None,
        deferred: Vec::new(),
        creative: false,
        player_block: [0; 3],
        map_colors: HashMap::new(),
    })
});

impl Client {
    /// `EffortlessBuilding.log(player, msg, true)`: the action bar.
    pub fn log(&mut self, msg: impl Into<String>) {
        self.action_bar = Some((msg.into(), self.ticks));
    }

    pub fn placement_reach(&self) -> i32 {
        if self.creative { CREATIVE_REACH } else { SURVIVAL_REACH }
    }
    fn build_mode_reach(&self) -> i32 {
        self.placement_reach() + 6
    }
    pub fn axis_limit(&self) -> i32 {
        if self.creative { CREATIVE_AXIS } else { SURVIVAL_AXIS }
    }
    fn at_once(&self) -> usize {
        if self.creative { CREATIVE_AT_ONCE } else { SURVIVAL_AT_ONCE }
    }
    pub fn mirror_radius_max(&self) -> i32 {
        if self.creative { CREATIVE_MIRROR_RADIUS } else { SURVIVAL_MIRROR_RADIUS }
    }
    /// `PowerLevel.canReplaceBlocks` / `canBreakFar`: creative only.
    pub fn can_replace(&self) -> bool {
        self.creative
    }
    pub fn replace_mode(&self) -> ReplaceMode {
        if self.can_replace() { self.replace } else { ReplaceMode::OnlyAir }
    }
    pub fn replace_action(&self) -> Action {
        match self.replace_mode() {
            ReplaceMode::OnlyAir => Action::ReplaceOnlyAir,
            ReplaceMode::BlocksAndAir => Action::ReplaceBlocksAndAir,
            ReplaceMode::OnlyBlocks => Action::ReplaceOnlyBlocks,
            ReplaceMode::FilteredByOffhand => Action::ReplaceFilteredByOffhand,
        }
    }

    pub fn set_mode(&mut self, mode: BuildMode) {
        if mode != self.mode {
            self.previous_mode = self.mode;
        }
        self.mode = mode;
        self.reset_clicks();
        self.log(mode.name());
    }

    fn reset_clicks(&mut self) {
        self.clicks = 0;
        self.first = None;
        self.second = None;
    }

    fn pretend_state(&self) -> Building {
        if self.building != Building::Idle {
            return self.building;
        }
        match self.abilities {
            Abilities::CanPlaceAndBreak => Building::Placing,
            Abilities::CanBreak => Building::Breaking,
            Abilities::None => Building::Idle,
        }
    }

    fn start_pos(&self) -> Option<Pos> {
        if self.pretend_state() == Building::Breaking { self.start_break } else { self.start_place }
    }
}

// ---------------------------------------------------------------------------------------------
// Keys (ClientEvents.registerKeyMappings)

fn key(id: &str, label: &str, default: &str, modifiers: u32) -> KeyRegistration {
    let mut k = KeyRegistration::new(id, default);
    k.label = label.to_owned();
    k.category = "Effortless Building".to_owned();
    k.modifiers = ClientModifiers::new(modifiers);
    k
}

pub fn register_keys(r: &mut dyn Registrar) {
    r.register_key_mapping(key("radial", "Radial Menu", "key.keyboard.left.alt", 0));
    r.register_key_mapping(key("modifiers", "Modifier Menu", "key.keyboard.keypad.add", 0));
    r.register_key_mapping(key("undo", "Undo", "key.keyboard.z", 2));
    r.register_key_mapping(key("redo", "Redo", "key.keyboard.y", 2));
    r.register_key_mapping(key("previous", "Activate Previous Build Mode", "key.keyboard.unknown", 0));
    r.register_key_mapping(key("toggle", "Toggle Disabled <> Previous Build Mode", "key.keyboard.unknown", 0));
}

/// Whether the key bound to mod mapping `id` is physically held (`ClientEvents.isKeybindDown`).
pub fn key_held(host: &dyn ClientHost, id: &str) -> bool {
    match host.mod_key(MOD_ID, id) {
        Some((name, _)) => host.is_key_down(&name),
        None => false,
    }
}

pub fn ui_sound(host: &dyn ClientHost, id: &str, volume: f32, pitch: f32) {
    let mut s = SoundPlay::ui(id);
    s.volume = volume;
    s.pitch = pitch;
    let _ = host.play_local_sound(&s, 0);
}

fn block_sound(host: &dyn ClientHost, id: &str, at: [f64; 3], volume: f32, pitch: f32) {
    let mut s = SoundPlay::at(id, SoundSource::Blocks, at);
    s.volume = volume;
    s.pitch = pitch;
    let _ = host.play_local_sound(&s, 0);
}

// ---------------------------------------------------------------------------------------------
// The local player

struct PlayerView {
    player: PlayerRef,
    eye: Vec3,
    block: Pos,
    look: Vec3,
    yaw: f32,
    creative: bool,
    crouching: bool,
    held: Option<(ItemKey, BlockKey)>,
    offhand: Option<ItemKey>,
    counts: HashMap<ItemKey, u32>,
}

fn player_view(host: &dyn ClientHost) -> Option<PlayerView> {
    let player = host.local_player()?;
    let entity = player.entity();
    let eye = entity.eye_position(host)?;
    let cam = host.camera()?;
    let feet = [eye[0], eye[1] - cam.eye_height as f64, eye[2]];
    // On a client only the hands and the main inventory can be read.
    let living = mod_api::LivingRef::new(player.entity());
    let held = living
        .hand_item(host, Hand::Main)
        .and_then(|st| host.item_block(st.item()).map(|b| (st.item(), b)));
    let off = living.hand_item(host, Hand::Off);
    let offhand = off.as_ref().map(|st| st.item());
    let mut counts = HashMap::new();
    for st in player.inventory(host).unwrap_or_default().iter().chain(off.iter()) {
        *counts.entry(st.item()).or_insert(0) += st.count();
    }
    Some(PlayerView {
        player,
        eye,
        block: modes::containing(feet),
        look: modes::look_vec(cam.yaw, cam.pitch),
        yaw: cam.yaw,
        creative: player.is_creative(host).unwrap_or(false),
        crouching: entity.is_crouching(host).unwrap_or(false),
        held,
        offhand,
        counts,
    })
}

fn face_of(hit: &Hit) -> Pos {
    let d = [
        hit.x - (hit.block_x as f64 + 0.5),
        hit.y - (hit.block_y as f64 + 0.5),
        hit.z - (hit.block_z as f64 + 0.5),
    ];
    let axis = (0..3).max_by(|a, b| d[*a].abs().total_cmp(&d[*b].abs())).unwrap_or(1);
    let mut f = [0; 3];
    f[axis] = if d[axis] >= 0.0 { 1 } else { -1 };
    f
}

fn hit_pos(hit: &Hit) -> Pos {
    [hit.block_x, hit.block_y, hit.block_z]
}

fn state_name(host: &dyn ClientHost, s: BlockState) -> String {
    host.block_state_string(s).unwrap_or_default()
}

fn is_air(host: &dyn ClientHost, s: BlockState) -> bool {
    let n = state_name(host, s);
    n == "minecraft:air" || n == "minecraft:cave_air" || n == "minecraft:void_air"
}

/// `ClientBlockUtilities.determineIfLookingAtInteractiveObject`, by what the block is.
fn is_interactive(host: &dyn ClientHost, s: BlockState) -> bool {
    if host.block_states().has_block_entity(s) == Some(true) {
        return true;
    }
    let name = state_name(host, s);
    let name = name.split('[').next().unwrap_or("");
    [
        "door", "gate", "button", "lever", "crafting_table", "anvil", "_bed", "bell", "repeater", "comparator",
        "loom", "stonecutter", "grindstone", "cartography_table", "smithing_table", "note_block", "daylight_detector",
    ]
    .iter()
    .any(|k| name.contains(k))
}

// ---------------------------------------------------------------------------------------------
// Block states for placement (BlockEntry.setItemAndFindNewBlockState, by properties)

const HORIZONTAL: [&str; 4] = ["south", "west", "north", "east"];

fn opposite(d: &str) -> &'static str {
    match d {
        "north" => "south",
        "south" => "north",
        "east" => "west",
        "west" => "east",
        "up" => "down",
        _ => "up",
    }
}

fn face_name(f: Pos) -> &'static str {
    match f {
        [1, _, _] => "east",
        [-1, _, _] => "west",
        [_, 1, _] => "up",
        [_, -1, _] => "down",
        [_, _, 1] => "south",
        _ => "north",
    }
}

/// The state `block` takes placed at `entry` by a player facing `yaw`, clicking `face` at height
/// `hit_y` (0..1) of the clicked block, turned and mirrored as the entry says.
fn placement_state(host: &dyn ClientHost, block: BlockKey, yaw: f32, face: Pos, hit_y: f64, e: &Entry) -> Option<BlockState> {
    let default = host.block_state_default(block)?;
    let text = host.block_state_string(default)?;
    let Some(open) = text.find('[') else { return Some(default) };
    let name = &text[..open];
    let mut props: Vec<(String, String)> = text[open + 1..text.len() - 1]
        .split(',')
        .filter_map(|kv| kv.split_once('=').map(|(k, v)| (k.to_owned(), v.to_owned())))
        .collect();

    // Player.getDirection, turned by the entry's rotation and mirrors.
    let mut dir = HORIZONTAL[(((yaw / 90.0) + 0.5).floor() as i32 & 3) as usize];
    for _ in 0..e.rotation {
        dir = match dir {
            "north" => "east",
            "east" => "south",
            "south" => "west",
            _ => "north",
        };
    }
    if (e.mirror[0] && (dir == "east" || dir == "west")) || (e.mirror[2] && (dir == "north" || dir == "south")) {
        dir = opposite(dir);
    }
    let clicked = face_name(face);
    let mut top = clicked == "down" || (clicked != "up" && hit_y > 0.5);
    if e.mirror[1] {
        top = !top;
    }
    let stairs_like = name.ends_with("_stairs");
    let six_way = props.iter().any(|(k, _)| k == "facing")
        && host.block_state_id(&format!("{name}[facing=up]")).is_some();
    for (k, v) in props.iter_mut() {
        match k.as_str() {
            "facing" => {
                *v = if six_way {
                    clicked.to_owned()
                } else if stairs_like {
                    dir.to_owned()
                } else {
                    opposite(dir).to_owned()
                };
            }
            "axis" => {
                *v = match face {
                    [x, _, _] if x != 0 => "x",
                    [_, y, _] if y != 0 => "y",
                    _ => "z",
                }
                .to_owned();
            }
            "half" if v == "bottom" || v == "top" => *v = if top { "top" } else { "bottom" }.to_owned(),
            "type" if v == "bottom" || v == "top" => *v = if top { "top" } else { "bottom" }.to_owned(),
            _ => {}
        }
    }
    let joined: Vec<String> = props.iter().map(|(k, v)| format!("{k}={v}")).collect();
    host.block_state_id(&format!("{name}[{}]", joined.join(","))).or(Some(default))
}

// ---------------------------------------------------------------------------------------------
// The tick (ClientEvents.onClientTickPre)

pub fn tick(host: &mut dyn ClientHost) {
    // Keys and screens first: they need the mutable host.
    let radial_click = host.is_down(MOD_ID, "radial");
    let modifiers_key = host.consume_click(MOD_ID, "modifiers");
    let undo = host.consume_click(MOD_ID, "undo");
    let redo = host.consume_click(MOD_ID, "redo");
    let previous = host.consume_click(MOD_ID, "previous");
    let toggle = host.consume_click(MOD_ID, "toggle");
    let radial_held = key_held(&*host, "radial");
    let mod_screen_open = host.mod_screen() == Some(MOD_ID);
    let any_screen = host.observed_screen_id().is_some();

    let Some(pv) = player_view(&*host) else { return };
    let mut actions: Vec<Action> = Vec::new();
    let mut open: Option<Screen> = None;
    let mut close = false;
    {
        let mut st = STATE.lock().unwrap();
        st.creative = pv.creative;
        st.player_block = pv.block;
        if !host.is_paused() {
            st.ticks += 1;
        }
        // A screen of ours that Escape (or another screen) closed.
        if st.screen.is_some() && !mod_screen_open {
            if st.screen == Some(Screen::Radial) {
                ui::radial_closed(&mut st, &*host);
            }
            st.screen = None;
        }
        for d in std::mem::take(&mut st.deferred) {
            match d {
                Deferred::Action(a) => actions.push(a),
                Deferred::CloseScreen => close = true,
            }
        }
        match st.screen {
            None if radial_click && !any_screen => open = Some(Screen::Radial),
            Some(Screen::Radial) if !radial_held => {
                ui::radial_closed(&mut st, &*host);
                close = true;
            }
            _ => {}
        }
        if modifiers_key && !any_screen {
            open = Some(Screen::Modifiers);
        }
        if undo {
            actions.push(Action::Undo);
        }
        if redo {
            actions.push(Action::Redo);
        }
        if previous {
            actions.push(Action::PreviousBuildMode);
        }
        if toggle {
            actions.push(Action::DisableBuildModeToggle);
        }
    }

    if close {
        host.mod_screen_close(MOD_ID);
        STATE.lock().unwrap().screen = None;
    }
    // Actions may open the modifier screen, after the radial menu closed.
    for a in actions {
        if a == Action::OpenModifierSettings {
            open = Some(Screen::Modifiers);
        } else {
            perform_action(host, a);
        }
    }
    if let Some(screen) = open {
        if host.mod_screen() == Some(MOD_ID) {
            host.mod_screen_close(MOD_ID);
        }
        if host.mod_screen_open(MOD_ID) {
            let mut st = STATE.lock().unwrap();
            st.screen = Some(screen);
            if screen == Screen::Radial {
                ui::radial_opened(&mut st);
            }
            cancel(&mut st, &*host);
        }
    }

    builder_tick(host, &pv);
    mouse_input(host, &pv);
    previews_tick(&*host);
}

/// `ModeOptions.performAction`.
pub fn perform_action(host: &mut dyn ClientHost, action: Action) {
    match action {
        Action::Undo | Action::Redo => {
            if let Some(token) = HISTORY.get() {
                let _ = token.send(host, &History { redo: action == Action::Redo });
            }
            return;
        }
        _ => {}
    }
    let mut st = STATE.lock().unwrap();
    match action {
        Action::PreviousBuildMode => {
            let prev = st.previous_mode;
            st.set_mode(prev);
            return;
        }
        Action::DisableBuildModeToggle => {
            if st.mode == BuildMode::Disabled {
                let m = st.before_disabled;
                st.set_mode(m);
            } else {
                st.before_disabled = st.mode;
                st.set_mode(BuildMode::Disabled);
            }
            return;
        }
        Action::ReplaceOnlyAir => st.replace = ReplaceMode::OnlyAir,
        Action::ReplaceBlocksAndAir => st.replace = ReplaceMode::BlocksAndAir,
        Action::ReplaceOnlyBlocks => st.replace = ReplaceMode::OnlyBlocks,
        Action::ReplaceFilteredByOffhand => st.replace = ReplaceMode::FilteredByOffhand,
        Action::ToggleProtectTileEntities => st.protect_tile_entities = !st.protect_tile_entities,
        Action::OpenModifierSettings => return,
        other => {
            st.opts.apply(other);
        }
    }
    st.log(action.name());
}

// ---------------------------------------------------------------------------------------------
// BuilderChain.onTick

fn builder_tick(host: &mut dyn ClientHost, pv: &PlayerView) {
    let near_range = if pv.creative { 5.0 } else { 4.5 };
    let near = host.raycast(near_range);
    let mut st = STATE.lock().unwrap();
    let st = &mut *st;
    let previous_coords = std::mem::take(&mut st.previous_coords);
    st.blocks.clear();
    st.blocks.limit = st.at_once();
    st.new_states.clear();
    st.invalid.clear();
    st.start_place = None;
    st.start_break = None;
    st.looking_near = near.clone();

    // determineAbilities
    let interactive = near
        .as_ref()
        .and_then(|h| pv.player.level(&*host).and_then(|l| l.block_state(&*host, hit_pos(h))))
        .map(|s| is_interactive(&*host, s))
        .unwrap_or(false);
    st.abilities = if interactive && !pv.crouching {
        Abilities::None
    } else if pv.held.is_none() {
        Abilities::CanBreak
    } else {
        Abilities::CanPlaceAndBreak
    };
    if st.abilities == Abilities::None {
        st.previous_coords = previous_coords;
        return;
    }

    let Some(level) = pv.player.level(&*host) else { return };
    let mode = st.mode;
    if st.building == Building::Idle {
        // findStartPosition
        let max_reach = st.placement_reach();
        let look_near = mode == BuildMode::Disabled || max_reach < 3;
        st.looking_at = if look_near { near.clone() } else { host.raycast(max_reach as f32) };
        let Some(hit) = st.looking_at.clone() else {
            st.abilities = Abilities::None;
            st.previous_coords = previous_coords;
            return;
        };
        let mut start = hit_pos(&hit);
        let out_of_reach = {
            let d = [start[0] - pv.block[0], start[1] - pv.block[1], start[2] - pv.block[2]];
            ((d[0] * d[0] + d[1] * d[1] + d[2] * d[2]) as i64) > (max_reach as i64 * max_reach as i64)
        };
        if !look_near && out_of_reach {
            st.abilities = Abilities::None;
            st.previous_coords = previous_coords;
            return;
        }
        st.start_break = Some(start);
        if st.abilities == Abilities::CanPlaceAndBreak {
            let quick_replacing = st.replace_mode() != ReplaceMode::OnlyAir;
            let replaceable = level
                .block_state(&*host, start)
                .and_then(|s| host.block_states().replaceable(s))
                .unwrap_or(false);
            if !quick_replacing && !replaceable {
                let f = face_of(&hit);
                start = [start[0] + f[0], start[1] + f[1], start[2] + f[2]];
            }
        } else if !look_near && !st.can_replace() {
            // Only creative breaks far.
            st.abilities = Abilities::None;
            st.previous_coords = previous_coords;
            return;
        }
        st.start_place = Some(start);
        st.blocks.set_start(start);
    }

    // BuildModes.findCoordinates
    find_coordinates(st, pv);
    // BuildModifiers.findCoordinates
    let mods = st.modifiers.clone();
    for m in &mods {
        m.apply(&mut st.blocks);
    }
    // BuilderFilter.filterOnCoordinates
    let border = level.world_border(&*host);
    st.blocks.retain(|e| {
        let inside = border.map_or(true, |[cx, cz, size, _]| {
            let half = size / 2.0;
            let (x, z) = (e.pos[0] as f64 + 0.5, e.pos[2] as f64 + 0.5);
            x >= cx - half && x <= cx + half && z >= cz - half && z <= cz + half
        });
        inside && level.is_loaded(&*host, e.pos)
    });
    if mode == BuildMode::Disabled && st.blocks.len() <= 1 {
        st.abilities = Abilities::None;
        st.previous_coords = st.blocks.coordinates();
        return;
    }

    // findExistingBlockStates + BuilderFilter.filterOnExistingBlockStates
    let placing = st.pretend_state() == Building::Placing;
    let replace = st.replace_mode();
    let protect = st.protect_tile_entities;
    let offhand_block = pv.offhand.and_then(|i| host.item_block(i));
    let states = host.block_states();
    let mut existing: HashMap<Pos, BlockState> = HashMap::new();
    st.blocks.retain(|e| {
        let Some(s) = level.block_state(&*host, e.pos) else { return false };
        existing.insert(e.pos, s);
        if protect && states.has_block_entity(s) == Some(true) {
            return false;
        }
        if placing && replace != ReplaceMode::FilteredByOffhand {
            let air = is_air(&*host, s);
            if !(replace == ReplaceMode::OnlyAir || replace == ReplaceMode::BlocksAndAir) && air {
                return false;
            }
            let replaceable = states.replaceable(s).unwrap_or(false);
            if !(replace == ReplaceMode::OnlyBlocks || replace == ReplaceMode::BlocksAndAir) && !replaceable {
                return false;
            }
        }
        if replace == ReplaceMode::FilteredByOffhand && Some(s.block()) != offhand_block {
            return false;
        }
        true
    });

    // findNewBlockStates (placing only), with ItemUsageTracker marking blocks it has no items for
    if st.building != Building::Breaking {
        if let (Some((item, block)), Some(hit)) = (pv.held, st.looking_at.clone()) {
            let face = face_of(&hit);
            let hit_y = hit.y - hit.block_y as f64;
            let mut used = 0u32;
            let have = if pv.creative { u32::MAX } else { pv.counts.get(&item).copied().unwrap_or(0) };
            let mut keep = Vec::new();
            for e in &st.blocks.entries {
                let new = placement_state(&*host, block, pv.yaw, face, hit_y, e);
                // PlaceChecker: nothing to do where the same state stands.
                if new.is_some() && existing.get(&e.pos) == new.as_ref() {
                    keep.push(false);
                    continue;
                }
                keep.push(true);
                used += 1;
                st.new_states.push(new);
                st.invalid.push(used > have);
            }
            let mut i = 0;
            st.blocks.retain(|_| {
                let k = keep[i];
                i += 1;
                k
            });
        } else {
            st.new_states = vec![None; st.blocks.len()];
            st.invalid = vec![false; st.blocks.len()];
        }
    } else {
        st.new_states = vec![None; st.blocks.len()];
        st.invalid = vec![false; st.blocks.len()];
    }

    // onBlocksChanged: a place (or break) sound at most once a tick.
    let coords = st.blocks.coordinates();
    let item = pv.held.map(|h| h.0);
    if (st.previous_item != item || previous_coords != coords) && st.blocks.len() > 1 && st.sound_time < st.ticks {
        st.sound_time = st.ticks;
        if let Some(Some(last)) = st.new_states.last() {
            if let Some(sound) = host.block_states().sound(*last) {
                let id = if st.building == Building::Breaking { sound.break_sound } else { sound.place };
                let at = [pv.block[0] as f64 + 0.5, pv.block[1] as f64 + 0.5, pv.block[2] as f64 + 0.5];
                block_sound(&*host, &id, at, 0.3, 0.8);
            }
        }
    }
    st.previous_item = item;
    st.previous_coords = coords;
}

/// TwoClicksBuildMode / ThreeClicksBuildMode.findCoordinates.
fn find_coordinates(st: &mut Client, pv: &PlayerView) {
    let mode = st.mode;
    if st.clicks == 0 || mode.clicks() == 1 {
        return;
    }
    let Some(first) = st.first else { return };
    let reach = st.build_mode_reach();
    let limit = st.axis_limit();
    let list = if mode.clicks() == 2 || st.clicks == 1 {
        let Some(second) = mode.second_pos(pv.eye, pv.look, first, reach) else { return };
        let second_l = modes::limit_axis(first, second, limit);
        st.blocks.clear();
        st.blocks.first = Some(first);
        st.blocks.last = Some(second);
        if mode.clicks() == 2 {
            mode.two_click_blocks(&st.opts, first, second_l)
        } else {
            mode.intermediate_blocks(&st.opts, first, second_l)
        }
    } else {
        let Some(second) = st.second else { return };
        let Some(third) = mode.third_pos(pv.eye, pv.look, second, reach) else { return };
        let second_l = modes::limit_axis(first, second, limit);
        let third_l = modes::limit_axis(first, third, limit);
        st.blocks.clear();
        st.blocks.first = Some(first);
        st.blocks.last = Some(third);
        mode.final_blocks(&st.opts, limit, first, second_l, third_l)
    };
    for p in list {
        st.blocks.add(Entry::new(p));
    }
}

/// `IBuildMode.onClick`: true when the blocks should be placed (or broken) now.
fn on_click(st: &mut Client, pv: &PlayerView) -> bool {
    let mode = st.mode;
    if mode.clicks() == 1 {
        return true;
    }
    st.clicks += 1;
    if st.clicks == 1 {
        st.first = st.start_pos();
        if st.first.is_none() {
            st.clicks = 0;
        }
        return false;
    }
    if mode.clicks() == 2 {
        st.clicks = 0;
        return true;
    }
    if st.clicks == 2 {
        if st.blocks.is_empty() {
            st.clicks = 0;
            return false;
        }
        let first = st.first.unwrap_or([0; 3]);
        st.second = mode.second_pos(pv.eye, pv.look, first, st.build_mode_reach());
        return false;
    }
    st.clicks = 0;
    true
}

fn cancel(st: &mut Client, host: &dyn ClientHost) {
    if st.building == Building::Idle {
        return;
    }
    st.building = Building::Idle;
    st.reset_clicks();
    ui_sound(host, "minecraft:ui.toast.out", 4.0, 1.0);
}

/// ClientEvents.onMouseInput: right and left click, with the original's 4-tick repeat.
fn mouse_input(host: &mut dyn ClientHost, pv: &PlayerView) {
    if host.observed_screen_id().is_some() || STATE.lock().unwrap().screen.is_some() {
        return;
    }
    let use_down = host.mapping("key.use").ok().flatten().map_or(false, |m| m.down);
    let attack_down = host.mapping("key.attack").ok().flatten().map_or(false, |m| m.down);
    let (right, left) = {
        let mut st = STATE.lock().unwrap();
        let use_down = use_down || std::mem::take(&mut st.pressed_use);
        let attack_down = attack_down || std::mem::take(&mut st.pressed_attack);
        let single = st.mode == BuildMode::Single;
        let fast = st.opts.fast;
        let mut right = false;
        if use_down {
            if st.place_cooldown <= 0 {
                st.place_cooldown = 4;
                right = true;
            } else if single {
                st.place_cooldown -= 1;
                if fast {
                    st.place_cooldown = 0;
                }
            }
        } else {
            st.place_cooldown = 0;
        }
        let mut left = false;
        if attack_down {
            if st.break_cooldown <= 0 {
                st.break_cooldown = 4;
                left = true;
            } else if single {
                st.break_cooldown -= 1;
                if fast {
                    st.break_cooldown = 0;
                }
            }
        } else {
            st.break_cooldown = 0;
        }
        (right, left)
    };
    if right {
        on_right_click(host, pv);
    }
    if left {
        on_left_click(host, pv);
    }
}

fn on_right_click(host: &mut dyn ClientHost, pv: &PlayerView) {
    let send = {
        let mut st = STATE.lock().unwrap();
        let st = &mut *st;
        if st.abilities != Abilities::CanPlaceAndBreak || st.building == Building::Breaking {
            cancel(st, &*host);
            return;
        }
        if st.building == Building::Idle {
            st.building = Building::Placing;
        }
        if !on_click(st, pv) {
            return;
        }
        st.building = Building::Idle;
        if st.blocks.is_empty() {
            return;
        }
        let mode = st.mode;
        let delay = if st.blocks.len() > 1 { APPEAR_ANIMATION as i32 } else { 0 };
        on_blocks_sent(st, false);
        let mut palette: Vec<String> = Vec::new();
        let mut index: HashMap<BlockState, i32> = HashMap::new();
        let mut blocks = Vec::with_capacity(st.blocks.len() * 4);
        for (e, s) in st.blocks.entries.iter().zip(st.new_states.iter()) {
            let Some(s) = s else { continue };
            let i = *index.entry(*s).or_insert_with(|| {
                palette.push(state_name(&*host, *s));
                palette.len() as i32 - 1
            });
            blocks.extend_from_slice(&[e.pos[0], e.pos[1], e.pos[2], i]);
        }
        PlaceBlocks { breaking: false, delay, skip_first: mode == BuildMode::Disabled, palette, blocks }
    };
    if send.blocks.is_empty() {
        return;
    }
    if let Some(token) = PLACE.get() {
        let _ = token.send(host, &send);
    }
}

fn on_left_click(host: &mut dyn ClientHost, pv: &PlayerView) {
    let send = {
        let mut st = STATE.lock().unwrap();
        let st = &mut *st;
        if st.abilities == Abilities::None || st.building == Building::Placing {
            cancel(st, &*host);
            return;
        }
        if !st.can_replace() {
            return;
        }
        if st.building == Building::Idle {
            st.building = Building::Breaking;
            // The tick assumed placing: start again from the block looked at.
            let Some(start) = st.start_break else { return };
            st.blocks.set_start(start);
            let mods = st.modifiers.clone();
            for m in &mods {
                m.apply(&mut st.blocks);
            }
            if let Some(level) = pv.player.level(&*host) {
                let protect = st.protect_tile_entities;
                let states = host.block_states();
                st.blocks.retain(|e| {
                    level.is_loaded(&*host, e.pos)
                        && level.block_state(&*host, e.pos).map_or(false, |s| {
                            !(protect && states.has_block_entity(s) == Some(true))
                        })
                });
            }
        }
        if !on_click(st, pv) {
            return;
        }
        st.building = Building::Idle;
        if st.blocks.is_empty() {
            return;
        }
        let mode = st.mode;
        on_blocks_sent(st, true);
        let mut blocks = Vec::with_capacity(st.blocks.len() * 4);
        for e in &st.blocks.entries {
            blocks.extend_from_slice(&[e.pos[0], e.pos[1], e.pos[2], -1]);
        }
        PlaceBlocks { breaking: true, delay: 0, skip_first: mode == BuildMode::Disabled, palette: Vec::new(), blocks }
    };
    if let Some(token) = PLACE.get() {
        let _ = token.send(host, &send);
    }
}

/// BlockPreviews.onBlocksPlaced / onBlocksBroken: keep them for the animation.
fn on_blocks_sent(st: &mut Client, breaking: bool) {
    let n = st.blocks.len();
    if n <= 1 || n > MAX_BLOCK_PREVIEWS {
        return;
    }
    let blocks = st
        .blocks
        .entries
        .iter()
        .zip(st.new_states.iter())
        .map(|(e, s)| (e.pos, *s))
        .collect();
    let first = st.blocks.first.unwrap_or(st.blocks.entries[0].pos);
    let last = st.blocks.last.unwrap_or(first);
    let time = st.ticks;
    st.placed.push(Placed { time, breaking, blocks, first, last });
}

/// The action bar's block count while building (BlockPreviews.drawLookAtPreview).
fn previews_tick(_host: &dyn ClientHost) {
    let mut st = STATE.lock().unwrap();
    let now = st.ticks;
    st.placed.retain(|p| p.time + if p.breaking { BREAK_ANIMATION } else { APPEAR_ANIMATION } >= now);
    if st.pretend_state() == Building::Idle || st.building == Building::Idle || st.blocks.is_empty() {
        return;
    }
    let mut lo = [i32::MAX; 3];
    let mut hi = [i32::MIN; 3];
    for e in &st.blocks.entries {
        for i in 0..3 {
            lo[i] = lo[i].min(e.pos[i]);
            hi[i] = hi[i].max(e.pos[i]);
        }
    }
    let dim = [hi[0] - lo[0] + 1, hi[1] - lo[1] + 1, hi[2] - lo[2] + 1];
    let mut d = String::from("(");
    for v in [dim[0], dim[2], dim[1]] {
        if v > 1 {
            d.push_str(&format!("{v}x"));
        }
    }
    d.pop();
    if d.len() > 1 {
        d.push(')');
    }
    let msg = format!("{} blocks {}", st.blocks.len(), d);
    st.log(msg);
}

// ---------------------------------------------------------------------------------------------
// Vanilla's own use and attack, held back while the mod builds

pub fn interact(host: &mut dyn ClientHost, event: &mut InteractionKeyMappingTriggeredEvent) -> Flow {
    if host.observed_screen_id().is_some() {
        return Flow::Continue;
    }
    let mut st = STATE.lock().unwrap();
    if st.screen.is_some() {
        return Flow::Cancel;
    }
    if event.attempt == InteractionAttempt::Press {
        match event.key {
            InteractionKey::Use if event.hand == Hand::Main => st.pressed_use = true,
            InteractionKey::Attack => st.pressed_attack = true,
            _ => {}
        }
    }
    match event.key {
        InteractionKey::Use if event.hand == Hand::Main || event.hand == Hand::Off => {
            if st.mode != BuildMode::Disabled && st.abilities == Abilities::CanPlaceAndBreak {
                return Flow::Cancel;
            }
        }
        InteractionKey::Attack => {
            if st.mode != BuildMode::Disabled && st.can_replace() && st.abilities != Abilities::None {
                return Flow::Cancel;
            }
        }
        _ => {}
    }
    Flow::Continue
}

// ---------------------------------------------------------------------------------------------
// World drawing (BlockPreviews, Outliner clusters, ModifierRenderer)

/// Vanilla `MapColor` colours by id, for the ghost blocks.
const MAP_COLORS: [u32; 62] = [
    0x000000, 0x7FB238, 0xF7E9A3, 0xC7C7C7, 0xFF0000, 0xA0A0FF, 0xA7A7A7, 0x007C00, 0xFFFFFF, 0xA4A8B8, 0x976D4D,
    0x707070, 0x4040FF, 0x8F7748, 0xFFFCF5, 0xD87F33, 0xB24CD8, 0x6699D8, 0xE5E533, 0x7FCC19, 0xF27FA5, 0x4C4C4C,
    0x999999, 0x4C7F99, 0x7F3FB2, 0x334CB2, 0x664C33, 0x667F33, 0x993333, 0x191919, 0xFAEE4D, 0x5CDBD5, 0x4A80FF,
    0x00D93A, 0x815631, 0x700200, 0xD1B1A1, 0x9F5224, 0x95576C, 0x706C8A, 0xBA8524, 0x677535, 0xA04D4E, 0x392923,
    0x876B62, 0x575C5C, 0x7A4958, 0x4C3E5C, 0x4C3223, 0x4C522A, 0x8E3C2E, 0x251610, 0xBD3031, 0x943F61, 0x5C191D,
    0x167E86, 0x3A8E8C, 0x562C3E, 0x14B485, 0x646464, 0xD8AF93, 0x7FA796,
];

fn rgb_of(host: &dyn ClientHost, cache: &mut HashMap<BlockState, u32>, s: BlockState) -> u32 {
    *cache.entry(s).or_insert_with(|| {
        let id = host.block_states().map_color(s).unwrap_or(11) as usize;
        let c = MAP_COLORS.get(id).copied().unwrap_or(0x707070);
        if c == 0 { 0xC0C0C0 } else { c }
    })
}

fn gain(x: f32, k: f32) -> f32 {
    let a = 0.5 * (2.0 * if x < 0.5 { x } else { 1.0 - x }).powf(k);
    if x < 0.5 { a } else { 1.0 - a }
}

/// A box's six faces, each corner list counterclockwise seen from outside.
fn cube_faces(lo: Vec3, hi: Vec3) -> [([[f64; 3]; 4], [f64; 3]); 6] {
    let [x0, y0, z0] = lo;
    let [x1, y1, z1] = hi;
    [
        ([[x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]], [-1.0, 0.0, 0.0]),
        ([[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]], [1.0, 0.0, 0.0]),
        ([[x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]], [0.0, -1.0, 0.0]),
        ([[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]], [0.0, 1.0, 0.0]),
        ([[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]], [0.0, 0.0, -1.0]),
        ([[x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]], [0.0, 0.0, 1.0]),
    ]
}

/// A ghost of the block to be placed, `scale` of a block around its centre, facing the camera.
fn ghost(quads: &mut Vec<WorldQuad>, cam: Vec3, p: Pos, scale: f64, argb: u32) {
    let c = [p[0] as f64 + 0.5, p[1] as f64 + 0.5, p[2] as f64 + 0.5];
    let h = scale / 2.0;
    let lo = [c[0] - h, c[1] - h, c[2] - h];
    let hi = [c[0] + h, c[1] + h, c[2] + h];
    for (corners, n) in cube_faces(lo, hi) {
        let to_cam = [cam[0] - c[0], cam[1] - c[1], cam[2] - c[2]];
        if n[0] * to_cam[0] + n[1] * to_cam[1] + n[2] * to_cam[2] > 0.0 {
            quads.push(WorldQuad::new(corners, argb, WorldDepth::Tested));
        }
    }
}

/// `Outliner.showCluster`: faint faces and an outline round the whole set of blocks.
fn cluster(quads: &mut Vec<WorldQuad>, lines: &mut Vec<WorldLine>, set: &HashSet<Pos>, face: u32, line: u32, width: f32) {
    let eps = 0.002;
    for p in set {
        for axis in 0..3 {
            for dir in [-1, 1] {
                let mut n = *p;
                n[axis] += dir;
                if set.contains(&n) {
                    continue;
                }
                let (u, v) = match axis {
                    0 => (1, 2),
                    1 => (0, 2),
                    _ => (0, 1),
                };
                let plane = p[axis] as f64 + if dir > 0 { 1.0 + eps } else { -eps };
                let at = |du: f64, dv: f64| {
                    let mut c = [0.0; 3];
                    c[axis] = plane;
                    c[u] = p[u] as f64 + du;
                    c[v] = p[v] as f64 + dv;
                    c
                };
                if face != 0 {
                    quads.push(WorldQuad::new(
                        [at(0.0, 0.0), at(1.0, 0.0), at(1.0, 1.0), at(0.0, 1.0)],
                        face,
                        WorldDepth::Tested,
                    ));
                }
                // An edge of this face is on the outline unless the neighbour across it shows the same face.
                let edges = [
                    (v, -1, at(0.0, 0.0), at(1.0, 0.0)),
                    (v, 1, at(0.0, 1.0), at(1.0, 1.0)),
                    (u, -1, at(0.0, 0.0), at(0.0, 1.0)),
                    (u, 1, at(1.0, 0.0), at(1.0, 1.0)),
                ];
                for (along, step, s, e) in edges {
                    let mut q = *p;
                    q[along] += step;
                    let mut q_out = q;
                    q_out[axis] += dir;
                    if set.contains(&q) && !set.contains(&q_out) {
                        continue;
                    }
                    lines.push(WorldLine::new(s, e, line, width, WorldDepth::Tested));
                }
            }
        }
    }
}

const COLOR_X: u32 = 0xFF4834;
const COLOR_Y: u32 = 0x43CC33;
const COLOR_Z: u32 = 0x34F7FF;

fn plane(quads: &mut Vec<WorldQuad>, a: Vec3, b: Vec3, y_plane: bool, rgb: u32) {
    let c = (50u32 << 24) | rgb;
    let corners = if y_plane {
        [[a[0], a[1], a[2]], [a[0], a[1], b[2]], [b[0], a[1], b[2]], [b[0], a[1], a[2]]]
    } else {
        [[a[0], a[1], a[2]], [a[0], b[1], a[2]], [b[0], b[1], b[2]], [b[0], a[1], b[2]]]
    };
    quads.push(WorldQuad::new(corners, c, WorldDepth::Tested));
    let mut back = corners;
    back.reverse();
    quads.push(WorldQuad::new(back, c, WorldDepth::Tested));
}

fn modifier_planes(st: &Client, quads: &mut Vec<WorldQuad>, lines: &mut Vec<WorldLine>) {
    let line = |lines: &mut Vec<WorldLine>, a: Vec3, b: Vec3, rgb: u32| {
        lines.push(WorldLine::new(a, b, (200u32 << 24) | rgb, 2.0, WorldDepth::Tested));
    };
    for m in &st.modifiers {
        match m {
            Modifier::Mirror(m) if m.enabled && m.axes.iter().any(|a| *a) => {
                let p = [m.position[0] + 0.001, m.position[1] + 0.001, m.position[2] + 0.001];
                let r = m.radius as f64;
                let mut vertical = |a: Vec3, b: Vec3, rgb: u32| {
                    if m.draw_planes {
                        plane(quads, a, b, false, rgb);
                    }
                    if m.draw_lines {
                        let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0, (a[2] + b[2]) / 2.0];
                        line(lines, [a[0], mid[1], a[2]], [b[0], mid[1], b[2]], rgb);
                        line(lines, [mid[0], a[1], mid[2]], [mid[0], b[1], mid[2]], rgb);
                    }
                };
                if m.axes[0] {
                    vertical([p[0], p[1] - r, p[2] - r], [p[0], p[1] + r, p[2] + r], COLOR_X);
                }
                if m.axes[2] {
                    vertical([p[0] - r, p[1] - r, p[2]], [p[0] + r, p[1] + r, p[2]], COLOR_Z);
                }
                if m.axes[1] {
                    let (a, b) = ([p[0] - r, p[1], p[2] - r], [p[0] + r, p[1], p[2] + r]);
                    if m.draw_planes {
                        plane(quads, a, b, true, COLOR_Y);
                    }
                    if m.draw_lines {
                        line(lines, [p[0], p[1], a[2]], [p[0], p[1], b[2]], COLOR_Y);
                        line(lines, [a[0], p[1], p[2]], [b[0], p[1], p[2]], COLOR_Y);
                    }
                }
                let n = m.axes.iter().filter(|a| **a).count();
                if m.draw_lines && n >= 2 {
                    line(lines, [p[0] - r, p[1], p[2]], [p[0] + r, p[1], p[2]], COLOR_X);
                    line(lines, [p[0], p[1] - r, p[2]], [p[0], p[1] + r, p[2]], COLOR_Y);
                    line(lines, [p[0], p[1], p[2] - r], [p[0], p[1], p[2] + r], COLOR_Z);
                }
            }
            Modifier::RadialMirror(rm) if rm.enabled => {
                let p = [rm.position[0] + 0.001, rm.position[1] + 0.001, rm.position[2] + 0.001];
                let r = rm.radius as f64;
                let angle = 2.0 * std::f32::consts::PI / rm.slices.max(1) as f32;
                let rot = |v: Vec3, a: f32| {
                    let (c, s) = (a.cos() as f64, a.sin() as f64);
                    [v[0] * c + v[2] * s, v[1], v[2] * c - v[0] * s]
                };
                let mut start = [r, 0.0, 0.0];
                if rm.slices % 4 == 2 {
                    start = rot(start, angle / 2.0);
                }
                for i in 0..rm.slices {
                    let v = rot(start, angle * i as f32);
                    let a = [p[0], p[1] - r, p[2]];
                    let b = [p[0] + v[0], p[1] + r, p[2] + v[2]];
                    if rm.draw_planes {
                        plane(quads, a, b, false, COLOR_Z);
                    }
                    if rm.draw_lines {
                        line(lines, [a[0], p[1], a[2]], [b[0], p[1], b[2]], COLOR_Z);
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn render(host: &dyn ClientHost, draw: &mut dyn WorldDrawContext) {
    let Some(cam) = host.camera() else { return };
    let cam = cam.position;
    let mut st = STATE.lock().unwrap();
    let st = &mut *st;
    let mut quads = Vec::new();
    let mut lines = Vec::new();
    let budget = 3800usize;

    // Placed and broken blocks, animating.
    let now = st.ticks as f32;
    let placed: Vec<(f32, bool, Vec<(Pos, Option<BlockState>)>, Pos, Pos)> =
        st.placed.iter().map(|p| (p.time as f32, p.breaking, p.blocks.clone(), p.first, p.last)).collect();
    for (time, breaking, blocks, first, last) in placed {
        let total = if breaking { BREAK_ANIMATION } else { APPEAR_ANIMATION } as f32;
        let dissolve = (now - time) / total;
        let span = dist_sq(first, last);
        for (p, s) in blocks {
            let place = if span > 0.5 {
                let a = dist_sq(first, p) / span;
                let b = dist_sq(last, p) / span;
                (a + (1.0 - b)) / 2.0
            } else {
                0.0
            };
            let place = place * (1.0 - 0.8);
            let t = gain(((dissolve - place as f32) / 0.8).clamp(0.0, 1.0), 0.5);
            let (scale, alpha) = if breaking {
                let t = 1.0 - t;
                (0.5 + t * 0.5, 0.7 + t * 0.3)
            } else {
                (0.5 + t * 0.3, 0.7 + t * 0.3)
            };
            let rgb = match s {
                Some(s) => rgb_of(host, &mut st.map_colors, s),
                None => 0xFFFFFF,
            };
            let a = ((alpha.clamp(0.0, 1.0) * 255.0) as u32) << 24;
            if quads.len() < budget {
                ghost(&mut quads, cam, p, scale as f64, a | rgb);
            }
        }
    }

    // What the next click builds.
    let mode = st.mode;
    let n = st.blocks.len();
    let show = n > 0
        && st.abilities != Abilities::None
        && !(mode == BuildMode::Disabled && n == 1)
        && !(st.building == Building::Idle && n == 1);
    if show {
        let set = st.blocks.coordinates();
        if st.pretend_state() != Building::Breaking {
            if n < MAX_BLOCK_PREVIEWS {
                let entries: Vec<(Pos, Option<BlockState>, bool)> = st
                    .blocks
                    .entries
                    .iter()
                    .zip(st.new_states.iter().zip(st.invalid.iter()))
                    .map(|(e, (s, inv))| (e.pos, *s, *inv))
                    .collect();
                for (p, s, invalid) in entries {
                    let rgb = if invalid {
                        0xFF0000
                    } else {
                        s.map(|s| rgb_of(host, &mut st.map_colors, s)).unwrap_or(0xFFFFFF)
                    };
                    if quads.len() < budget {
                        ghost(&mut quads, cam, p, 0.5, 0xB3000000 | rgb);
                    }
                }
                cluster(&mut quads, &mut lines, &set, 0x18FFFFFF, 0xFFFFFFFF, 1.0);
            } else {
                cluster(&mut quads, &mut lines, &set, 0x30FFFFFF, 0xFFFFFFFF, 2.0);
            }
        } else {
            cluster(&mut quads, &mut lines, &set, 0x30CC1A1A, 0xFFCC1A1A, 2.0);
        }
    }

    // drawOutlineAtBreakPosition: a thin box where a far break would start.
    if mode != BuildMode::Disabled && st.abilities != Abilities::None && st.looking_near.is_none() {
        if let Some(p) = st.start_break {
            let set: HashSet<Pos> = [p].into_iter().collect();
            cluster(&mut Vec::new(), &mut lines, &set, 0, 0xFF222222, 1.0);
        }
    }

    modifier_planes(st, &mut quads, &mut lines);
    quads.truncate(4000);
    let _ = draw.quads(&quads);
    let _ = draw.lines(&lines);
}

fn dist_sq(a: Pos, b: Pos) -> f64 {
    let d = [(a[0] - b[0]) as f64, (a[1] - b[1]) as f64, (a[2] - b[2]) as f64];
    d[0] * d[0] + d[1] * d[1] + d[2] * d[2]
}

// ---------------------------------------------------------------------------------------------
// The action bar line (vanilla's overlay message: 60 ticks, fading over the last 20)

pub fn hud(host: &dyn ClientHost, draw: &mut dyn mod_api::DrawContext) {
    let st = STATE.lock().unwrap();
    let Some((msg, at)) = &st.action_bar else { return };
    let age = st.ticks.saturating_sub(*at) as f32;
    let left = 60.0 - age;
    if left <= 0.0 {
        return;
    }
    let alpha = ((left * 255.0 / 20.0) as u32).min(255);
    if alpha <= 8 {
        return;
    }
    let [w, h] = draw.size();
    let tw = draw.text_width(msg);
    let _ = host;
    draw.text(msg, [(w - tw) / 2.0, h - 72.0], (alpha << 24) | 0xFFFFFF, true);
}
