# Effortless Building for Horizon

An unofficial port of [Effortless Building](https://github.com/Requios/effortless-building) by **Requios**
(LGPL; ported from its `1.21` branch) to [Horizon](https://github.com/horizon-rust/horizon) as a native mod.
All credit for the design, the behaviour and the textures goes to Requios; bugs in this port are mine.

## What's ported

- **Build modes** (hold **Left Alt** for the radial menu, then click or release over a mode): Disable, Single,
  Line, Wall, Floor, Cube, Diagonal Line, Diagonal Wall, Slope Floor, Circle, Cylinder, Sphere. Second and third
  points follow your view along lines and planes through the first block, as in the original.
- **Mode options** in the radial menu: build speed, filled / hollow, cube skeleton, raised edge, circle start
  (corner / middle).
- **Quick replace** (creative): only air, blocks and air, only blocks, filtered by offhand; protect tile entities.
- **Modifiers** (**keypad +**): Mirror, Array and Radial Mirror, with the original's settings screen and their
  planes drawn in the world.
- **Undo / redo** (**Ctrl+Z / Ctrl+Y**, 50 steps), placement preview with the appear animation, block counts in
  the action bar, creative reach 200 and up to 10,000 blocks a click.

## Not ported yet

- The preview shows each block in its map colour (Horizon's mod API can't draw block textures in the world yet).
- Placed blocks are turned by their `facing` / `axis` / `half` / `type` properties, not by each block's own
  placement logic, so a few blocks may face differently than in vanilla.
- Survival is fixed at power level 0: the reach upgrades, power-level items and randomizer bags aren't ported.
- Modifier settings aren't saved between sessions.

## Build

```sh
cargo horizon-mod build          # target/horizon-mod/effortless.hmod
cargo horizon-mod check target/horizon-mod/effortless.hmod
```

## Licence

LGPL-2.1-only, as the original (its licence text is in `LICENSE`). Textures in `assets/effortless/textures/gui/`
are the original mod's; see `assets/effortless/textures/gui/license.txt` and `NOTICE.md`.
