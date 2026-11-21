//! Server side, after Effortless Building's `ServerBlockPlacer` and `UndoRedo` (Requios, LGPL-3.0):
//! checks a block set from the client, applies it after its appear delay, keeps undo/redo.
use crate::proto::{History, PlaceBlocks};
use mod_api::{
    BlockState, IPayloadContext, ItemKey, ItemStack, LevelRef, PlayerRef, ServerHost, ServerTickEvent, ServerTickPhase,
    Text,
};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

/// `ServerConfig.validation.maxBlocksPlacedAtOnce`.
pub const MAX_BLOCKS_PLACED_AT_ONCE: usize = 10_000;
/// `ServerConfig.memory.undoStackSize`.
const UNDO_STACK_SIZE: usize = 50;

#[derive(Clone)]
struct Change {
    pos: [i32; 3],
    /// What stood there before.
    old: BlockState,
    /// What the change put there (air when it broke a block).
    new: BlockState,
}

struct Pending {
    player: PlayerRef,
    ticks: i32,
    req: PlaceBlocks,
}

#[derive(Default)]
struct Stacks {
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
}

static PENDING: LazyLock<Mutex<Vec<Pending>>> = LazyLock::new(Default::default);
static STACKS: LazyLock<Mutex<HashMap<u128, Stacks>>> = LazyLock::new(Default::default);

fn red(host: &mut dyn ServerHost, player: PlayerRef, msg: &str) {
    let _ = player.tell(host, Text::literal(format!("\u{a7}c{msg}")));
}

fn push_fixed(stack: &mut Vec<Vec<Change>>, set: Vec<Change>) {
    if set.is_empty() {
        return;
    }
    stack.push(set);
    if stack.len() > UNDO_STACK_SIZE {
        stack.remove(0);
    }
}

pub fn is_air(host: &dyn ServerHost, state: BlockState) -> bool {
    host.block_state_string(state)
        .map(|s| matches!(s.as_str(), "minecraft:air" | "minecraft:cave_air" | "minecraft:void_air"))
        .unwrap_or(false)
}

pub fn handle_place(req: PlaceBlocks, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let player = cx.player().map_err(|e| format!("{e:?}"))?;
    let host = cx.server_host().map_err(|e| format!("{e:?}"))?;
    if !allowed(host, player) || !validate(host, player, &req) {
        return Ok(());
    }
    if req.breaking || req.delay <= 0 {
        apply(host, player, &req);
    } else {
        PENDING.lock().unwrap().push(Pending { player, ticks: req.delay, req });
    }
    Ok(())
}

/// Places the block sets whose appear animation has run.
pub fn tick(host: &mut dyn ServerHost, event: &ServerTickEvent<'_>) {
    if !matches!(event.phase, ServerTickPhase::ServerTickStart { .. }) {
        return;
    }
    let due: Vec<Pending> = {
        let mut pending = PENDING.lock().unwrap();
        if pending.is_empty() {
            return;
        }
        for p in pending.iter_mut() {
            p.ticks -= 1;
        }
        let (due, wait): (Vec<_>, Vec<_>) = pending.drain(..).partition(|p| p.ticks <= 0);
        *pending = wait;
        due
    };
    for p in due {
        apply(host, p.player, &p.req);
    }
}

fn allowed(host: &mut dyn ServerHost, player: PlayerRef) -> bool {
    if player.may_build(&*host) == Some(false) {
        red(host, player, "You are not allowed to build.");
        return false;
    }
    true
}

fn validate(host: &mut dyn ServerHost, player: PlayerRef, req: &PlaceBlocks) -> bool {
    let n = req.blocks.len() / 4;
    if n == 0 {
        red(host, player, "No blocks to place.");
        return false;
    }
    if req.skip_first && n == 1 {
        red(host, player, "No blocks to place because the first block was skipped.");
        return false;
    }
    if n > MAX_BLOCKS_PLACED_AT_ONCE {
        red(host, player, &format!("Too many blocks to place. Max: {MAX_BLOCKS_PLACED_AT_ONCE}"));
        return false;
    }
    true
}

/// Items in the player's inventory, by item.
fn inventory_counts(host: &dyn ServerHost, player: PlayerRef) -> HashMap<ItemKey, u32> {
    let mut counts = HashMap::new();
    for (slot, stack) in player.inventory_slots(host).unwrap_or_default() {
        if slot < 36 || slot == 40 {
            *counts.entry(stack.item()).or_insert(0) += stack.count();
        }
    }
    counts
}

/// `InventoryHelper.removeFromInventory`: the held slot first, then the rest.
fn remove_from_inventory(host: &mut dyn ServerHost, player: PlayerRef, item: ItemKey, mut amount: u32) {
    let selected = player.selected_slot(&*host).map(|s| s as u32);
    let mut slots = player.inventory_slots(&*host).unwrap_or_default();
    slots.sort_by_key(|(slot, _)| if Some(*slot) == selected { 0 } else { 1 + *slot });
    for (slot, stack) in slots {
        if amount == 0 {
            break;
        }
        if stack.item() != item || !(slot < 36 || slot == 40) {
            continue;
        }
        let take = amount.min(stack.count());
        let left = stack.count() - take;
        let new = if left == 0 { None } else { ItemStack::new(item, left) };
        if player.set_inventory_slot(host, slot, new).is_ok() {
            amount -= take;
        }
    }
}

/// Uses one of `item` from the tracked counts; false (and a note, once) when there is none left.
fn use_item(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    creative: bool,
    item: ItemKey,
    counts: &mut HashMap<ItemKey, u32>,
    used: &mut HashMap<ItemKey, u32>,
    warned: &mut Vec<ItemKey>,
) -> bool {
    if creative {
        return true;
    }
    let have = counts.get(&item).copied().unwrap_or(0);
    let spent = used.entry(item).or_insert(0);
    if *spent < have {
        *spent += 1;
        return true;
    }
    if !warned.contains(&item) {
        warned.push(item);
        // `Item.toString()`: the registry path.
        let name = host
            .item_block(item)
            .and_then(|b| host.block_state_default(b))
            .and_then(|s| host.block_state_string(s))
            .map(|s| s.split('[').next().unwrap_or("").rsplit(':').next().unwrap_or("").to_owned())
            .unwrap_or_else(|| "block".into());
        red(host, player, &format!("{name} not found in inventory."));
    }
    false
}

/// Places `state` or breaks what stands at `pos`; the change made, if any.
fn set_one(
    host: &mut dyn ServerHost,
    player: PlayerRef,
    level: LevelRef,
    pos: [i32; 3],
    target: BlockState,
    creative: bool,
) -> Option<Change> {
    if !level.is_loaded(&*host, pos) {
        return None;
    }
    let old = level.block_state(&*host, pos)?;
    if old == target {
        return None;
    }
    let ok = if is_air(&*host, target) {
        if is_air(&*host, old) {
            return None;
        }
        if creative {
            matches!(level.destroy_block(host, pos, false, None, false), Ok(_))
        } else {
            player.destroy_block(host, pos, true)
        }
    } else {
        player.place_block(host, level, pos, target, true).is_ok()
    };
    if !ok {
        return None;
    }
    let new = level.block_state(&*host, pos)?;
    Some(Change { pos, old, new })
}

fn apply(host: &mut dyn ServerHost, player: PlayerRef, req: &PlaceBlocks) {
    let Some(level) = player.level(&*host) else { return };
    let creative = player.is_creative(&*host).unwrap_or(false);
    let air = host.block_state_id("minecraft:air");
    let palette: Vec<Option<BlockState>> = req.palette.iter().map(|s| host.block_state_id(s)).collect();
    let mut counts = inventory_counts(&*host, player);
    let mut used: HashMap<ItemKey, u32> = HashMap::new();
    let mut warned = Vec::new();
    let mut changes = Vec::new();

    for (i, b) in req.blocks.chunks_exact(4).enumerate() {
        if req.skip_first && i == 0 {
            continue;
        }
        let pos = [b[0], b[1], b[2]];
        let target = if req.breaking || b[3] < 0 {
            match air {
                Some(a) => a,
                None => continue,
            }
        } else {
            match palette.get(b[3] as usize).copied().flatten() {
                Some(s) => s,
                None => continue,
            }
        };
        if !req.breaking {
            let Some(item) = host.block_item(target.block()) else { continue };
            // An occupied spot is the client's call (replace modes); the server only checks it is loaded.
            if !use_item(host, player, creative, item, &mut counts, &mut used, &mut warned) {
                continue;
            }
            match set_one(host, player, level, pos, target, creative) {
                Some(c) => changes.push(c),
                None => {
                    if let Some(n) = used.get_mut(&item) {
                        *n = n.saturating_sub(1);
                    }
                }
            }
        } else if let Some(c) = set_one(host, player, level, pos, target, creative) {
            changes.push(c);
        }
    }
    if !creative {
        for (item, n) in used {
            if n > 0 {
                remove_from_inventory(host, player, item, n);
            }
        }
    }
    let Some(id) = player.entity().uuid(host) else { return };
    let mut stacks = STACKS.lock().unwrap();
    let s = stacks.entry(id).or_default();
    push_fixed(&mut s.undo, changes);
    s.redo.clear();
}

pub fn handle_history(req: History, cx: &mut dyn IPayloadContext) -> Result<(), String> {
    let player = cx.player().map_err(|e| format!("{e:?}"))?;
    let host = cx.server_host().map_err(|e| format!("{e:?}"))?;
    let Some(id) = player.entity().uuid(host) else { return Ok(()) };
    let set = {
        let mut stacks = STACKS.lock().unwrap();
        let s = stacks.entry(id).or_default();
        if req.redo { s.redo.pop() } else { s.undo.pop() }
    };
    let Some(set) = set else { return Ok(()) };
    let Some(level) = player.level(&*host) else { return Ok(()) };
    let creative = player.is_creative(&*host).unwrap_or(false);
    let mut counts = inventory_counts(&*host, player);
    let mut used: HashMap<ItemKey, u32> = HashMap::new();
    let mut warned = Vec::new();
    let mut done = Vec::new();
    // Undo walks back from the last change; redo replays in order.
    let order: Vec<&Change> = if req.redo { set.iter().collect() } else { set.iter().rev().collect() };
    for c in order {
        let target = if req.redo { c.new } else { c.old };
        if !is_air(&*host, target) {
            let Some(item) = host.block_item(target.block()) else { continue };
            if !use_item(host, player, creative, item, &mut counts, &mut used, &mut warned) {
                continue;
            }
        }
        if let Some(applied) = set_one(host, player, level, c.pos, target, creative) {
            // Keep the original pair so the step can be flipped back again.
            done.push(Change { pos: c.pos, old: c.old, new: c.new });
            let _ = applied;
        }
    }
    if !creative {
        for (item, n) in used {
            if n > 0 {
                remove_from_inventory(host, player, item, n);
            }
        }
    }
    if !req.redo {
        done.reverse();
    }
    let mut stacks = STACKS.lock().unwrap();
    let s = stacks.entry(id).or_default();
    if req.redo {
        push_fixed(&mut s.undo, done);
    } else {
        push_fixed(&mut s.redo, done);
    }
    Ok(())
}

/// Forget a player's history when they leave or change dimension.
pub fn clear(host: &mut dyn ServerHost, uuid: u128) {
    STACKS.lock().unwrap().remove(&uuid);
    let player = host.player(uuid);
    PENDING.lock().unwrap().retain(|p| Some(p.player) != player);
}
