//! Debug / developer overlays (F3 keybinds).
//!
//! | Keybind   | Toggle | Feature                         |
//! |-----------|--------|---------------------------------|
//! | F3 + B    | ✓      | Block bounding-box outlines     |
//! | F3 + E    | ✓      | Entity bounding-box outlines    |
//! | F3 + A    | ✓      | Chunk boundary lines            |
//! | F3 + V    | ✓      | Void / chunk wireframe overlay  |

use bevy::diagnostic::{DiagnosticsStore, FrameTimeDiagnosticsPlugin};
use bevy::prelude::*;

use crate::world::chunk::Chunk;
use crate::world::components::PlayerCamera;
use crate::world::components::PlayerController;
use crate::world::resources::{CameraState, ChunkManager, PhysicsState};

/// Number of blocks along X and Z per chunk (must match chunk.rs).
const CHUNK_SIZE: usize = 16;
/// Height of the chunk block grid.
const CHUNK_HEIGHT: usize = 16;

// ---------------------------------------------------------------------------
// Resources
// ---------------------------------------------------------------------------

/// Global debug toggle state.
#[derive(Resource, Default)]
pub struct DebugState {
    /// Show the Minecraft-style F3 text screen.
    pub show_screen: bool,
    /// Show AABB wireframe for every block in loaded chunks.
    pub show_block_aabb: bool,
    /// Show AABB wireframe for every non-block entity.
    pub show_entity_aabb: bool,
    /// Show chunk-boundary debug lines.
    pub show_chunk_lines: bool,
    /// Show chunk wireframe (all face edges) for visual debugging.
    pub show_chunk_wireframe: bool,
}

// ---------------------------------------------------------------------------
// Plugin
// ---------------------------------------------------------------------------

pub struct DebugPlugin;

impl Plugin for DebugPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(FrameTimeDiagnosticsPlugin::default())
            .init_resource::<DebugState>()
            .add_systems(Startup, spawn_debug_screen)
            .add_systems(Update, debug_keybind_system)
            .add_systems(Update, update_debug_screen)
            .add_systems(PostUpdate, debug_render_overlay_system);
    }
}

#[derive(Component)]
struct DebugScreenRoot;

#[derive(Component)]
struct DebugScreenText;

// ---------------------------------------------------------------------------
// Keybind input
// ---------------------------------------------------------------------------

/// Listens for F3+key combos and toggles the corresponding debug flags.
fn debug_keybind_system(keyboard: Res<ButtonInput<KeyCode>>, mut state: ResMut<DebugState>) {
    if keyboard.just_pressed(KeyCode::F3) {
        state.show_screen = !state.show_screen;
    }
    if keyboard.all_pressed([KeyCode::F3]) && keyboard.just_pressed(KeyCode::KeyB) {
        state.show_block_aabb = !state.show_block_aabb;
        info!(
            "debug: block AABB {}",
            if state.show_block_aabb { "ON" } else { "OFF" }
        );
    }
    if keyboard.all_pressed([KeyCode::F3]) && keyboard.just_pressed(KeyCode::KeyE) {
        state.show_entity_aabb = !state.show_entity_aabb;
        info!(
            "debug: entity AABB {}",
            if state.show_entity_aabb { "ON" } else { "OFF" }
        );
    }
    if keyboard.all_pressed([KeyCode::F3]) && keyboard.just_pressed(KeyCode::KeyA) {
        state.show_chunk_lines = !state.show_chunk_lines;
        info!(
            "debug: chunk lines {}",
            if state.show_chunk_lines { "ON" } else { "OFF" }
        );
    }
    if keyboard.all_pressed([KeyCode::F3]) && keyboard.just_pressed(KeyCode::KeyV) {
        state.show_chunk_wireframe = !state.show_chunk_wireframe;
        info!(
            "debug: chunk wireframe {}",
            if state.show_chunk_wireframe {
                "ON"
            } else {
                "OFF"
            }
        );
    }
}

fn spawn_debug_screen(mut commands: Commands) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(4.0),
                top: Val::Px(4.0),
                width: Val::Px(520.0),
                height: Val::Auto,
                padding: UiRect::all(Val::Px(4.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.36)),
            Visibility::Hidden,
            DebugScreenRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::WHITE),
                DebugScreenText,
            ));
        });
}

fn update_debug_screen(
    state: Res<DebugState>,
    diagnostics: Res<DiagnosticsStore>,
    cam_state: Res<CameraState>,
    physics: Res<PhysicsState>,
    chunk_manager: Option<Res<ChunkManager>>,
    camera: Query<&Transform, With<PlayerCamera>>,
    player: Query<&Transform, (With<PlayerController>, Without<PlayerCamera>)>,
    chunks: Query<&Chunk>,
    mut roots: Query<&mut Visibility, With<DebugScreenRoot>>,
    mut texts: Query<&mut Text, With<DebugScreenText>>,
) {
    for mut visibility in &mut roots {
        *visibility = if state.show_screen {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    if !state.show_screen {
        return;
    }

    let fps = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FPS)
        .and_then(|fps| fps.smoothed())
        .unwrap_or(0.0);
    let frame_ms = diagnostics
        .get(&FrameTimeDiagnosticsPlugin::FRAME_TIME)
        .and_then(|frame| frame.smoothed())
        .unwrap_or(0.0);
    let player_pos = player
        .single()
        .map(|transform| transform.translation)
        .unwrap_or(Vec3::ZERO);
    let camera_pos = camera
        .single()
        .map(|transform| transform.translation)
        .unwrap_or(Vec3::ZERO);
    let chunk = IVec2::new(
        (player_pos.x / CHUNK_SIZE as f32).floor() as i32,
        (player_pos.z / CHUNK_SIZE as f32).floor() as i32,
    );
    let velocity = physics
        .velocities
        .values()
        .next()
        .copied()
        .unwrap_or(Vec3::ZERO);
    let loaded_chunk_count = chunks.iter().count();
    let render_radius = chunk_manager
        .as_ref()
        .map(|manager| manager.render_radius)
        .unwrap_or(0);

    let content = format!(
        "Minecraft Bevy 0.18 (Vulkan)\n\
         FPS: {fps:.0} ({frame_ms:.2} ms)\n\
         XYZ: {px:.2} / {py:.2} / {pz:.2}\n\
         Camera XYZ: {cx:.2} / {cy:.2} / {cz:.2}\n\
         Chunk: {chunk_x}, {chunk_z}  Loaded: {loaded_chunk_count}  Radius: {render_radius}\n\
         Facing yaw/pitch: {yaw:.1} / {pitch:.1} deg  Mode: {mode:?}\n\
         Velocity: {vx:.2}, {vy:.2}, {vz:.2}  Sprint: {sprint}\n\
         Hotbar Slot: {slot}\n\
         Debug Toggles: F3+B blocks={blocks}  F3+E entities={entities}  F3+A chunks={chunk_lines}  F3+V wire={wire}",
        px = player_pos.x,
        py = player_pos.y,
        pz = player_pos.z,
        cx = camera_pos.x,
        cy = camera_pos.y,
        cz = camera_pos.z,
        chunk_x = chunk.x,
        chunk_z = chunk.y,
        yaw = cam_state.yaw.to_degrees(),
        pitch = cam_state.pitch.to_degrees(),
        mode = cam_state.mode,
        vx = velocity.x,
        vy = velocity.y,
        vz = velocity.z,
        sprint = physics.sprinting,
        slot = physics.hotbar_slot + 1,
        blocks = state.show_block_aabb,
        entities = state.show_entity_aabb,
        chunk_lines = state.show_chunk_lines,
        wire = state.show_chunk_wireframe,
    );

    for mut text in &mut texts {
        *text = Text::new(content.clone());
    }
}

// ---------------------------------------------------------------------------
// Debug rendering (PostUpdate)
// ---------------------------------------------------------------------------

/// Draws debug wireframe overlays depending on the current DebugState.
fn debug_render_overlay_system(
    state: Res<DebugState>,
    cam: Single<&Transform, With<PlayerCamera>>,
    mut gizmos: Gizmos,
    chunks: Query<&Chunk>,
    entities: Query<
        &Transform,
        (
            Without<Chunk>,
            Without<PlayerController>,
            Without<PlayerCamera>,
        ),
    >,
) {
    // ── Block AABBs from chunk data (F3+B) ──
    if state.show_block_aabb {
        for chunk in &chunks {
            let chunk_world_x = chunk.position.x as f32 * CHUNK_SIZE as f32;
            let chunk_world_z = chunk.position.y as f32 * CHUNK_SIZE as f32;
            let y_base = chunk.y_world_offset;

            for ly in 0..CHUNK_HEIGHT {
                for lz in 0..CHUNK_SIZE {
                    for lx in 0..CHUNK_SIZE {
                        if chunk.get_block(lx, ly, lz) != shared::world::BlockKind::Air {
                            let world_pos = Vec3::new(
                                chunk_world_x + lx as f32 + 0.5,
                                y_base + ly as f32 + 0.5,
                                chunk_world_z + lz as f32 + 0.5,
                            );
                            gizmos.cube(
                                Transform::from_translation(world_pos).with_scale(Vec3::ONE),
                                Color::srgba(0.0, 1.0, 0.0, 0.3),
                            );
                        }
                    }
                }
            }
        }
    }

    // ── Entity AABBs (F3+E) ──
    if state.show_entity_aabb {
        for tf in &entities {
            gizmos.cube(
                Transform::from_translation(tf.translation).with_scale(Vec3::splat(0.5)),
                Color::srgba(1.0, 1.0, 0.0, 0.4),
            );
        }
    }

    // ── Chunk boundary lines (F3+A) ──
    if state.show_chunk_lines {
        let cam_pos = cam.translation;
        let start_x = (cam_pos.x / 16.0).floor() as i32 * 16 - 64;
        let end_x = (cam_pos.x / 16.0).ceil() as i32 * 16 + 64;
        let start_z = (cam_pos.z / 16.0).floor() as i32 * 16 - 64;
        let end_z = (cam_pos.z / 16.0).ceil() as i32 * 16 + 64;
        let line_y = cam_pos.y.max(0.0).floor();

        for x in (start_x..=end_x).step_by(16) {
            let from = Vec3::new(x as f32, line_y, start_z as f32);
            let to = Vec3::new(x as f32, line_y, end_z as f32);
            gizmos.line(from, to, Color::srgb(0.0, 0.5, 1.0));
            gizmos.line(
                Vec3::new(x as f32, line_y, start_z as f32),
                Vec3::new(x as f32, line_y + 4.0, start_z as f32),
                Color::srgb(0.0, 0.5, 1.0),
            );
        }
        for z in (start_z..=end_z).step_by(16) {
            let from = Vec3::new(start_x as f32, line_y, z as f32);
            let to = Vec3::new(end_x as f32, line_y, z as f32);
            gizmos.line(from, to, Color::srgb(0.0, 0.5, 1.0));
        }
    }

    // ── Chunk wireframe (F3+V) ──
    if state.show_chunk_wireframe {
        for chunk in &chunks {
            let chunk_world_x = chunk.position.x as f32 * CHUNK_SIZE as f32;
            let chunk_world_z = chunk.position.y as f32 * CHUNK_SIZE as f32;
            let y_base = chunk.y_world_offset;

            // Draw a wireframe box around the entire chunk column
            let min = Vec3::new(chunk_world_x, y_base, chunk_world_z);
            let max = Vec3::new(
                chunk_world_x + CHUNK_SIZE as f32,
                y_base + CHUNK_HEIGHT as f32,
                chunk_world_z + CHUNK_SIZE as f32,
            );
            draw_wireframe_box(&mut gizmos, min, max, Color::srgb(0.5, 0.0, 1.0));
        }
    }
}

/// Draw 12 edges of an axis-aligned box.
fn draw_wireframe_box(gizmos: &mut Gizmos, min: Vec3, max: Vec3, color: Color) {
    let corners = [
        Vec3::new(min.x, min.y, min.z),
        Vec3::new(max.x, min.y, min.z),
        Vec3::new(max.x, min.y, max.z),
        Vec3::new(min.x, min.y, max.z),
        Vec3::new(min.x, max.y, min.z),
        Vec3::new(max.x, max.y, min.z),
        Vec3::new(max.x, max.y, max.z),
        Vec3::new(min.x, max.y, max.z),
    ];
    // Bottom face
    gizmos.line(corners[0], corners[1], color);
    gizmos.line(corners[1], corners[2], color);
    gizmos.line(corners[2], corners[3], color);
    gizmos.line(corners[3], corners[0], color);
    // Top face
    gizmos.line(corners[4], corners[5], color);
    gizmos.line(corners[5], corners[6], color);
    gizmos.line(corners[6], corners[7], color);
    gizmos.line(corners[7], corners[4], color);
    // Vertical edges
    gizmos.line(corners[0], corners[4], color);
    gizmos.line(corners[1], corners[5], color);
    gizmos.line(corners[2], corners[6], color);
    gizmos.line(corners[3], corners[7], color);
}
