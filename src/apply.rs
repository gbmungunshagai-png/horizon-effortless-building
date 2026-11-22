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
//!
//! The history (undo and redo) holds only changes the world has shown: a sent block counts once
//! the client sees it at its position, and is dropped if it never shows up. Undo and redo then
//! touch only positions that still hold what the history left there.
use crate::modes::Pos;
use mod_api::{BlockState, ClientHost, HostBlockStates, Identifier, ServerboundPlayPacket, Text};
use std::collections::VecDeque;
use std::sync::{LazyLock, Mutex};

/// `ServerConfig.memory.undoStackSize`.
const UNDO_STACK_SIZE: usize = 50;
/// Most `/setblock` commands sent in one tick.
const PER_TICK: usize = 256;
/// Most blocks clicked in one tick: a steady pace like a player holding the mouse, not a burst
/// that anticheats on other servers flag.
const CLICKS_PER_TICK: usize = 2;
/// Ticks to wait for the probe command's answer before assuming clicks.
const PROBE_TIMEOUT: u64 = 60;
/// Ticks after the last command during which its answers are hidden from chat. Past it, answers
/// still owed are forgotten, so later answers to the player's own commands show.
const ANSWER_WINDOW: u64 = 100;
/// Ticks to wait for a sent block to show in the world before dropping it from the history.
const CONFIRM_TIMEOUT: u64 = 100;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Unknown,
    Probing,
    Commands,
    Clicks,
}

/// A block state, or (in tests) a stand-in for one: what the history compares.
pub trait State: Copy + PartialEq {
    type Block: PartialEq;
    fn block_of(self) -> Self::Block;
}

impl State for BlockState {
    type Block = mod_api::BlockKey;
    fn block_of(self) -> Self::Block {
        self.block()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Change<S = BlockState> {
    pub pos: Pos,
    pub old: S,
    pub new: S,
}

/// One block to set: its target and what stood there when it was sent.
#[derive(Clone, Debug)]
pub struct Op<S = BlockState> {
    pub pos: Pos,
    pub old: S,
    pub target: S,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Record {
    /// A new build: push on the undo stack and clear redo.
    Build,
    Undo,
    Redo,
}

struct Job<S = BlockState> {
    due: u64,
    ops: VecDeque<Op<S>>,
    /// Sent, not yet seen in the world: the change and the tick it was sent.
    sent: Vec<(Op<S>, u64)>,
    /// Seen in the world: what goes on the history.
    done: Vec<Change<S>>,
    record: Record,
}

impl<S> Job<S> {
    fn new(due: u64, ops: Vec<Op<S>>, record: Record) -> Job<S> {
        Job { due, ops: ops.into(), sent: Vec::new(), done: Vec::new(), record }
    }
}

struct Applier {
    method: Method,
    probe_sent: u64,
    /// The probe's block, held until the answer says whether it was set.
    probe_op: Option<Op>,
    /// Command answers still to hide from chat.
    pending_answers: u32,
    /// The tick the last command was sent.
    last_command: u64,
    jobs: VecDeque<Job>,
    undo: Vec<Vec<Change>>,
    redo: Vec<Vec<Change>>,
}

static APPLIER: LazyLock<Mutex<Applier>> = LazyLock::new(|| {
    Mutex::new(Applier {
        method: Method::Unknown,
        probe_sent: 0,
        probe_op: None,
        pending_answers: 0,
        last_command: 0,
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
    a.probe_op = None;
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
    APPLIER.lock().unwrap().jobs.push_back(Job::new(now + delay, ops, Record::Build));
}

/// What `history` did: how many blocks it queued, and how many it left alone because they had
/// changed since (another player, or the world).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HistoryResult {
    pub queued: usize,
    pub skipped: usize,
}

/// The ops that take `set` back (`redo` false) or again: only positions that still hold the
/// state the history left there.
fn history_ops<S: State>(set: &[Change<S>], redo: bool, current: impl Fn(Pos) -> Option<S>) -> (Vec<Op<S>>, usize) {
    let mut skipped = 0;
    let mut ops: Vec<Op<S>> = set
        .iter()
        .filter_map(|c| {
            let (expected, target) = if redo { (c.old, c.new) } else { (c.new, c.old) };
            let now = current(c.pos);
            if now != Some(expected) {
                skipped += 1;
                return None;
            }
            Some(Op { pos: c.pos, old: expected, target })
        })
        .collect();
    if !redo {
        ops.reverse();
    }
    (ops, skipped)
}

/// `UndoRedo.undo` / `redo`: the last set back (or again), as a new job. None when there is
/// nothing to undo (or redo).
pub fn history(now: u64, redo: bool, current: impl Fn(Pos) -> Option<BlockState>) -> Option<HistoryResult> {
    let mut a = APPLIER.lock().unwrap();
    let set = if redo { a.redo.pop() } else { a.undo.pop() }?;
    let (ops, skipped) = history_ops(&set, redo, current);
    let queued = ops.len();
    a.jobs.push_back(Job::new(now, ops, if redo { Record::Redo } else { Record::Undo }));
    Some(HistoryResult { queued, skipped })
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

/// Moves sent blocks the world now shows to `done`, and drops those that never showed.
/// A click may set another state of the block than asked (the server picks facing and the
/// like), so a block counts when the block is right, and its history keeps the state it got.
fn confirm<S: State>(job: &mut Job<S>, now: u64, current: &dyn Fn(Pos) -> Option<S>) {
    let sent = std::mem::take(&mut job.sent);
    for (op, at) in sent {
        match current(op.pos) {
            Some(s) if s == op.target || s.block_of() == op.target.block_of() && s != op.old => {
                job.done.push(Change { pos: op.pos, old: op.old, new: s });
            }
            _ if now > at + CONFIRM_TIMEOUT => {}
            _ => job.sent.push((op, at)),
        }
    }
}

/// Sends what is due this tick. Returns a note for the action bar, if any.
pub fn tick(host: &mut dyn ClientHost, now: u64, clicker: &Clicker) -> Option<String> {
    let mut a = APPLIER.lock().unwrap();
    if a.method == Method::Probing && now > a.probe_sent + PROBE_TIMEOUT {
        // No answer: assume no permission, and click the probe's block too.
        a.method = Method::Clicks;
        a.pending_answers = 0;
        if let Some(op) = a.probe_op.take() {
            if let Some(j) = a.jobs.front_mut() {
                j.ops.push_front(op);
            }
        }
    }
    if a.pending_answers > 0 && now > a.last_command + ANSWER_WINDOW {
        a.pending_answers = 0;
    }
    let Some(job) = a.jobs.front_mut() else { return None };
    if job.due > now {
        return None;
    }
    let level = host.local_player().and_then(|p| p.level(&*host));
    let current = |pos: Pos| level.as_ref().and_then(|l| l.block_state(&*host, pos));
    confirm(job, now, &current);
    let mut note = None;
    match a.method {
        Method::Unknown => {
            // Probe with the first block of the set; its answer decides.
            let op = a.jobs.front_mut().and_then(|j| j.ops.pop_front());
            if let Some(op) = op {
                let line = host
                    .block_state_string(op.target)
                    .map(|state| format!("/setblock {} {} {} {state} replace", op.pos[0], op.pos[1], op.pos[2]));
                if line.is_some_and(|l| host.send_chat(&l).is_ok()) {
                    a.method = Method::Probing;
                    a.probe_sent = now;
                    a.last_command = now;
                    a.pending_answers += 1;
                    a.probe_op = Some(op);
                } else {
                    a.method = Method::Clicks;
                    if let Some(j) = a.jobs.front_mut() {
                        j.ops.push_front(op);
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
                        a.last_command = now;
                        sent += 1;
                        if let Some(j) = a.jobs.front_mut() {
                            j.sent.push((op, now));
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
            while sent < CLICKS_PER_TICK {
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
                    j.sent.push((op, now));
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
    // A finished job (all sent, and each seen or given up on) goes on the history.
    if a.method != Method::Probing && a.jobs.front().is_some_and(|j| j.ops.is_empty() && j.sent.is_empty()) {
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
        if a.method == Method::Probing {
            // The probe's block was not set: click it with the rest.
            if let Some(op) = a.probe_op.take() {
                if let Some(j) = a.jobs.front_mut() {
                    j.ops.push_front(op);
                }
            }
        }
        a.method = Method::Clicks;
        return true;
    }
    if key == "command.context.here" {
        a.pending_answers = a.pending_answers.saturating_sub(1);
        return true;
    }
    if a.method == Method::Probing {
        a.method = Method::Commands;
        // The probe was answered: wait for its block like any other sent one. `tick` holds no
        // clock here; the probe's send time is close enough.
        let at = a.probe_sent;
        if let Some(op) = a.probe_op.take() {
            if let Some(j) = a.jobs.front_mut() {
                j.sent.push((op, at));
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

    /// A stand-in state: the block is the tens, the state the units.
    #[derive(Clone, Copy, Debug, PartialEq)]
    struct T(u32);
    impl State for T {
        type Block = u32;
        fn block_of(self) -> u32 {
            self.0 / 10
        }
    }
    fn st(id: u32) -> T {
        T(id * 10)
    }

    #[test]
    fn undo_leaves_changed_blocks_alone() {
        let set = vec![
            Change { pos: [0, 0, 0], old: st(0), new: st(1) },
            Change { pos: [1, 0, 0], old: st(0), new: st(1) },
        ];
        // Someone replaced the second block since.
        let world = |p: Pos| Some(if p == [1, 0, 0] { st(7) } else { st(1) });
        let (ops, skipped) = history_ops(&set, false, world);
        assert_eq!(skipped, 1);
        assert_eq!(ops.len(), 1);
        assert_eq!((ops[0].pos, ops[0].old, ops[0].target), ([0, 0, 0], st(1), st(0)));
        // Redo wants the old state back in place.
        let (ops, skipped) = history_ops(&set, true, |_| Some(st(0)));
        assert_eq!((ops.len(), skipped), (2, 0));
    }

    #[test]
    fn only_seen_blocks_reach_the_history() {
        let mut job = Job::new(0, Vec::new(), Record::Build);
        job.sent.push((Op { pos: [0, 0, 0], old: st(0), target: st(1) }, 0));
        job.sent.push((Op { pos: [1, 0, 0], old: st(0), target: st(1) }, 0));
        // The server set the first block only.
        let world = |p: Pos| Some(if p == [0, 0, 0] { st(1) } else { st(0) });
        confirm(&mut job, 1, &world);
        assert_eq!(job.done.len(), 1);
        assert_eq!(job.sent.len(), 1);
        confirm(&mut job, CONFIRM_TIMEOUT + 1, &world);
        assert_eq!(job.done.len(), 1);
        assert!(job.sent.is_empty());
    }

    #[test]
    fn a_click_may_get_another_state_of_the_block() {
        let mut job = Job::new(0, Vec::new(), Record::Build);
        // Asked for stairs facing north (10), the server made them face east (13).
        job.sent.push((Op { pos: [0, 0, 0], old: T(0), target: T(10) }, 0));
        confirm(&mut job, 1, &|_| Some(T(13)));
        assert_eq!(job.done, vec![Change { pos: [0, 0, 0], old: T(0), new: T(13) }]);
    }

    #[test]
    fn reach_is_the_box_distance() {
        let c = Clicker { eye: [0.5, 1.62, 0.5], reach: 5.0, held_block: None };
        assert!(c.in_reach([5, 1, 0]));
        assert!(!c.in_reach([7, 1, 0]));
    }
}
