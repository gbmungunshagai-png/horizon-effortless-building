//! Build modifiers: Mirror, Array and Radial Mirror, ported from Effortless Building's
//! `buildmodifier` package (Requios, LGPL-2.1), and the ordered block set they work on.
use crate::modes::{Pos, Vec3};
use std::collections::HashSet;

/// One position of a build and how its block is turned (`BlockEntry`).
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub pos: Pos,
    pub mirror: [bool; 3],
    /// Quarter turns clockwise seen from above (`Rotation`).
    pub rotation: u8,
}

impl Entry {
    pub fn new(pos: Pos) -> Self {
        Self { pos, mirror: [false; 3], rotation: 0 }
    }
    fn copy_turn(&self, pos: Pos) -> Self {
        Self { pos, mirror: self.mirror, rotation: self.rotation }
    }
}

/// `BlockSet`: positions in the order they were added, each once.
#[derive(Clone, Debug, Default)]
pub struct BlockSet {
    pub entries: Vec<Entry>,
    seen: HashSet<Pos>,
    pub first: Option<Pos>,
    pub last: Option<Pos>,
    /// Most entries it takes; 0 for no limit.
    pub limit: usize,
}

impl BlockSet {
    pub fn clear(&mut self) {
        self.entries.clear();
        self.seen.clear();
        self.first = None;
        self.last = None;
    }
    pub fn add(&mut self, e: Entry) -> bool {
        // `BlockSet.ClientSide.isFull`: no more than the player may place at once.
        if self.limit != 0 && self.entries.len() >= self.limit {
            return false;
        }
        if !self.seen.insert(e.pos) {
            return false;
        }
        self.entries.push(e);
        true
    }
    pub fn contains(&self, p: &Pos) -> bool {
        self.seen.contains(p)
    }
    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
    pub fn set_start(&mut self, p: Pos) {
        self.clear();
        self.add(Entry::new(p));
        self.first = Some(p);
        self.last = Some(p);
    }
    pub fn retain(&mut self, mut keep: impl FnMut(&Entry) -> bool) {
        let seen = &mut self.seen;
        self.entries.retain(|e| {
            let k = keep(e);
            if !k {
                seen.remove(&e.pos);
            }
            k
        });
    }
    pub fn coordinates(&self) -> HashSet<Pos> {
        self.seen.clone()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mirror {
    pub enabled: bool,
    pub position: Vec3,
    pub axes: [bool; 3],
    pub radius: i32,
    pub draw_lines: bool,
    pub draw_planes: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Array {
    pub enabled: bool,
    pub offset: Pos,
    pub count: i32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RadialMirror {
    pub enabled: bool,
    pub position: Vec3,
    pub slices: i32,
    pub alternate: bool,
    pub radius: i32,
    pub draw_lines: bool,
    pub draw_planes: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Modifier {
    Mirror(Mirror),
    Array(Array),
    RadialMirror(RadialMirror),
}

impl Mirror {
    /// A new mirror at the player's block (its lower corner), mirroring X.
    pub fn new(player_block: Pos) -> Self {
        Self {
            enabled: true,
            position: [player_block[0] as f64, player_block[1] as f64, player_block[2] as f64],
            axes: [true, false, false],
            radius: 10,
            draw_lines: true,
            draw_planes: true,
        }
    }

    fn within_range(&self, p: Pos) -> bool {
        (0..3).all(|i| {
            let c = p[i] as f64 + 0.5;
            !(c < self.position[i] - self.radius as f64) && !(c > self.position[i] + self.radius as f64)
        })
    }

    fn mirror_axis(&self, blocks: &mut BlockSet, e: &Entry, axis: usize) {
        let v = self.position[axis] + (self.position[axis] - e.pos[axis] as f64 - 0.5);
        let mut pos = e.pos;
        pos[axis] = v.floor() as i32;
        if blocks.contains(&pos) {
            return;
        }
        let mut n = e.copy_turn(pos);
        n.mirror[axis] = !n.mirror[axis];
        blocks.add(n.clone());
        for next in axis + 1..3 {
            if self.axes[next] {
                self.mirror_axis(blocks, &n, next);
            }
        }
    }

    pub fn apply(&self, blocks: &mut BlockSet) {
        if !(self.enabled && self.axes.iter().any(|a| *a)) {
            return;
        }
        let originals = blocks.entries.clone();
        for e in &originals {
            if !self.within_range(e.pos) {
                continue;
            }
            for axis in 0..3 {
                if self.axes[axis] {
                    self.mirror_axis(blocks, e, axis);
                }
            }
        }
    }
}

impl Array {
    pub fn new() -> Self {
        Self { enabled: true, offset: [0, 0, 0], count: 5 }
    }

    pub fn reach(&self) -> i32 {
        let largest = self.offset.iter().map(|v| v.abs()).max().unwrap_or(0);
        largest * self.count
    }

    pub fn apply(&self, blocks: &mut BlockSet) {
        if !self.enabled || self.offset == [0, 0, 0] {
            return;
        }
        let originals = blocks.entries.clone();
        for e in &originals {
            let mut pos = e.pos;
            for _ in 0..self.count {
                pos = [pos[0] + self.offset[0], pos[1] + self.offset[1], pos[2] + self.offset[2]];
                if blocks.contains(&pos) {
                    continue;
                }
                blocks.add(e.copy_turn(pos));
            }
        }
    }
}

/// `Vec3.yRot`, with the float trigonometry `Mth` uses.
fn y_rot(v: Vec3, angle: f32) -> Vec3 {
    let (c, s) = (angle.cos() as f64, angle.sin() as f64);
    [v[0] * c + v[2] * s, v[1], v[2] * c - v[0] * s]
}

impl RadialMirror {
    pub fn new(player_block: Pos) -> Self {
        Self {
            enabled: true,
            position: [player_block[0] as f64, player_block[1] as f64, player_block[2] as f64],
            slices: 4,
            alternate: false,
            radius: 20,
            draw_lines: true,
            draw_planes: false,
        }
    }

    fn within_range(&self, p: Pos) -> bool {
        let d = [
            p[0] as f64 + 0.5 - self.position[0],
            p[1] as f64 + 0.5 - self.position[1],
            p[2] as f64 + 0.5 - self.position[2],
        ];
        d[0] * d[0] + d[1] * d[1] + d[2] * d[2] < (self.radius * self.radius) as f64
    }

    pub fn apply(&self, blocks: &mut BlockSet) {
        if !self.enabled || self.slices < 1 {
            return;
        }
        let originals = blocks.entries.clone();
        for e in &originals {
            if !self.within_range(e.pos) {
                continue;
            }
            let slice_angle = 2.0 * std::f64::consts::PI / self.slices as f64;
            let rel = [
                e.pos[0] as f64 + 0.5 - self.position[0],
                e.pos[1] as f64 + 0.5 - self.position[1],
                e.pos[2] as f64 + 0.5 - self.position[2],
            ];
            let mut start_angle = rel[0].atan2(rel[2]);
            if start_angle < 0.0 {
                start_angle += std::f64::consts::PI;
            }
            let start_in_slice = start_angle % slice_angle;
            for i in 1..self.slices {
                let mut cur = slice_angle * i as f64;
                let alternate = self.alternate && i % 2 == 1;
                if alternate {
                    cur = cur - start_in_slice + (slice_angle - start_in_slice);
                }
                let new_rel = y_rot(rel, cur as f32);
                let pos = [
                    (self.position[0] + new_rel[0]).floor() as i32,
                    (self.position[1] + new_rel[1]).floor() as i32,
                    (self.position[2] + new_rel[2]).floor() as i32,
                ];
                if blocks.contains(&pos) {
                    continue;
                }
                let mut n = e.copy_turn(pos);
                let angle = new_rel[0].atan2(new_rel[2]);
                let pi = std::f64::consts::PI;
                let turn = if angle < -0.751 * pi || angle > 0.749 * pi {
                    2
                } else if angle < -0.251 * pi {
                    1
                } else if angle > 0.249 * pi {
                    3
                } else {
                    0
                };
                n.rotation = (n.rotation + turn) % 4;
                blocks.add(n);
            }
        }
    }
}

impl Modifier {
    pub fn apply(&self, blocks: &mut BlockSet) {
        match self {
            Modifier::Mirror(m) => m.apply(blocks),
            Modifier::Array(a) => a.apply(blocks),
            Modifier::RadialMirror(r) => r.apply(blocks),
        }
    }
    pub fn enabled(&self) -> bool {
        match self {
            Modifier::Mirror(m) => m.enabled,
            Modifier::Array(a) => a.enabled,
            Modifier::RadialMirror(r) => r.enabled,
        }
    }
    pub fn set_enabled(&mut self, on: bool) {
        match self {
            Modifier::Mirror(m) => m.enabled = on,
            Modifier::Array(a) => a.enabled = on,
            Modifier::RadialMirror(r) => r.enabled = on,
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Modifier::Mirror(_) => "Mirror",
            Modifier::Array(_) => "Array",
            Modifier::RadialMirror(_) => "Radial Mirror",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_of(p: &[Pos]) -> BlockSet {
        let mut b = BlockSet::default();
        for p in p {
            b.add(Entry::new(*p));
        }
        b
    }

    #[test]
    fn mirror_x_about_block_corner() {
        let m = Mirror::new([0, 0, 0]);
        let mut b = set_of(&[[2, 0, 0]]);
        m.apply(&mut b);
        assert!(b.contains(&[-3, 0, 0]));
        assert!(b.entries[1].mirror[0]);
    }

    #[test]
    fn mirror_xz_makes_four() {
        let mut m = Mirror::new([0, 0, 0]);
        m.axes = [true, false, true];
        let mut b = set_of(&[[2, 0, 3]]);
        m.apply(&mut b);
        assert_eq!(b.len(), 4);
    }

    #[test]
    fn array_repeats() {
        let mut a = Array::new();
        a.offset = [0, 2, 0];
        a.count = 3;
        let mut b = set_of(&[[0, 0, 0]]);
        a.apply(&mut b);
        assert_eq!(b.entries.iter().map(|e| e.pos).collect::<Vec<_>>(), vec![[0, 0, 0], [0, 2, 0], [0, 4, 0], [0, 6, 0]]);
    }

    #[test]
    fn radial_four_slices() {
        let r = RadialMirror::new([0, 0, 0]);
        let mut b = set_of(&[[3, 0, 0]]);
        r.apply(&mut b);
        assert_eq!(b.len(), 4);
    }
}
