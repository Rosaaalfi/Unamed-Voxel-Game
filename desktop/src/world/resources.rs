//! ECS Resources shared across world systems.
//!
//! Resources hold global state:
//! - `WorldModels` – cached mesh / material handles created from pack assets.
//! - `PhysicsState` – per-entity velocity map (placeholder for proper physics).
//! - `CameraState` – current camera mode for orbiting / switching.

use bevy::prelude::*;
use shared::world::BlockKind;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphicsQuality {
    Low,
    Medium,
    Fancy,
    Fabulous,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkBuilderMode {
    Threaded,
    Single,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct TransformEditorState {
    pub translation: Vec3,
    pub left_rotation: Quat,
    pub scale: Vec3,
    pub right_rotation: Quat,
    pub matrix: Mat4,
}

impl TransformEditorState {
    pub fn right_hand_default() -> Self {
        Self {
            translation: Vec3::new(0.42, -0.36, -0.78),
            left_rotation: Quat::from_euler(EulerRot::XYZ, -0.35, 0.18, -0.08),
            scale: Vec3::ONE,
            right_rotation: Quat::IDENTITY,
            matrix: Mat4::IDENTITY,
        }
    }

    pub fn left_hand_default() -> Self {
        Self {
            translation: Vec3::new(-0.42, -0.36, -0.78),
            left_rotation: Quat::from_euler(EulerRot::XYZ, -0.35, -0.18, 0.08),
            scale: Vec3::ONE,
            right_rotation: Quat::IDENTITY,
            matrix: Mat4::IDENTITY,
        }
    }
}

#[derive(Resource, Clone)]
pub struct GameSettings {
    pub fov_degrees: f32,
    pub render_distance: i32,
    pub graphics_quality: GraphicsQuality,
    pub vsync: bool,
    pub fullscreen: bool,
    pub fullscreen_resolution: (u32, u32),
    pub chunk_builder: ChunkBuilderMode,
    pub max_framerate: u32,
    pub smooth_lighting: bool,
    pub gui_scale: f32,
    pub view_bobbing: bool,
    pub first_person_right: TransformEditorState,
    pub first_person_left: TransformEditorState,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            fov_degrees: 70.0,
            render_distance: 7,
            graphics_quality: GraphicsQuality::Medium,
            vsync: true,
            fullscreen: false,
            fullscreen_resolution: (1280, 720),
            chunk_builder: ChunkBuilderMode::Threaded,
            max_framerate: 120,
            smooth_lighting: true,
            gui_scale: 2.0,
            view_bobbing: true,
            first_person_right: TransformEditorState::right_hand_default(),
            first_person_left: TransformEditorState::left_hand_default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolKind {
    WoodenPickaxe,
    WoodenAxe,
    WoodenSword,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    Block(BlockKind),
    Tool(ToolKind),
}

impl ItemKind {
    pub const fn place_block(self) -> Option<BlockKind> {
        match self {
            Self::Block(kind) if kind.is_solid() => Some(kind),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------------------
// WorldModels
// ---------------------------------------------------------------------------

/// Holds mesh/material handles generated from pack block models.
#[derive(Resource, Default)]
pub struct WorldModels {
    /// Per-block-kind mesh handles built from Bedrock block-model JSON.
    pub block_meshes: HashMap<BlockKind, Handle<Mesh>>,
    /// Shared material used for all blocks (single atlas texture).
    pub block_material: Option<Handle<StandardMaterial>>,
}

// ---------------------------------------------------------------------------
// PhysicsState
// ---------------------------------------------------------------------------

/// Simple velocity store — one day this will be a proper physics engine.
#[derive(Resource)]
pub struct PhysicsState {
    /// Per-entity velocity in world units / s.
    pub velocities: HashMap<Entity, Vec3>,
    /// Currently selected hotbar slot (0-8).
    pub hotbar_slot: usize,
    /// Sprint is toggled by Ctrl, then cleared automatically when movement stops.
    pub sprinting: bool,
    /// Items available in each hotbar slot. `None` means empty.
    pub hotbar_items: [Option<ItemKind>; 9],
    /// Main inventory storage slots.
    pub inventory_items: [Option<ItemKind>; 27],
}

impl Default for PhysicsState {
    fn default() -> Self {
        Self {
            velocities: HashMap::new(),
            hotbar_slot: 0,
            sprinting: false,
            hotbar_items: [
                Some(ItemKind::Block(BlockKind::Grass)),
                Some(ItemKind::Block(BlockKind::Dirt)),
                Some(ItemKind::Block(BlockKind::Stone)),
                Some(ItemKind::Tool(ToolKind::WoodenPickaxe)),
                Some(ItemKind::Block(BlockKind::Sand)),
                Some(ItemKind::Block(BlockKind::OakLog)),
                Some(ItemKind::Block(BlockKind::OakLeaves)),
                Some(ItemKind::Tool(ToolKind::WoodenAxe)),
                Some(ItemKind::Tool(ToolKind::WoodenSword)),
            ],
            inventory_items: [
                Some(ItemKind::Block(BlockKind::Cobblestone)),
                Some(ItemKind::Block(BlockKind::Sand)),
                Some(ItemKind::Block(BlockKind::OakLog)),
                Some(ItemKind::Block(BlockKind::OakLeaves)),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            ],
        }
    }
}

// ---------------------------------------------------------------------------
// World Time
// ---------------------------------------------------------------------------

#[derive(Resource)]
pub struct WorldTime {
    pub tick: u64,
    pub time_of_day: f32,
    pub day: u64,
}

impl Default for WorldTime {
    fn default() -> Self {
        Self {
            tick: 6000,
            time_of_day: 0.25,
            day: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// CameraState
// ---------------------------------------------------------------------------

/// Describes the current camera-relative viewpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraMode {
    FirstPerson,
    ThirdPersonBehind,
    ThirdPersonFront,
}

impl Default for CameraMode {
    fn default() -> Self {
        Self::FirstPerson
    }
}

/// Tracks which camera mode is active and provides a place for future
/// camera-configuration fields (FoV, zoom, smoothing …).
#[derive(Resource)]
pub struct CameraState {
    pub mode: CameraMode,
    pub yaw: f32,
    pub pitch: f32,
    pub sensitivity: f32,
    pub body_yaw: f32,
    pub attack_timer: f32,
    pub use_timer: f32,
}

impl Default for CameraState {
    fn default() -> Self {
        Self {
            mode: CameraMode::default(),
            yaw: 0.0,
            pitch: 0.0,
            sensitivity: 0.003,
            body_yaw: 0.0,
            attack_timer: 0.0,
            use_timer: 0.0,
        }
    }
}

// ---------------------------------------------------------------------------
// ChunkManager
// ---------------------------------------------------------------------------

/// Manages chunk loading/unloading based on player position and render distance.
#[derive(Resource)]
pub struct ChunkManager {
    /// Number of chunks to render in each direction from the player (±render_radius).
    pub render_radius: i32,
    /// Currently loaded chunk columns (chunk-space IVec2 positions).
    pub loaded_chunks: HashSet<IVec2>,
    /// Shared atlas material used by generated chunk meshes.
    pub terrain_material: Handle<StandardMaterial>,
    /// Transparent material for water meshes.
    pub water_material: Handle<StandardMaterial>,
}
