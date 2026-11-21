//! Client-only block placing: what the original's `ServerBlockPlacer` and `UndoRedo` do on the
//! server, done from the client so the mod needs nothing on the server.
//!
//! Two ways, chosen by what the server allows:
//! - **Commands**: `/setblock` for each block, when the player may run it (an op, or single player
//!   with cheats). Any reach, the exact block states, replacing, and exact undo.
//! - **Clicks**: the vanilla `use_item_on` / `player_action` packets a mouse click sends, on any
//!   server. The server applies vanilla's rules: only blocks within reach, the held item, the
//!   server's own placement states, and breaking only what breaks at once (creative).
//!
//! The first build of a world probes with one command; the server's answer (success, or "unknown
//! command" for a player without permission) picks the way for the rest of the session.
use crate::modes::Pos;
use mod_api::{BlockState, ClientHost, HostBlockStates, Identifier, ServerboundPlayPacket, Text};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

/// `ServerConfig.memory.undoStackSize`.
const UNDO_STACK_SIZE: usize = 50;
/// Most blocks sent in one tick.
const PER_TICK: usize = 256;
/// Ticks to wait for the probe command's answer before assuming clicks.
const PROBE_TIMEOUT: u64 = 60;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Unknown,
    Probing,
    Commands,
    Clicks,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change {
    pub pos: Pos,
    pub old: BlockState,
    pub new: BlockState,
}

/// One block to set: its target and what stood there when it was sent.
#[derive(Clone, Debug)]
pub struct Op {
    pub pos: Pos,
    pub old: BlockState,
    pub target: BlockState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    /// A new build: push on the undo stack and clear redo.
    Build,
    Undo,
    Redo,
}

struct Job {
    due: u64,
    ops: VecDeque<Op>,
    done: Vec<Change>,
    record: Record,
}

struct Applier {
    method: Method,
    probe_sent: u64,
    /// Command answers still to hide from chat.
    pending_answers: u32,
    jobs: VecDeque<Job>,
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
}

static APPLIER: LazyLock<Mutex<Applier>> = LazyLock::new(|| {
    Mutex::new(Applier {
        method: Method::Unknown,
        probe_sent: 0,
        pending_answers: 0,
        jobs: VecDeque::new(),
        undo: Vec::new(),
        redo: Vec::new(),
    })
});

pub fn method() -> Method {
    APPLIER.lock().unwrap().method
}

/// A new world or server: probe again, forget the history (it belongs to another world).
pub fn reset() {
    let mut a = APPLIER.lock().unwrap();
    a.method = Method::Unknown;
    a.pending_answers = 0;
    a.jobs.clear();
    a.undo.clear();
    a.redo.clear();
}

/// Queue a block set; `delay` ticks of appear animation first.
pub fn submit(now: u64, delay: u64, ops: Vec<Op>) {
    if ops.is_empty() {
        return;
    }
    APPLIER.lock().unwrap().jobs.push_back(Job { due: now + delay, ops: ops.into(), done: Vec::new(), record: Record::Build });
}

/// `UndoRedo.undo` / `redo`: the last set back (or again), as a new job.
pub fn history(now: u64, redo: bool, current: impl Fn(Pos) -> Option<BlockState>) {
    let mut a = APPLIER.lock().unwrap();
    let set = if redo { a.redo.pop() } else { a.undo.pop() };
    let Some(set) = set else { return };
    let mut ops: Vec<Op> = set
        .iter()
        .filter_map(|c| {
            let target = if redo { c.new } else { c.old };
            let old = current(c.pos)?;
            Some(Op { pos: c.pos, old, target })
        })
        .collect();
    if !redo {
        ops.reverse();
    }
    a.jobs.push_back(Job {
        due: now,
        ops: ops.into(),
        done: Vec::new(),
        record: if redo { Record::Redo } else { Record::Undo },
    });
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

// ---------------------------------------------------------------------------------------------
// Vanilla packet bodies (protocol of the connected game: the packet name picks its id)

fn varint(out: &mut Vec<u8>, mut v: i32) {
    loop {
        let mut b = (v as u32 & 0x7F) as u8;
        v = ((v as u32) >> 7) as i32;
        if v != 0 {
            b |= 0x80;
        }
        out.push(b);
        if v == 0 {
            break;
        }
    }
}

/// `BlockPos.asLong`.
fn block_pos(out: &mut Vec<u8>, p: Pos) {
    let v = ((p[0] as i64 & 0x3FF_FFFF) << 38) | ((p[2] as i64 & 0x3FF_FFFF) << 12) | (p[1] as i64 & 0xFFF);
    out.extend_from_slice(&v.to_be_bytes());
}

fn packet(name: &str, body: Vec<u8>) -> Option<ServerboundPlayPacket> {
    Some(ServerboundPlayPacket::new(Identifier::parse(name).ok()?, body))
}

/// `ServerboundUseItemOnPacket`: the main hand clicking the top of `pos` at its centre. Clicking a
/// replaceable block (air, grass) places into it, as vanilla's `BlockPlaceContext` does.
fn use_item_on(p: Pos) -> Option<ServerboundPlayPacket> {
    let mut b = Vec::new();
    varint(&mut b, 0); // main hand
    block_pos(&mut b, p);
    varint(&mut b, 1); // up
    for f in [0.5f32, 0.5, 0.5] {
        b.extend_from_slice(&f.to_be_bytes());
    }
    b.push(0); // inside
    b.push(0); // world border
    varint(&mut b, 0); // sequence
    packet("minecraft:use_item_on", b)
}

/// `ServerboundPlayerActionPacket` START_DESTROY_BLOCK: breaks at once in creative.
fn start_destroy(p: Pos) -> Option<ServerboundPlayPacket> {
    let mut b = Vec::new();
    varint(&mut b, 0);
    block_pos(&mut b, p);
    b.push(1); // up
    varint(&mut b, 0);
    packet("minecraft:player_action", b)
}

fn swing() -> Option<ServerboundPlayPacket> {
    let mut b = Vec::new();
    varint(&mut b, 0);
    packet("minecraft:swing", b)
}

// ---------------------------------------------------------------------------------------------

/// What the player needs for the clicks way: where their eyes are, their reach, what they hold.
pub struct Clicker {
    pub eye: [f64; 3],
    pub reach: f64,
    pub held_block: Option<mod_api::BlockKey>,
}

impl Clicker {
    /// `Player.isWithinBlockInteractionRange(pos, 1.0)`, with a little margin.
    pub fn in_reach(&self, p: Pos) -> bool {
        let mut d2 = 0.0;
        for i in 0..3 {
            let lo = p[i] as f64;
            let hi = lo + 1.0;
            let e = self.eye[i];
            let d = if e < lo { lo - e } else if e > hi { e - hi } else { 0.0 };
            d2 += d * d;
        }
        let r = self.reach + 1.0 - 0.05;
        d2 < r * r
    }
}

fn is_air(host: &dyn ClientHost, s: BlockState) -> bool {
    matches!(
        host.block_state_string(s).as_deref(),
        Some("minecraft:air") | Some("minecraft:cave_air") | Some("minecraft:void_air")
    )
}

/// Sends what is due this tick. Returns a note for the action bar, if any.
pub fn tick(host: &mut dyn ClientHost, now: u64, clicker: &Clicker) -> Option<String> {
    let mut a = APPLIER.lock().unwrap();
    if a.method == Method::Probing && now > a.probe_sent + PROBE_TIMEOUT {
        a.method = Method::Clicks;
    }
    let Some(job) = a.jobs.front_mut() else { return None };
    if job.due > now {
        return None;
    }
    let mut note = None;
    match a.method {
        Method::Unknown => {
            // Probe with the first block of the set; its answer decides.
            let op = a.jobs.front().and_then(|j| j.ops.front().cloned());
            if let Some(op) = op {
                if let Some(state) = host.block_state_string(op.target) {
                    if host.send_chat(&format!("/setblock {} {} {} {state} replace", op.pos[0], op.pos[1], op.pos[2])).is_ok() {
                        a.method = Method::Probing;
                        a.probe_sent = now;
                        a.pending_answers += 1;
                    } else {
                        a.method = Method::Clicks;
                    }
                }
            }
        }
        Method::Probing => {}
        Method::Commands => {
            let mut sent = 0;
            while sent < PER_TICK {
                let Some(op) = a.jobs.front_mut().and_then(|j| j.ops.pop_front()) else { break };
                let Some(state) = host.block_state_string(op.target) else { continue };
                let line = format!("/setblock {} {} {} {state} replace", op.pos[0], op.pos[1], op.pos[2]);
                match host.send_chat(&line) {
                    Ok(()) => {
                        a.pending_answers += 1;
                        sent += 1;
                        if let Some(j) = a.jobs.front_mut() {
                            j.done.push(Change { pos: op.pos, old: op.old, new: op.target });
                        }
                    }
                    Err(_) => {
                        // Try again next tick.
                        if let Some(j) = a.jobs.front_mut() {
                            j.ops.push_front(op);
                        }
                        break;
                    }
                }
            }
        }
        Method::Clicks => {
            let mut sent = 0;
            let mut far = 0;
            let mut other = 0;
            while sent < PER_TICK {
                let Some(op) = a.jobs.front_mut().and_then(|j| j.ops.pop_front()) else { break };
                if !clicker.in_reach(op.pos) {
                    far += 1;
                    continue;
                }
                let breaking = is_air(&*host, op.target);
                let placing = !breaking;
                if placing && clicker.held_block != Some(op.target.block()) {
                    // A click places only the held block.
                    other += 1;
                    continue;
                }
                let mut packets = Vec::new();
                if !is_air(&*host, op.old) {
                    // Break first: what stands there is a block to replace, or the one to break.
                    let replaceable = host.block_states().replaceable(op.old).unwrap_or(false);
                    if breaking || !replaceable {
                        packets.extend(start_destroy(op.pos));
                    }
                }
                if placing {
                    packets.extend(use_item_on(op.pos));
                }
                let mut ok = true;
                for p in packets {
                    if host.send_packet(p).is_err() {
                        ok = false;
                        break;
                    }
                }
                if !ok {
                    if let Some(j) = a.jobs.front_mut() {
                        j.ops.push_front(op);
                    }
                    break;
                }
                sent += 1;
                if let Some(j) = a.jobs.front_mut() {
                    j.done.push(Change { pos: op.pos, old: op.old, new: op.target });
                }
            }
            if sent > 0 {
                if let Some(p) = swing() {
                    let _ = host.send_packet(p);
                }
            }
            if far > 0 {
                note = Some(format!("{far} blocks out of reach (no command permission here)"));
            } else if other > 0 {
                note = Some(format!("{other} blocks skipped: only the held block can be placed here"));
            }
        }
    }
    // A finished job goes on the history.
    if a.jobs.front().is_some_and(|j| j.ops.is_empty()) {
        let job = a.jobs.pop_front().unwrap();
        match job.record {
            Record::Build => {
                push_fixed(&mut a.undo, job.done);
                a.redo.clear();
            }
            Record::Undo => {
                let mut done = job.done;
                done.reverse();
                // Redo puts back what undo removed: flip each change.
                let flipped = done.into_iter().map(|c| Change { pos: c.pos, old: c.new, new: c.old }).collect();
                push_fixed(&mut a.redo, flipped);
            }
            Record::Redo => {
                push_fixed(&mut a.undo, job.done);
            }
        }
    }
    note
}

/// A chat line from the server: the answers to our commands decide the way and stay hidden.
/// True to hide the line.
pub fn on_chat(message: &Text) -> bool {
    let mut a = APPLIER.lock().unwrap();
    if a.pending_answers == 0 {
        return false;
    }
    // The translation keys of the line and its parts (the API keeps them, not the shown words).
    let debug = format!("{message:?}");
    let keys = ["command.unknown.", "command.context.here", "commands.setblock."];
    let Some(key) = keys.into_iter().find(|k| debug.contains(&format!("key: \"{k}"))) else { return false };
    if key.starts_with("command.unknown.") {
        // No permission: every command we sent fails the same way.
        // Its second line (the command and "<--[HERE]") follows: that one ends the answer.
        a.method = Method::Clicks;
        return true;
    }
    if key == "command.context.here" {
        a.pending_answers = a.pending_answers.saturating_sub(1);
        return true;
    }
    if a.method == Method::Probing {
        a.method = Method::Commands;
        // The probe set its block: count it done.
        if let Some(j) = a.jobs.front_mut() {
            if let Some(op) = j.ops.pop_front() {
                j.done.push(Change { pos: op.pos, old: op.old, new: op.target });
            }
        }
    }
    a.pending_answers = a.pending_answers.saturating_sub(1);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_pos_packs_like_vanilla() {
        let mut b = Vec::new();
        block_pos(&mut b, [1, 2, 3]);
        assert_eq!(i64::from_be_bytes(b.try_into().unwrap()), (1 << 38) | (3 << 12) | 2);
        let mut b = Vec::new();
        block_pos(&mut b, [-1, -1, -1]);
        assert_eq!(i64::from_be_bytes(b.try_into().unwrap()), -1);
    }

    #[test]
    fn varints() {
        let mut b = Vec::new();
        varint(&mut b, 300);
        assert_eq!(b, vec![0xAC, 0x02]);
        let mut b = Vec::new();
        varint(&mut b, -1);
        assert_eq!(b, vec![0xFF, 0xFF, 0xFF, 0xFF, 0x0F]);
    }

    #[test]
    fn reach_is_the_box_distance() {
        let c = Clicker { eye: [0.5, 1.62, 0.5], reach: 5.0, held_block: None };
        assert!(c.in_reach([5, 1, 0]));
        assert!(!c.in_reach([7, 1, 0]));
    }
}
