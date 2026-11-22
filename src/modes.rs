//! Build modes, their options and their geometry: a port of Effortless Building's
//! `buildmode` package (Requios, LGPL-2.1). Pure maths, no game calls.
//!
//! The second and third points of a shape are not the block under the crosshair: like the
//! original, they come from where the look ray crosses a plane (or a line) through the first
//! point, so a wall stays on the first block's plane however you look.

pub type Pos = [i32; 3];
pub type Vec3 = [f64; 3];

/// The original's build modes, in its radial menu order (1.21 build: no roof modes).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BuildMode {
    Disabled,
    Single,
    Line,
    Wall,
    Floor,
    Cube,
    DiagonalLine,
    DiagonalWall,
    SlopeFloor,
    Circle,
    Cylinder,
    Sphere,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    Basic,
    Diagonal,
    Circular,
}

impl Category {
    /// The coloured line at the inner edge of a mode's segment (ARGB).
    pub fn color(self) -> u32 {
        match self {
            Category::Basic => argbf(0.0, 0.5, 1.0, 0.8),
            Category::Diagonal => argbf(0.56, 0.28, 0.87, 0.8),
            Category::Circular => argbf(0.29, 0.76, 0.3, 1.0),
        }
    }
}

/// `new Color(r, g, b, a)` with floats, as the original's catnip `Color` rounds them.
pub fn argbf(r: f32, g: f32, b: f32, a: f32) -> u32 {
    let c = |v: f32| ((v * 255.0 + 0.5) as u32).min(255);
    (c(a) << 24) | (c(r) << 16) | (c(g) << 8) | c(b)
}

impl BuildMode {
    pub const ALL: [BuildMode; 12] = [
        BuildMode::Disabled,
        BuildMode::Single,
        BuildMode::Line,
        BuildMode::Wall,
        BuildMode::Floor,
        BuildMode::Cube,
        BuildMode::DiagonalLine,
        BuildMode::DiagonalWall,
        BuildMode::SlopeFloor,
        BuildMode::Circle,
        BuildMode::Cylinder,
        BuildMode::Sphere,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildMode::Disabled => "Disable",
            BuildMode::Single => "Single",
            BuildMode::Line => "Line",
            BuildMode::Wall => "Wall",
            BuildMode::Floor => "Floor",
            BuildMode::Cube => "Cube",
            BuildMode::DiagonalLine => "Diagonal Line",
            BuildMode::DiagonalWall => "Diagonal Wall",
            BuildMode::SlopeFloor => "Slope Floor",
            BuildMode::Circle => "Circle",
            BuildMode::Cylinder => "Cylinder",
            BuildMode::Sphere => "Sphere",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            BuildMode::Disabled => "Disable mod and use vanilla placement rules",
            BuildMode::Single => "Like vanilla, but with increased reach and placement preview",
            _ => "",
        }
    }

    pub fn category(self) -> Category {
        match self {
            BuildMode::DiagonalLine | BuildMode::DiagonalWall | BuildMode::SlopeFloor => Category::Diagonal,
            BuildMode::Circle | BuildMode::Cylinder | BuildMode::Sphere => Category::Circular,
            _ => Category::Basic,
        }
    }

    /// Column of its icon in row 1 of `icons.png`.
    pub fn icon(self) -> (u32, u32) {
        (BuildMode::ALL.iter().position(|m| *m == self).unwrap_or(0) as u32, 1)
    }

    pub fn options(self) -> &'static [OptionKind] {
        match self {
            BuildMode::Single => &[OptionKind::BuildSpeed],
            BuildMode::Wall | BuildMode::Floor => &[OptionKind::Fill],
            BuildMode::Cube => &[OptionKind::CubeFill],
            BuildMode::SlopeFloor => &[OptionKind::RaisedEdge],
            BuildMode::Circle | BuildMode::Cylinder | BuildMode::Sphere => &[OptionKind::CircleStart, OptionKind::Fill],
            _ => &[],
        }
    }

    /// Clicks a shape takes: 1 places at once, 2 and 3 are the original's two- and three-click modes.
    pub fn clicks(self) -> u8 {
        match self {
            BuildMode::Disabled | BuildMode::Single => 1,
            BuildMode::Line | BuildMode::Wall | BuildMode::Floor | BuildMode::Circle => 2,
            _ => 3,
        }
    }
}

/// Every button the radial menu shows: actions on the left, mode options on the right.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Undo,
    Redo,
    OpenModifierSettings,
    ReplaceOnlyAir,
    ReplaceBlocksAndAir,
    ReplaceOnlyBlocks,
    ReplaceFilteredByOffhand,
    ToggleProtectTileEntities,
    NormalSpeed,
    FastSpeed,
    Full,
    Hollow,
    CubeFull,
    CubeHollow,
    CubeSkeleton,
    ShortEdge,
    LongEdge,
    CircleStartCorner,
    CircleStartCenter,
    PreviousBuildMode,
    DisableBuildModeToggle,
}

impl Action {
    pub fn name(self) -> &'static str {
        match self {
            Action::Undo => "Undo",
            Action::Redo => "Redo",
            Action::OpenModifierSettings => "Open Modifier Settings",
            Action::ReplaceOnlyAir => "Replace Only Air",
            Action::ReplaceBlocksAndAir => "Replace Blocks And Air",
            Action::ReplaceOnlyBlocks => "Replace Only Blocks",
            Action::ReplaceFilteredByOffhand => "Filter By Offhand",
            Action::ToggleProtectTileEntities => "Protect Tile Entities",
            Action::NormalSpeed => "Normal",
            Action::FastSpeed => "Fast",
            Action::Full | Action::CubeFull => "Filled",
            Action::Hollow | Action::CubeHollow => "Hollow",
            Action::CubeSkeleton => "Skeleton",
            Action::ShortEdge => "Short Edge",
            Action::LongEdge => "Long Edge",
            Action::CircleStartCorner => "Corner",
            Action::CircleStartCenter => "Middle",
            Action::PreviousBuildMode => "Activate Previous Build Mode",
            Action::DisableBuildModeToggle => "Toggle Disabled <> Previous Build Mode",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            Action::ReplaceOnlyAir => {
                "You will only place blocks where there was air. Replaceables such as grass will still be replaced."
            }
            Action::ReplaceBlocksAndAir => "You will replace blocks and air.",
            Action::ReplaceOnlyBlocks => "You will only replace blocks, not air.",
            Action::ToggleProtectTileEntities => {
                "If enabled, blocks that hold data such as chests, furnaces, and hoppers will not be replaced."
            }
            _ => "",
        }
    }

    /// Cell of its icon in `icons.png` (column, row).
    pub fn icon(self) -> (u32, u32) {
        match self {
            Action::OpenModifierSettings => (0, 0),
            Action::Undo => (1, 0),
            Action::Redo => (2, 0),
            Action::ReplaceOnlyAir => (4, 0),
            Action::ReplaceBlocksAndAir => (5, 0),
            Action::ReplaceOnlyBlocks => (6, 0),
            Action::ReplaceFilteredByOffhand => (7, 0),
            Action::ToggleProtectTileEntities => (8, 0),
            Action::PreviousBuildMode => (1, 1),
            Action::DisableBuildModeToggle => (0, 1),
            Action::NormalSpeed => (0, 2),
            Action::FastSpeed => (1, 2),
            Action::Full => (2, 2),
            Action::Hollow => (3, 2),
            Action::CubeFull => (4, 2),
            Action::CubeHollow => (5, 2),
            Action::CubeSkeleton => (6, 2),
            Action::ShortEdge => (7, 2),
            Action::LongEdge => (8, 2),
            Action::CircleStartCorner => (9, 2),
            Action::CircleStartCenter => (10, 2),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionKind {
    BuildSpeed,
    Fill,
    CubeFill,
    RaisedEdge,
    CircleStart,
}

impl OptionKind {
    pub fn name(self) -> &'static str {
        match self {
            OptionKind::BuildSpeed => "Build Speed",
            OptionKind::Fill | OptionKind::CubeFill => "Filling",
            OptionKind::RaisedEdge => "Raised Edge",
            OptionKind::CircleStart => "Start Point",
        }
    }

    pub fn actions(self) -> &'static [Action] {
        match self {
            OptionKind::BuildSpeed => &[Action::NormalSpeed, Action::FastSpeed],
            OptionKind::Fill => &[Action::Full, Action::Hollow],
            OptionKind::CubeFill => &[Action::CubeFull, Action::CubeHollow, Action::CubeSkeleton],
            OptionKind::RaisedEdge => &[Action::ShortEdge, Action::LongEdge],
            OptionKind::CircleStart => &[Action::CircleStartCorner, Action::CircleStartCenter],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CubeFill {
    Full,
    Hollow,
    Skeleton,
}

/// `ModeOptions`: the current choice of each option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModeOptions {
    pub fast: bool,
    pub hollow: bool,
    pub cube_fill: CubeFill,
    pub long_edge: bool,
    pub circle_center: bool,
}

impl Default for ModeOptions {
    fn default() -> Self {
        Self { fast: false, hollow: false, cube_fill: CubeFill::Full, long_edge: false, circle_center: false }
    }
}

impl ModeOptions {
    /// The action currently chosen for `option`.
    pub fn selected(&self, option: OptionKind) -> Action {
        match option {
            OptionKind::BuildSpeed => {
                if self.fast {
                    Action::FastSpeed
                } else {
                    Action::NormalSpeed
                }
            }
            OptionKind::Fill => {
                if self.hollow {
                    Action::Hollow
                } else {
                    Action::Full
                }
            }
            OptionKind::CubeFill => match self.cube_fill {
                CubeFill::Full => Action::CubeFull,
                CubeFill::Hollow => Action::CubeHollow,
                CubeFill::Skeleton => Action::CubeSkeleton,
            },
            OptionKind::RaisedEdge => {
                if self.long_edge {
                    Action::LongEdge
                } else {
                    Action::ShortEdge
                }
            }
            OptionKind::CircleStart => {
                if self.circle_center {
                    Action::CircleStartCenter
                } else {
                    Action::CircleStartCorner
                }
            }
        }
    }

    /// Applies an option action; false for one that isn't an option.
    pub fn apply(&mut self, action: Action) -> bool {
        match action {
            Action::NormalSpeed => self.fast = false,
            Action::FastSpeed => self.fast = true,
            Action::Full => self.hollow = false,
            Action::Hollow => self.hollow = true,
            Action::CubeFull => self.cube_fill = CubeFill::Full,
            Action::CubeHollow => self.cube_fill = CubeFill::Hollow,
            Action::CubeSkeleton => self.cube_fill = CubeFill::Skeleton,
            Action::ShortEdge => self.long_edge = false,
            Action::LongEdge => self.long_edge = true,
            Action::CircleStartCorner => self.circle_center = false,
            Action::CircleStartCenter => self.circle_center = true,
            _ => return false,
        }
        true
    }
}

// ---------------------------------------------------------------------------------------------
// Look-ray helpers (BuildModes.java)

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn len_sq(a: Vec3) -> f64 {
    dot(a, a)
}
/// `BlockPos.containing`.
pub fn containing(v: Vec3) -> Pos {
    [v[0].floor() as i32, v[1].floor() as i32, v[2].floor() as i32]
}
fn lower_corner(p: Pos) -> Vec3 {
    [p[0] as f64, p[1] as f64, p[2] as f64]
}

/// `Entity.getLookAngle` from yaw and pitch in degrees, nudged off 0 and ±1 as `getPlayerLookVec` does.
pub fn look_vec(yaw: f32, pitch: f32) -> Vec3 {
    let (yaw, pitch) = ((yaw as f64).to_radians(), (pitch as f64).to_radians());
    let raw = [-yaw.sin() * pitch.cos(), -pitch.sin(), yaw.cos() * pitch.cos()];
    let mut out = [0.0; 3];
    for i in 0..3 {
        let mut c = raw[i];
        if c.abs() < 0.0001 {
            c = 0.0001;
        }
        if (c - 1.0).abs() < 0.0001 {
            c = 0.9999;
        }
        if (c + 1.0).abs() < 0.0001 {
            c = -0.9999;
        }
        out[i] = c;
    }
    out
}

pub fn find_x_bound(x: f64, start: Vec3, look: Vec3) -> Vec3 {
    let y = (x - start[0]) / look[0] * look[1] + start[1];
    let z = (x - start[0]) / look[0] * look[2] + start[2];
    [x, y, z]
}
pub fn find_y_bound(y: f64, start: Vec3, look: Vec3) -> Vec3 {
    let x = (y - start[1]) / look[1] * look[0] + start[0];
    let z = (y - start[1]) / look[1] * look[2] + start[2];
    [x, y, z]
}
pub fn find_z_bound(z: f64, start: Vec3, look: Vec3) -> Vec3 {
    let x = (z - start[2]) / look[2] * look[0] + start[0];
    let y = (z - start[2]) / look[2] * look[1] + start[1];
    [x, y, z]
}

/// `isCriteriaValid` with `skipRaytrace` (every caller in the original passes true).
fn criteria_valid(start: Vec3, look: Vec3, reach: i32, plane_bound: Vec3, dist_to_player_sq: f64) -> bool {
    let reach = reach as f64;
    dot(sub(plane_bound, start), look) > 0.0 && dist_to_player_sq > 2.0 && dist_to_player_sq < reach * reach
}

struct LineCriteria {
    plane_bound: Vec3,
    line_bound: Vec3,
    dist_to_line_sq: f64,
    dist_to_player_sq: f64,
}

/// Picks among line criteria as Line and findHeight do.
fn pick_line(mut list: Vec<LineCriteria>, start: Vec3, look: Vec3, reach: i32) -> Option<Pos> {
    list.retain(|c| criteria_valid(start, look, reach, c.plane_bound, c.dist_to_player_sq));
    let mut selected = 0;
    for i in 1..list.len() {
        let (c, s) = (&list[i], &list[selected]);
        if c.dist_to_line_sq < 2.0 && s.dist_to_line_sq < 2.0 {
            if c.dist_to_player_sq < s.dist_to_player_sq {
                selected = i;
            }
        } else if c.dist_to_line_sq < s.dist_to_line_sq {
            selected = i;
        }
    }
    list.get(selected).map(|c| containing(c.line_bound))
}

/// `Line.findLine`: the second point on the axis line through `first` nearest the look ray.
pub fn find_line(start: Vec3, look: Vec3, first: Pos, reach: i32) -> Option<Pos> {
    let bounds = [
        find_x_bound(first[0] as f64, start, look),
        find_y_bound(first[1] as f64, start, look),
        find_z_bound(first[2] as f64, start, look),
    ];
    let list = bounds
        .into_iter()
        .map(|plane_bound| {
            let bound = containing(plane_bound);
            let d = [(bound[0] - first[0]).abs(), (bound[1] - first[1]).abs(), (bound[2] - first[2]).abs()];
            let longest = d[0].max(d[1]).max(d[2]);
            let line_bound = if longest == d[0] {
                [bound[0] as f64, first[1] as f64, first[2] as f64]
            } else if longest == d[1] {
                [first[0] as f64, bound[1] as f64, first[2] as f64]
            } else {
                [first[0] as f64, first[1] as f64, bound[2] as f64]
            };
            LineCriteria {
                plane_bound,
                line_bound,
                dist_to_line_sq: len_sq(sub(line_bound, plane_bound)),
                dist_to_player_sq: len_sq(sub(plane_bound, start)),
            }
        })
        .collect();
    pick_line(list, start, look, reach)
}

/// `Wall.findWall`: the look ray on the X or Z plane through `first`.
pub fn find_wall(start: Vec3, look: Vec3, first: Pos, reach: i32) -> Option<Pos> {
    struct C {
        plane_bound: Vec3,
        dist_to_player_sq: f64,
        angle: f64,
    }
    let make = |plane_bound: Vec3| {
        let wall = sub(plane_bound, lower_corner(first));
        C {
            plane_bound,
            dist_to_player_sq: len_sq(sub(plane_bound, start)),
            angle: wall[0] * look[0] + wall[2] * look[2],
        }
    };
    let mut list = vec![
        make(find_x_bound(first[0] as f64, start, look)),
        make(find_z_bound(first[2] as f64, start, look)),
    ];
    list.retain(|c| criteria_valid(start, look, reach, c.plane_bound, c.dist_to_player_sq));
    let mut selected = 0;
    for i in 1..list.len() {
        let (c, s) = (&list[i], &list[selected]);
        if c.dist_to_player_sq < s.dist_to_player_sq && c.angle.abs() - s.angle.abs() < 3.0 {
            selected = i;
        }
    }
    list.get(selected).map(|c| containing(c.plane_bound))
}

/// `Floor.findFloor`: the look ray on the horizontal plane through `first`.
pub fn find_floor(start: Vec3, look: Vec3, first: Pos, reach: i32) -> Option<Pos> {
    let plane_bound = find_y_bound(first[1] as f64, start, look);
    let d = len_sq(sub(plane_bound, start));
    criteria_valid(start, look, reach, plane_bound, d).then(|| containing(plane_bound))
}

/// `ThreeClicksBuildMode.findHeight`: the third point straight above or below `second`.
pub fn find_height(start: Vec3, look: Vec3, second: Pos, reach: i32) -> Option<Pos> {
    let list = [
        find_x_bound(second[0] as f64, start, look),
        find_z_bound(second[2] as f64, start, look),
    ]
    .into_iter()
    .map(|plane_bound| {
        let bound = containing(plane_bound);
        let line_bound = [second[0] as f64, bound[1] as f64, second[2] as f64];
        LineCriteria {
            plane_bound,
            line_bound,
            dist_to_line_sq: len_sq(sub(line_bound, plane_bound)),
            dist_to_player_sq: len_sq(sub(plane_bound, start)),
        }
    })
    .collect();
    pick_line(list, start, look, reach)
}

impl BuildMode {
    /// Where the second point is, from the look ray.
    pub fn second_pos(self, start: Vec3, look: Vec3, first: Pos, reach: i32) -> Option<Pos> {
        match self {
            BuildMode::Line => find_line(start, look, first, reach),
            BuildMode::Wall => find_wall(start, look, first, reach),
            _ => find_floor(start, look, first, reach),
        }
    }

    /// Where the third point is (three-click modes).
    pub fn third_pos(self, start: Vec3, look: Vec3, second: Pos, reach: i32) -> Option<Pos> {
        find_height(start, look, second, reach)
    }
}

/// Limits `p` to `limit` blocks from `first` on each axis.
pub fn limit_axis(first: Pos, mut p: Pos, limit: i32) -> Pos {
    for i in 0..3 {
        if p[i] - first[i] >= limit {
            p[i] = first[i] + limit - 1;
        }
        if first[i] - p[i] >= limit {
            p[i] = first[i] - limit + 1;
        }
    }
    p
}

// ---------------------------------------------------------------------------------------------
// Block lists (each mode's getXBlocks), in the original's order.

/// `for (v = a; a < b ? v <= b : v >= b; v += a < b ? 1 : -1)`.
fn span(a: i32, b: i32) -> impl Iterator<Item = i32> {
    let step = if a < b { 1 } else { -1 };
    let n = (b - a).abs() + 1;
    (0..n).map(move |i| a + i * step)
}

pub fn x_line(list: &mut Vec<Pos>, x1: i32, x2: i32, y: i32, z: i32) {
    list.extend(span(x1, x2).map(|x| [x, y, z]));
}
pub fn y_line(list: &mut Vec<Pos>, y1: i32, y2: i32, x: i32, z: i32) {
    list.extend(span(y1, y2).map(|y| [x, y, z]));
}
pub fn z_line(list: &mut Vec<Pos>, z1: i32, z2: i32, x: i32, y: i32) {
    list.extend(span(z1, z2).map(|z| [x, y, z]));
}

pub fn line_blocks(p1: Pos, p2: Pos) -> Vec<Pos> {
    let mut list = Vec::new();
    let ([x1, y1, z1], [x2, y2, z2]) = (p1, p2);
    if x1 != x2 {
        x_line(&mut list, x1, x2, y1, z1);
    } else if y1 != y2 {
        y_line(&mut list, y1, y2, x1, z1);
    } else {
        z_line(&mut list, z1, z2, x1, y1);
    }
    list
}

fn x_wall(list: &mut Vec<Pos>, cap: usize, x: i32, y1: i32, y2: i32, z1: i32, z2: i32) {
    for z in span(z1, z2) {
        if list.len() >= cap {
            return;
        }
        for y in span(y1, y2) {
            list.push([x, y, z]);
        }
    }
}
fn z_wall(list: &mut Vec<Pos>, cap: usize, x1: i32, x2: i32, y1: i32, y2: i32, z: i32) {
    for x in span(x1, x2) {
        if list.len() >= cap {
            return;
        }
        for y in span(y1, y2) {
            list.push([x, y, z]);
        }
    }
}

pub fn wall_blocks(opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos) -> Vec<Pos> {
    let mut list = Vec::new();
    let ([x1, y1, z1], [x2, y2, z2]) = (p1, p2);
    if x1 == x2 {
        if !opts.hollow {
            x_wall(&mut list, cap, x1, y1, y2, z1, z2);
        } else {
            z_line(&mut list, z1, z2, x1, y1);
            z_line(&mut list, z1, z2, x1, y2);
            y_line(&mut list, y1, y2, x1, z1);
            y_line(&mut list, y1, y2, x1, z2);
        }
    } else if !opts.hollow {
        z_wall(&mut list, cap, x1, x2, y1, y2, z1);
    } else {
        x_line(&mut list, x1, x2, y1, z1);
        x_line(&mut list, x1, x2, y2, z1);
        y_line(&mut list, y1, y2, x1, z1);
        y_line(&mut list, y1, y2, x2, z1);
    }
    list
}

fn floor_full(list: &mut Vec<Pos>, cap: usize, x1: i32, x2: i32, y: i32, z1: i32, z2: i32) {
    for x in span(x1, x2) {
        if list.len() >= cap {
            return;
        }
        for z in span(z1, z2) {
            list.push([x, y, z]);
        }
    }
}
fn floor_hollow(list: &mut Vec<Pos>, x1: i32, x2: i32, y: i32, z1: i32, z2: i32) {
    x_line(list, x1, x2, y, z1);
    x_line(list, x1, x2, y, z2);
    z_line(list, z1, z2, x1, y);
    z_line(list, z1, z2, x2, y);
}

pub fn floor_blocks(hollow: bool, cap: usize, p1: Pos, p2: Pos) -> Vec<Pos> {
    let mut list = Vec::new();
    if hollow {
        floor_hollow(&mut list, p1[0], p2[0], p1[1], p1[2], p2[2]);
    } else {
        floor_full(&mut list, cap, p1[0], p2[0], p1[1], p1[2], p2[2]);
    }
    list
}

pub fn cube_blocks(fill: CubeFill, cap: usize, p1: Pos, p3: Pos) -> Vec<Pos> {
    let mut list = Vec::new();
    let ([x1, y1, z1], [x2, y2, z2]) = (p1, p3);
    match fill {
        CubeFill::Full => {
            for x in span(x1, x2) {
                for z in span(z1, z2) {
                    if list.len() >= cap {
                        return list;
                    }
                    for y in span(y1, y2) {
                        list.push([x, y, z]);
                    }
                }
            }
        }
        CubeFill::Hollow => {
            x_wall(&mut list, cap, x1, y1, y2, z1, z2);
            x_wall(&mut list, cap, x2, y1, y2, z1, z2);
            z_wall(&mut list, cap, x1, x2, y1, y2, z1);
            z_wall(&mut list, cap, x1, x2, y1, y2, z2);
            floor_full(&mut list, cap, x1, x2, y1, z1, z2);
            floor_full(&mut list, cap, x1, x2, y2, z1, z2);
        }
        CubeFill::Skeleton => {
            x_line(&mut list, x1, x2, y1, z1);
            x_line(&mut list, x1, x2, y1, z2);
            x_line(&mut list, x1, x2, y2, z1);
            x_line(&mut list, x1, x2, y2, z2);
            y_line(&mut list, y1, y2, x1, z1);
            y_line(&mut list, y1, y2, x1, z2);
            y_line(&mut list, y1, y2, x2, z1);
            y_line(&mut list, y1, y2, x2, z2);
            z_line(&mut list, z1, z2, x1, y1);
            z_line(&mut list, z1, z2, x1, y2);
            z_line(&mut list, z1, z2, x2, y1);
            z_line(&mut list, z1, z2, x2, y2);
        }
    }
    list
}

/// `DiagonalLine.getDiagonalLineBlocks`: sampled along the segment between block centres.
pub fn diagonal_line_blocks(p1: Pos, p2: Pos, sample_multiplier: f64) -> Vec<Pos> {
    let mut list: Vec<Pos> = Vec::new();
    let first = [p1[0] as f64 + 0.5, p1[1] as f64 + 0.5, p1[2] as f64 + 0.5];
    let second = [p2[0] as f64 + 0.5, p2[1] as f64 + 0.5, p2[2] as f64 + 0.5];
    let dist = len_sq(sub(second, first)).sqrt();
    let iterations = (dist * sample_multiplier).ceil() as i32;
    let step = 1.0 / iterations as f64;
    let mut t = 0.0;
    while t <= 1.0 {
        let lerp = [
            first[0] + (second[0] - first[0]) * t,
            first[1] + (second[1] - first[1]) * t,
            first[2] + (second[2] - first[2]) * t,
        ];
        let candidate = containing(lerp);
        if list.last() != Some(&candidate) {
            list.push(candidate);
        }
        t += step;
    }
    list
}

pub fn diagonal_wall_blocks(cap: usize, p1: Pos, p2: Pos, p3: Pos) -> Vec<Pos> {
    let line = diagonal_line_blocks(p1, p2, 1.0);
    let (lo, hi) = (p1[1].min(p3[1]), p1[1].max(p3[1]));
    let mut list = Vec::new();
    for y in lo..=hi {
        if list.len() >= cap {
            break;
        }
        for b in &line {
            list.push([b[0], y, b[2]]);
        }
    }
    list
}

pub fn slope_floor_blocks(opts: &ModeOptions, axis_limit: i32, cap: usize, p1: Pos, p2: Pos, p3: Pos) -> Vec<Pos> {
    let ([x1, y1, z1], [x2, _, z2], [_, y3, _]) = (p1, p2, p3);
    let mut list = Vec::new();
    let x_len = (x2 - x1).abs();
    let z_len = (z2 - z1).abs();
    let on_x_axis = if !opts.long_edge { z_len <= x_len } else { z_len > x_len };
    if on_x_axis {
        let line = diagonal_line_blocks([x1, y1, z1], [x2, y3, z1], 1.0);
        let lo = z1.min(z2);
        let mut hi = z1.max(z2);
        if hi - lo >= axis_limit {
            hi = lo + axis_limit - 1;
        }
        for z in lo..=hi {
            if list.len() >= cap {
                break;
            }
            for b in &line {
                list.push([b[0], b[1], z]);
            }
        }
    } else {
        let line = diagonal_line_blocks([x1, y1, z1], [x1, y3, z2], 1.0);
        let lo = x1.min(x2);
        let mut hi = x1.max(x2);
        if hi - lo >= axis_limit {
            hi = lo + axis_limit - 1;
        }
        for x in lo..=hi {
            if list.len() >= cap {
                break;
            }
            for b in &line {
                list.push([x, b[1], b[2]]);
            }
        }
    }
    list
}

fn ellipse_radius(cx: f32, cz: f32, rx: f32, rz: f32, x: i32, z: i32) -> f32 {
    let theta = ((z as f32 - cz) as f64).atan2((x as f32 - cx) as f64) as f32;
    let part1 = rx * rx * theta.sin() * theta.sin();
    let part2 = rz * rz * theta.cos() * theta.cos();
    rx * rz / (part1 + part2).sqrt()
}

pub fn circle_blocks(opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos) -> Vec<Pos> {
    let ([mut x1, y1, mut z1], [x2, _, z2]) = (p1, p2);
    let (mut cx, mut cz) = (x1 as f32, z1 as f32);
    if !opts.circle_center {
        cx = x1 as f32 + (x2 - x1) as f32 / 2.0;
        cz = z1 as f32 + (z2 - z1) as f32 / 2.0;
    } else {
        x1 = (cx - (x2 as f32 - cx)) as i32;
        z1 = (cz - (z2 as f32 - cz)) as i32;
    }
    let rx = (x2 as f32 - cx).abs();
    let rz = (z2 as f32 - cz).abs();
    let mut list = Vec::new();
    for x in span(x1, x2) {
        if list.len() >= cap {
            break;
        }
        for z in span(z1, z2) {
            // As in `sphere_blocks`: skip points surely outside (or, hollow, surely inside).
            let d2 = (cx - x as f32).powi(2) + (cz - z as f32).powi(2);
            let (r_max, r_min) = (rx.max(rz) + 0.41, rx.min(rz) - 0.61);
            if d2 >= r_max * r_max || opts.hollow && r_min > 0.0 && d2 <= r_min * r_min {
                continue;
            }
            let d = d2.sqrt();
            let r = ellipse_radius(cx, cz, rx, rz, x, z);
            let inside = d < r + 0.4;
            if inside && (!opts.hollow || d > r - 0.6) {
                list.push([x, y1, z]);
            }
        }
    }
    list
}

pub fn cylinder_blocks(opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos, p3: Pos) -> Vec<Pos> {
    let circle = circle_blocks(opts, cap, p1, p2);
    let (lo, hi) = (p1[1].min(p3[1]), p1[1].max(p3[1]));
    let mut list = Vec::new();
    for y in lo..=hi {
        if list.len() >= cap {
            break;
        }
        for b in &circle {
            list.push([b[0], y, b[2]]);
        }
    }
    list
}

pub fn sphere_blocks(opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos, p3: Pos) -> Vec<Pos> {
    let ([mut x1, mut y1, mut z1], [x2, _, z2], [x3, y3, z3]) = (p1, p2, p3);
    let (mut cx, mut cy, mut cz) = (x1 as f32, y1 as f32, z1 as f32);
    if !opts.circle_center {
        cx = x1 as f32 + (x2 - x1) as f32 / 2.0;
        cy = y1 as f32 + (y3 - y1) as f32 / 2.0;
        cz = z1 as f32 + (z2 - z1) as f32 / 2.0;
    } else {
        x1 = (cx - (x2 as f32 - cx)) as i32;
        y1 = (cy - (y3 as f32 - cy)) as i32;
        z1 = (cz - (z2 as f32 - cz)) as i32;
    }
    let rx = (x2 as f32 - cx).abs();
    let ry = (y3 as f32 - cy).abs();
    let rz = (z2 as f32 - cz).abs();
    // The radius in any direction lies between the smallest and the largest of the three, so a
    // point farther than the largest (or, hollow, nearer than the smallest) is skipped without
    // working its radius out; the margin covers rounding. Same blocks, far less work.
    let r_max = rx.max(ry).max(rz) + 0.4 + 0.01;
    let r_min = rx.min(ry).min(rz) - 0.6 - 0.01;
    let mut list = Vec::new();
    for x in span(x1, x3) {
        for z in span(z1, z3) {
            if list.len() >= cap {
                return list;
            }
            let dxz2 = (cx - x as f32).powi(2) + (cz - z as f32).powi(2);
            if dxz2 >= r_max * r_max {
                continue;
            }
            for y in span(y1, y3) {
                let d2 = dxz2 + (cy - y as f32).powi(2);
                if d2 >= r_max * r_max || opts.hollow && r_min > 0.0 && d2 <= r_min * r_min {
                    continue;
                }
                let d = d2.sqrt();
                let rxz = ellipse_radius(cx, cz, rx, rz, x, z);
                let r = ellipse_radius(cx, cy, rxz, ry, x, y);
                let inside = d < r + 0.4;
                if inside && (!opts.hollow || d > r - 0.6) {
                    list.push([x, y, z]);
                }
            }
        }
    }
    list
}

impl BuildMode {
    /// Two-click modes: every block between the two points. Generation stops at about `cap`
    /// blocks (the most placed at once), so a huge shape never costs more than that.
    pub fn two_click_blocks(self, opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos) -> Vec<Pos> {
        let mut list = match self {
            BuildMode::Line => line_blocks(p1, p2),
            BuildMode::Wall => wall_blocks(opts, cap, p1, p2),
            BuildMode::Floor => floor_blocks(opts.hollow, cap, p1, p2),
            BuildMode::Circle => circle_blocks(opts, cap, p1, p2),
            _ => vec![p1],
        };
        list.truncate(cap);
        list
    }

    /// Three-click modes after the first click: what the first two points show.
    pub fn intermediate_blocks(self, opts: &ModeOptions, cap: usize, p1: Pos, p2: Pos) -> Vec<Pos> {
        let mut list = match self {
            BuildMode::Cube => floor_blocks(opts.cube_fill == CubeFill::Skeleton, cap, p1, p2),
            BuildMode::DiagonalLine => diagonal_line_blocks(p1, p2, 10.0),
            BuildMode::DiagonalWall => diagonal_line_blocks(p1, p2, 1.0),
            BuildMode::SlopeFloor => floor_blocks(opts.hollow, cap, p1, p2),
            BuildMode::Cylinder | BuildMode::Sphere => circle_blocks(opts, cap, p1, p2),
            _ => vec![p1],
        };
        list.truncate(cap);
        list
    }

    /// Three-click modes: the finished shape.
    pub fn final_blocks(self, opts: &ModeOptions, axis_limit: i32, cap: usize, p1: Pos, p2: Pos, p3: Pos) -> Vec<Pos> {
        let mut list = match self {
            BuildMode::Cube => cube_blocks(opts.cube_fill, cap, p1, p3),
            BuildMode::DiagonalLine => diagonal_line_blocks(p1, p3, 10.0),
            BuildMode::DiagonalWall => diagonal_wall_blocks(cap, p1, p2, p3),
            BuildMode::SlopeFloor => slope_floor_blocks(opts, axis_limit, cap, p1, p2, p3),
            BuildMode::Cylinder => cylinder_blocks(opts, cap, p1, p2, p3),
            BuildMode::Sphere => sphere_blocks(opts, cap, p1, p2, p3),
            _ => vec![p1],
        };
        list.truncate(cap);
        list
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const M: usize = usize::MAX;

    #[test]
    fn huge_shapes_stop_at_the_cap() {
        let o = ModeOptions::default();
        let t = std::time::Instant::now();
        assert_eq!(BuildMode::Cube.final_blocks(&o, 1000, 10_000, [0, 0, 0], [999, 0, 999], [999, 999, 999]).len(), 10_000);
        assert_eq!(BuildMode::Sphere.final_blocks(&o, 1000, 10_000, [0, 0, 0], [999, 0, 999], [999, 999, 999]).len(), 10_000);
        assert_eq!(BuildMode::Cylinder.final_blocks(&o, 1000, 10_000, [0, 0, 0], [999, 0, 999], [999, 999, 999]).len(), 10_000);
        assert_eq!(BuildMode::Floor.two_click_blocks(&o, 10_000, [0, 0, 0], [999, 0, 999]).len(), 10_000);
        let hollow = ModeOptions { hollow: true, ..o };
        assert_eq!(BuildMode::Sphere.final_blocks(&hollow, 1000, 10_000, [0, 0, 0], [999, 0, 999], [999, 999, 999]).len(), 10_000);
        assert!(t.elapsed().as_millis() < 500, "took {:?}", t.elapsed());
    }

    fn set(v: &[Pos]) -> std::collections::BTreeSet<Pos> {
        v.iter().copied().collect()
    }

    #[test]
    fn line_runs_along_the_longest_axis() {
        // Standing next to the first block, looking along +X a bit down. (Further back, the
        // plane through the first block is nearer than the ground and the line goes up, as in
        // the original.)
        let start = [1.5, 1.62, 0.5];
        let look = look_vec(-90.0, 10.0);
        let p = find_line(start, look, [2, 0, 0], 50).unwrap();
        assert_eq!(p[1], 0);
        assert_eq!(p[2], 0);
        assert!(p[0] > 2);
        assert_eq!(line_blocks([2, 0, 0], p).len() as i32, p[0] - 1);
    }

    #[test]
    fn floor_stays_on_first_plane() {
        let start = [0.5, 5.62, 0.5];
        let look = look_vec(-45.0, 30.0);
        let p = find_floor(start, look, [3, 2, 3], 50).unwrap();
        assert_eq!(p[1], 2);
        assert!(find_floor(start, look_vec(0.0, -30.0), [3, 2, 3], 50).is_none(), "looking up never meets a floor below");
    }

    #[test]
    fn wall_keeps_one_axis() {
        let start = [0.5, 1.62, 0.5];
        let look = look_vec(-60.0, -10.0);
        let p = find_wall(start, look, [5, 0, 3], 50).unwrap();
        assert!(p[0] == 5 || p[2] == 3);
    }

    #[test]
    fn height_stays_above_second() {
        let start = [0.5, 1.62, 0.5];
        let look = look_vec(-45.0, -20.0);
        let p = find_height(start, look, [4, 0, 4], 50).unwrap();
        assert_eq!((p[0], p[2]), (4, 4));
    }

    #[test]
    fn walls_and_cubes() {
        let o = ModeOptions::default();
        assert_eq!(wall_blocks(&o, M, [0, 0, 0], [0, 2, 3]).len(), 12);
        let hollow = ModeOptions { hollow: true, ..o };
        assert_eq!(set(&wall_blocks(&hollow, M, [0, 0, 0], [0, 2, 3])).len(), 10);
        assert_eq!(set(&cube_blocks(CubeFill::Full, M, [0, 0, 0], [2, 2, 2])).len(), 27);
        assert_eq!(set(&cube_blocks(CubeFill::Hollow, M, [0, 0, 0], [2, 2, 2])).len(), 26);
        assert_eq!(set(&cube_blocks(CubeFill::Skeleton, M, [0, 0, 0], [2, 2, 2])).len(), 20);
    }

    #[test]
    fn diagonal_line_is_connected() {
        let l = diagonal_line_blocks([0, 0, 0], [5, 3, 0], 10.0);
        assert_eq!(l.first(), Some(&[0, 0, 0]));
        assert_eq!(l.last(), Some(&[5, 3, 0]));
    }

    #[test]
    fn circles() {
        let o = ModeOptions::default();
        let c = set(&circle_blocks(&o, M, [0, 0, 0], [6, 0, 6]));
        assert!(c.contains(&[3, 0, 3]) && c.contains(&[0, 0, 3]) && !c.contains(&[0, 0, 0]));
        let hollow = ModeOptions { hollow: true, ..o };
        assert!(!set(&circle_blocks(&hollow, M, [0, 0, 0], [6, 0, 6])).contains(&[3, 0, 3]));
        let centered = ModeOptions { circle_center: true, ..o };
        let c = set(&circle_blocks(&centered, M, [0, 0, 0], [3, 0, 3]));
        assert!(c.contains(&[-3, 0, 0]) && c.contains(&[3, 0, 0]));
        let s = set(&sphere_blocks(&o, M, [0, 0, 0], [4, 0, 4], [4, 4, 4]));
        assert!(s.contains(&[2, 2, 2]) && !s.contains(&[0, 0, 0]));
    }

    #[test]
    fn limit() {
        assert_eq!(limit_axis([0, 0, 0], [20, -20, 3], 8), [7, -7, 3]);
    }
}
