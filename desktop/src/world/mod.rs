//! World plugin — terrain, block rendering, physics, player input, camera.
//!
//! This module is organised by ECS concern:
//! - [`components`] – marker components (`PlayerController`, `SolidBlock`, …).
//! - [`resources`]  – global resources (`WorldModels`, `PhysicsState`, …).
//! - [`mesh`]       – Bedrock geometry / block-model mesh builders.
//! - [`setup`]      – startup systems that prepare assets and spawn the scene.
//! - [`systems`]    – per-tick systems (gravity, input, camera).

use bevy::prelude::*;

pub mod chunk;
pub mod components;
pub mod mesh;
pub mod resources;
pub mod setup;
pub mod systems;

/// Player eye height / collision height.
pub const PLAYER_HEIGHT: f32 = 1.8;

/// Registers all world-related plugins, resources, and systems.
pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<resources::WorldModels>()
            .init_resource::<resources::PhysicsState>()
            .init_resource::<resources::CameraState>()
            .init_resource::<resources::WorldTime>()
            .init_resource::<resources::GameSettings>()
            .add_systems(
                Startup,
                (setup::prepare_block_models, setup::spawn_player_and_terrain),
            )
            .add_systems(
                Update,
                (
                    systems::apply_gravity_system,
                    systems::mouse_look_system,
                    systems::camera_follow_system,
                    systems::player_input_system,
                    systems::camera_control_system,
                    systems::cursor_grab_system,
                    systems::sync_entity_transform_state,
                    systems::apply_settings_system,
                    systems::update_sky_follow_system,
                    systems::update_world_time_system,
                    systems::update_underwater_graphics_system,
                    systems::animate_water_system,
                    chunk::animate_chunk_loads,
                    chunk::animate_chunk_unloads,
                    chunk::update_water_flow,
                    systems::update_block_outline_system,
                    chunk::update_chunk_loading,
                ),
            );
    }
}
