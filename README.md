# ⛏️ Unnamed Craft

[![Rust](https://img.shields.io/badge/rust-2024-dea584?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org)
[![Bevy](https://img.shields.io/badge/Bevy-0.18-232326?style=for-the-badge&logo=bevy&logoColor=white)](https://bevyengine.org)
[![License](https://img.shields.io/badge/license-MIT-blue?style=for-the-badge)](LICENSE)
[![CI](https://img.shields.io/badge/CI-passing-brightgreen?style=for-the-badge)](#)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-blue?style=for-the-badge)](#)
[![PRs](https://img.shields.io/badge/PRs-welcome-orange?style=for-the-badge)](#)

**Unnamed Craft** is a Minecraft-inspired voxel game built with [Rust](https://www.rust-lang.org) and [Bevy 0.18](https://bevyengine.org). It features a fully functional voxel world with terrain generation, block interaction, player physics, third-person and first-person cameras, a resource pack system, and dedicated server support — all powered by the Bevy ECS framework with a Vulkan/Metal rendering backend.

> ⚠️ **Work in Progress:** This project is under active development. Features are being added and refined regularly.

---

## ✨ Features

### 🌍 World & Terrain
- **Procedural terrain** — Seeded fractal value-noise terrain generation with plains, forests, deserts, and ocean biomes
- **Sea level**, beaches, and oak tree placement
- **Chunk-based world** with streaming load/unload, distance-based LOD (2×/4× grouping)
- **Block types:** Grass, Dirt, Stone, Sand, Water, Oak Log, Oak Leaves, Cobblestone
- **Semi-transparent water** with underwater fog and overlay effects

### 🎮 Gameplay
- **First-person & third-person** camera modes (behind & front)
- **WASD movement** + Space jump + Ctrl sprint
- **Left-click break** / **Right-click place** blocks with raycast targeting
- **Inventory system** — 27-slot main storage, hotbar with 9 quick slots, item pick/swap/drop
- **Hotbar item icons** — auto-generated isometric block previews
- **Block target outline** preview

### 🖼️ Rendering & Visuals
- **PBR rendering** with directional light + shadow cascades (4 splits)
- **Day/night cycle** with sun/moon orbit, moon phases, and starfield
- **Bloom**, fog, and volumetric light effects
- **Sky gradient dome** with billboard sun/moon and cloud groups
- **Deferred depth/normal prepass** pipeline scaffolding
- **Vulkan backend** (desktop), Metal (iOS/macOS)

### 🧑 Player
- **Animated player model** loaded from Bedrock `player.geo.json` with bone animation (walk, run, attack, interact)
- **First-person hands** with held item 3D mesh preview
- **Third-person held item** attached to right arm
- **Full-body shadow** while in first-person view
- **AABB collision** with gravity and ground clamping

### 📦 Asset System
- **Bedrock JSON model pipeline** — entity geometry, block models, per-face UVs
- **Resource pack override system** — client-side custom textures, models, and UI
- **Texture atlas** generation for terrain, items, and GUI sprites
- **Minecraft-style HUD** — hearts, hotbar, crosshair, inventory panels, pause menu, F3 debug screen

### 🖥️ Multiplayer-Ready Architecture
- **Dedicated headless server** (`game-server` crate)
- **Local server** auto-started for single-player
- **Network protocol** scaffolding for future P2P/coop support

---

## 🏗️ Project Structure

```
├── desktop/               # Desktop client (Vulkan backend)
│   ├── src/
│   │   ├── main.rs        # Entry point, plugin registration
│   │   ├── player.rs      # Player model, bone animation
│   │   ├── debug.rs       # F3 debug overlay
│   │   ├── world/          # Chunk rendering, physics, camera, input
│   │   └── ui/             # HUD, inventory, pause menu, settings
│   └── pack/unamed/        # Desktop-specific resource pack overrides
│
├── mobile/                # Mobile client (Android / iOS)
│   ├── src/
│   │   └── main.rs
│   └── pack/unamed/
│
├── server/                # Headless dedicated server
│   ├── src/
│   │   ├── main.rs
│   │   └── lib.rs
│
├── shared/                # Common library
│   ├── src/
│   │   ├── lib.rs         # Game constants, network config
│   │   ├── world.rs       # BlockKind enum, world generation
│   │   ├── bedrock.rs     # Bedrock JSON model parsers
│   │   └── pack.rs        # Resource pack resolver
│   ├── pack/unamed/       # Base resource pack assets
│   │   ├── atlases/       # Texture atlas definitions
│   │   ├── models/        # Bedrock geometry models (blocks, entities)
│   │   ├── textures/      # Block/item/GUI textures
│   │   ├── animations/    # Player animation JSON
│   │   ├── shaders/       # HLSL shader sources
│   │   └── sounds/        # Audio assets
│
├── RealMinecraftExample/  # Vanilla Minecraft reference (inspected for accuracy)
├── memory/                # Development notes & handoff documents
└── Cargo.toml             # Workspace root
```

---

## 🚀 Getting Started

### Prerequisites

- **Rust** — Install via [rustup](https://rustup.rs/) (minimum edition 2024)
- **Vulkan SDK** (Windows/Linux) or **MoltenVK** (macOS) — Required by Bevy for GPU rendering
- **Cargo** — Comes with Rust toolchain

### Build & Run

#### Quick Check (fast compilation)
```powershell
cargo check -p client-desktop
```

#### Full Build
```powershell
cargo build -p client-desktop
```

#### Run Desktop Client
```powershell
$env:RUST_LOG="info"
cargo run -p client-desktop
```

#### Run Dedicated Server
```powershell
$env:RUST_LOG="info"
cargo run -p game-server
```

#### Run Tests
```powershell
cargo test -p shared
```

> 📝 The desktop client automatically starts a local server in the background for single-player mode.

---

## 🎮 Controls

| Action | Input |
|--------|-------|
| Move | `W` `A` `S` `D` |
| Jump | `Space` |
| Sprint (hold) | `Ctrl` |
| Break block | `Left Mouse Button` |
| Place block | `Right Mouse Button` |
| Select hotbar slot | `1`–`9` or `Mouse Wheel` |
| Open Inventory | `E` |
| Pause / Menu | `Escape` |
| Toggle Debug (F3) | `F3` |
| Camera mode | (in-game toggle) |

---

## 🧰 Tech Stack

| Technology | Purpose |
|------------|---------|
| [Rust](https://www.rust-lang.org) | Systems language — safety, performance, zero-cost abstractions |
| [Bevy 0.18](https://bevyengine.org) | ECS game engine — rendering, physics, asset management, UI |
| [wgpu](https://wgpu.rs/) | Cross-platform GPU abstraction (Vulkan, Metal, DX12) |
| [serde](https://serde.rs/) | Serialization — Bedrock JSON model parsing |
| [image](https://crates.io/crates/image) | PNG texture loading |

---

## 📸 Screenshots

> _Coming soon — screenshots of the current build will be added here._

---

## 🤝 Contributing

Contributions, issues, and feature requests are welcome! Feel free to open a [pull request](../../pulls) or an [issue](../../issues).

Before contributing, please check the [`memory/`](memory/) directory for development notes, API gotchas, and active work tracking.

---

## 📄 License

This project is licensed under the **MIT License**. See the [LICENSE](LICENSE) file for details.

---

## 🙏 Acknowledgments

- **[Minecraft](https://minecraft.net)** by Mojang Studios — the original inspiration and reference
- **[Bevy Engine](https://bevyengine.org)** community — for the amazing open-source game engine
- **[Bedrock JSON format](https://wiki.bedrock.dev)** — model and animation format reference
- All contributors and testers who have helped shape this project

---

<p align="center">
  Built with ❤️ using Rust & Bevy
</p>
