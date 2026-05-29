//! ECS Components for the world / gameplay layer.
//!
//! Each component is a small, focused data type attached to entities.
//! Use marker components (`PlayerController`, `SolidBlock`, `WaterBlock`)
//! to let queries select exactly the entities they need.

use bevy::prelude::*;

// ---------------------------------------------------------------------------
// Player
// ---------------------------------------------------------------------------

/// Marks an entity as the player-controlled character.
/// The player-input and physics systems use this to find the player entity.
#[derive(Component)]
pub struct PlayerController;

/// Marks the camera that follows the player (first-person child or third-person).
#[derive(Component)]
pub struct PlayerCamera;

#[derive(Component, Clone, Debug)]
pub struct EntityTransformState {
    pub translation: Vec3,
    pub left_rotation: Quat,
    pub scale: Vec3,
    pub right_rotation: Quat,
    pub matrix: Mat4,
}

impl Default for EntityTransformState {
    fn default() -> Self {
        Self {
            translation: Vec3::ZERO,
            left_rotation: Quat::IDENTITY,
            scale: Vec3::ONE,
            right_rotation: Quat::IDENTITY,
            matrix: Mat4::IDENTITY,
        }
    }
}

#[derive(Component, Clone, Copy, Debug)]
#[allow(dead_code)]
pub struct EntityAabb {
    pub half_extents: Vec3,
}

impl EntityAabb {
    pub const fn player() -> Self {
        Self {
            half_extents: Vec3::new(0.30, 0.90, 0.30),
        }
    }
}

// ---------------------------------------------------------------------------
// Blocks
// ---------------------------------------------------------------------------

/// Marks an entity as a solid, collidable block (stone, dirt, …).
#[derive(Component)]
#[allow(dead_code)]
pub struct SolidBlock;

/// Marks an entity as a water block (non-solid, transparent, buoyant).
#[derive(Component)]
#[allow(dead_code)]
pub struct WaterBlock;

#[derive(Component)]
pub struct BlockOutline;

#[derive(Component)]
pub struct SkyDome;

#[derive(Component)]
pub struct LightweightCloud;

#[derive(Component)]
pub struct CelestialBillboard;

#[derive(Component)]
pub struct SunLight;

#[derive(Component)]
pub struct SunVisual;

#[derive(Component)]
pub struct MoonVisual;

#[derive(Component)]
pub struct StarVisual;

/// Marks a separate water mesh entity for a chunk (transparent).
#[derive(Component)]
pub struct WaterChunkEntity;

#[derive(Component)]
pub struct ChunkLoadAnimation {
    pub age: f32,
}

#[derive(Component)]
pub struct ChunkUnloadAnimation {
    pub age: f32,
}
