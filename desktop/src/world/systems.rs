//! Update systems: physics, player input, camera control, cursor.
//!
//! Each system handles one mechanic:
//! - `apply_gravity_system` – simple downward acceleration with ground clamp.
//! - `player_input_system`  – WASD movement, jump, block break / place.
//! - `mouse_look_system`    – first-person mouse look (yaw/pitch).
//! - `camera_follow_system` – attaches camera to player (first/third person).
//! - `camera_control_system`– F5 mode switch.
//! - `cursor_grab_system`   – auto-grab / hide cursor in-game.

use super::chunk::{self, CHUNK_HEIGHT, CHUNK_SIZE, Chunk};
use super::components::*;
use super::resources::*;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::light::VolumetricFog;
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::prelude::*;
use bevy::window::{PresentMode, PrimaryWindow, WindowMode};
use shared::world::BlockKind;

/// Try to add an item to the player's inventory.
/// Returns true if the item was added successfully.
fn add_item_to_inventory(physics: &mut PhysicsState, item: ItemKind) -> bool {
    // First try hotbar slots
    for slot in 0..9 {
        if physics.hotbar_items[slot].is_none() {
            physics.hotbar_items[slot] = Some(item);
            return true;
        }
    }
    // Then try main inventory slots
    for slot in 0..27 {
        if physics.inventory_items[slot].is_none() {
            physics.inventory_items[slot] = Some(item);
            return true;
        }
    }
    false // inventory full
}

/// Remove one item matching `kind` from inventory.
/// Checks hotbar first, then main inventory.
/// Returns true if an item was consumed.
fn consume_item_from_inventory(
    physics: &mut PhysicsState,
    kind: impl Fn(ItemKind) -> bool,
) -> bool {
    // Check hotbar first
    for slot in 0..9 {
        if let Some(item) = physics.hotbar_items[slot] {
            if kind(item) {
                physics.hotbar_items[slot] = None;
                return true;
            }
        }
    }
    // Then main inventory
    for slot in 0..27 {
        if let Some(item) = physics.inventory_items[slot] {
            if kind(item) {
                physics.inventory_items[slot] = None;
                return true;
            }
        }
    }
    false
}

fn block_kind_to_item(kind: BlockKind) -> Option<ItemKind> {
    match kind {
        BlockKind::Air | BlockKind::Water => None,
        other => Some(ItemKind::Block(other)),
    }
}

/// Gravity acceleration (blocks/s²) — Minecraft: 32.0 downward
const GRAVITY: f32 = -32.0;
/// Terminal velocity when falling (blocks/s)
const TERMINAL_VELOCITY: f32 = -78.4;
/// Jump upward velocity (blocks/s) — Minecraft: ~8.4 gives ~1.25 blocks height
const JUMP_VELOCITY: f32 = 10.0;
/// Ground friction coefficient (per-tick multiplier at 20 tps)
const GROUND_FRICTION: f32 = 0.6;
/// Player eye height above foot position (Minecraft: ~1.62 for 1.8m tall)
const EYE_HEIGHT: f32 = 1.62;
const WALK_SPEED: f32 = 4.317;
const SPRINT_SPEED: f32 = 5.612;
const GROUND_ACCEL: f32 = 38.0;
const AIR_ACCEL: f32 = 12.0;
const VOID_DAMAGE_Y: f32 = -200.0;
const VOID_RESPAWN_Y: f32 = -300.0;

pub fn sync_entity_transform_state(
    mut query: Query<(&Transform, &mut EntityTransformState), Changed<Transform>>,
) {
    for (transform, mut state) in &mut query {
        state.translation = transform.translation;
        state.scale = transform.scale;
        state.left_rotation = transform.rotation;
        state.right_rotation = Quat::IDENTITY;
        state.matrix = transform.to_matrix();
    }
}

pub fn apply_settings_system(
    settings: Res<GameSettings>,
    chunk_manager: Option<ResMut<ChunkManager>>,
    mut camera_q: Query<&mut Projection, With<PlayerCamera>>,
    mut windows: Query<&mut Window, With<PrimaryWindow>>,
) {
    if let Some(mut manager) = chunk_manager {
        if manager.render_radius != settings.render_distance {
            manager.render_radius = settings.render_distance.clamp(2, 16);
        }
    }

    if settings.is_changed() {
        for mut projection in &mut camera_q {
            if let Projection::Perspective(perspective) = &mut *projection {
                perspective.fov = settings.fov_degrees.to_radians();
            }
        }
        if let Ok(mut window) = windows.single_mut() {
            window.present_mode = if settings.vsync {
                PresentMode::AutoVsync
            } else {
                PresentMode::AutoNoVsync
            };
            window.mode = if settings.fullscreen {
                WindowMode::BorderlessFullscreen(MonitorSelection::Primary)
            } else {
                WindowMode::Windowed
            };
        }
    }
}

// ---------------------------------------------------------------------------
// Physics
// ---------------------------------------------------------------------------

/// Check if a point is inside a water block by scanning loaded chunks.
fn is_underwater(chunks: &[Chunk], point: Vec3) -> bool {
    chunks
        .iter()
        .any(|c| c.block_at_world(point) == BlockKind::Water)
}

/// Check if player's eyes (head) are underwater.
fn player_head_underwater(chunks: &[Chunk], player_pos: Vec3) -> bool {
    is_underwater(chunks, player_pos + Vec3::new(0.0, 1.5, 0.0))
}

/// Applies gravity to player velocity (does NOT move position — that's
/// done in `player_input_system` alongside collision detection).
pub fn apply_gravity_system(
    time: Res<Time>,
    mut physics: ResMut<PhysicsState>,
    mut query: Query<(Entity, &mut Transform, Option<&PlayerController>)>,
    chunk_query: Query<&Chunk>,
) {
    let dt = time.delta_secs().min(0.05);
    let chunk_data: Vec<Chunk> = chunk_query.iter().map(|c| c.clone()).collect();
    for (entity, _transform, controller) in &mut query {
        let vel = physics.velocities.entry(entity).or_insert(Vec3::ZERO);
        if controller.is_some() {
            if player_head_underwater(&chunk_data, _transform.translation) {
                // Underwater: buoyancy counteracts gravity, slow sink
                vel.y += (GRAVITY * 0.15) * dt; // Much less gravity
                if vel.y < TERMINAL_VELOCITY * 0.3 {
                    vel.y = TERMINAL_VELOCITY * 0.3;
                }
            } else {
                vel.y += GRAVITY * dt;
                // Clamp to terminal velocity
                if vel.y < TERMINAL_VELOCITY {
                    vel.y = TERMINAL_VELOCITY;
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Mouse look
// ---------------------------------------------------------------------------

/// Reads mouse delta and updates CameraState yaw/pitch.
pub fn mouse_look_system(
    mut cam_state: ResMut<CameraState>,
    mut mouse: MessageReader<MouseMotion>,
    menu_state: Res<bevy::prelude::State<crate::ui::inventory::MenuState>>,
) {
    // Freeze camera look when in UI mode
    if *menu_state.get() != crate::ui::inventory::MenuState::Playing {
        return;
    }

    let mut delta = Vec2::ZERO;
    for ev in mouse.read() {
        delta += ev.delta;
    }
    if delta == Vec2::ZERO {
        return;
    }

    cam_state.yaw -= delta.x * cam_state.sensitivity;
    cam_state.pitch -= delta.y * cam_state.sensitivity;
    cam_state.pitch = cam_state.pitch.clamp(-1.5, 1.5);
}

// ---------------------------------------------------------------------------
// Camera follow
// ---------------------------------------------------------------------------

/// Applies yaw/pitch to the player entity and positions the camera.
/// For third-person modes, the camera orbits the player with collision.
///
/// Implements CAMERA.md:
/// - First Person: camera at (player.pos + eye_height), rotation = yaw/pitch
/// - Third Person Back: camera = target - forward * distance, look_at(target)
/// - Third Person Front: camera = target + forward * distance, look_at(target)
///
/// NOTE: Camera is a separate entity (NOT child of player), so all transforms
/// are set in WORLD space.
pub fn camera_follow_system(
    cam_state: Res<CameraState>,
    settings: Res<GameSettings>,
    mut player_q: Query<&mut Transform, (With<PlayerController>, Without<PlayerCamera>)>,
    mut cam_q: Query<&mut Transform, (With<PlayerCamera>, Without<PlayerController>)>,
    chunk_query: Query<&Chunk>,
) {
    let Ok(player_t) = player_q.single_mut() else {
        return;
    };
    let Ok(mut cam_t) = cam_q.single_mut() else {
        return;
    };

    let pitch = cam_state.pitch;
    let yaw = cam_state.yaw;

    let forward = look_dir_from_yaw_pitch(yaw, pitch);

    let eye_pos = player_t.translation + Vec3::new(0.0, EYE_HEIGHT, 0.0);

    match cam_state.mode {
        CameraMode::FirstPerson => {
            // Camera at eye position, facing player's look direction
            // Since camera is NOT a child of player, we set WORLD transforms:
            let bob = if settings.view_bobbing {
                let phase = time_since_start(player_t.translation.x, player_t.translation.z);
                Vec3::new(phase.sin() * 0.015, phase.cos().abs() * 0.012, 0.0)
            } else {
                Vec3::ZERO
            };
            cam_t.translation = eye_pos + bob;
            // Yaw (Y) then pitch (X) — combined rotation for the camera
            cam_t.rotation = Quat::from_rotation_y(yaw) * Quat::from_rotation_x(pitch);
        }
        CameraMode::ThirdPersonBehind | CameraMode::ThirdPersonFront => {
            const CAM_DISTANCE: f32 = 4.0;

            // Target is at player's eye height
            let target = eye_pos;

            // Direction sign: behind = -forward, front = +forward
            let dir_sign = match cam_state.mode {
                CameraMode::ThirdPersonBehind => -1.0,
                CameraMode::ThirdPersonFront => 1.0,
                _ => unreachable!(),
            };

            // Raycast from target to camera for wall penetration prevention
            let dir_to_camera = forward * dir_sign;
            let dist_to_camera = CAM_DISTANCE;

            let mut actual_distance = dist_to_camera;
            let steps = (dist_to_camera * 4.0) as i32;
            for i in 0..steps {
                let t = i as f32 / steps as f32;
                let check_pos = target + dir_to_camera * (t * dist_to_camera);
                let chunk_blocked = chunk_query.iter().any(|c| {
                    let block = c.block_at_world(check_pos);
                    block.is_solid()
                });
                if chunk_blocked {
                    actual_distance = (t * dist_to_camera).max(0.5);
                    break;
                }
            }

            let final_pos = target + dir_to_camera * actual_distance;
            cam_t.translation = final_pos;

            cam_t.look_at(target, Vec3::Y);
        }
    }
}

fn time_since_start(x: f32, z: f32) -> f32 {
    (x.abs() + z.abs()) * 8.0
}

// ---------------------------------------------------------------------------

pub fn update_sky_follow_system(
    cam_q: Query<&Transform, (With<PlayerCamera>, Without<SkyDome>)>,
    mut sky_q: Query<&mut Transform, With<SkyDome>>,
    mut billboards: Query<
        &mut Transform,
        (
            With<CelestialBillboard>,
            Without<PlayerCamera>,
            Without<SkyDome>,
        ),
    >,
) {
    let Ok(cam_t) = cam_q.single() else { return };
    for mut sky_t in &mut sky_q {
        sky_t.translation = cam_t.translation;
    }
    for mut billboard_t in &mut billboards {
        billboard_t.look_at(Vec3::ZERO, Vec3::Y);
    }
}

pub fn update_world_time_system(
    time: Res<Time>,
    mut world_time: ResMut<WorldTime>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sun_q: Query<(&mut Transform, &mut DirectionalLight), With<SunLight>>,
    mut sun_visual_q: Query<&mut Transform, (With<SunVisual>, Without<SunLight>)>,
    mut moon_visual_q: Query<
        &mut Transform,
        (With<MoonVisual>, Without<SunLight>, Without<SunVisual>),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    sky_q: Query<&MeshMaterial3d<StandardMaterial>, With<SkyDome>>,
    star_q: Query<&MeshMaterial3d<StandardMaterial>, With<StarVisual>>,
) {
    const DAY_SECONDS: f32 = 1200.0;
    const SUN_DISTANCE: f32 = 760.0;

    let tick_delta = (time.delta_secs() / DAY_SECONDS * 24_000.0) as u64;
    world_time.tick = world_time.tick.saturating_add(tick_delta.max(1));
    world_time.day = world_time.tick / 24_000;
    world_time.time_of_day = (world_time.tick % 24_000) as f32 / 24_000.0;

    let angle = world_time.time_of_day * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    let sun_height = angle.sin();
    let sun_pos =
        Vec3::new(angle.cos() * 0.35, sun_height, angle.cos() * -0.92).normalize() * SUN_DISTANCE;
    let moon_pos = -sun_pos;

    // Smoother day/night transition curve
    let daylight = ((sun_height + 0.20) / 1.40).clamp(0.0, 1.0);

    // Brighter ambient at night so things are visible
    let night_factor = (1.0 - daylight).clamp(0.0, 1.0);

    ambient.brightness = 0.08 + daylight * 0.50 + night_factor * 0.04;
    ambient.color = Color::srgba(
        0.18 + daylight * 0.44,
        0.22 + daylight * 0.44,
        0.36 + daylight * 0.40,
        1.0,
    );

    // Moon glow — subtly illuminate at night
    let moon_brightness = night_factor.powf(1.35).min(0.8);

    for (mut transform, mut light) in &mut sun_q {
        if daylight > 0.08 {
            // Daytime: bright sun
            *transform = Transform::from_translation(sun_pos).looking_at(Vec3::ZERO, Vec3::Y);
            light.illuminance = 2_000.0 + daylight * 36_000.0;
            light.color = Color::srgb(1.0, 0.86 + daylight * 0.12, 0.68 + daylight * 0.22);
            light.shadows_enabled = true;
        } else {
            // Nighttime: dim moonlight
            *transform = Transform::from_translation(moon_pos).looking_at(Vec3::ZERO, Vec3::Y);
            light.illuminance = moon_brightness * 1_600.0;
            light.color = Color::srgb(0.42, 0.50, 0.78);
            light.shadows_enabled = true;
        }
    }

    for mut transform in &mut sun_visual_q {
        transform.translation = sun_pos;
    }
    for mut transform in &mut moon_visual_q {
        transform.translation = moon_pos;
        let phase = (world_time.day % 8) as f32 / 8.0;
        let scale = 0.78 + (phase * std::f32::consts::TAU).cos().abs() * 0.22;
        transform.scale = Vec3::splat(scale);
    }

    // Update sky dome material color for night gradient
    if let Ok(sky_mat_handle) = sky_q.single() {
        if let Some(sky_mat) = materials.get_mut(&sky_mat_handle.0) {
            // Night sky gets darker and bluer
            let r = 1.0 - night_factor * 0.92;
            let g = 1.0 - night_factor * 0.88;
            let b = 1.0 - night_factor * 0.62;
            sky_mat.base_color = Color::srgba(r, g, b, 1.0);
            // Slight emissive glow at night (stars)
            sky_mat.emissive = LinearRgba::rgb(
                night_factor * 0.015,
                night_factor * 0.018,
                night_factor * 0.035,
            );
        }
    }

    for star_mat_handle in &star_q {
        if let Some(star_mat) = materials.get_mut(&star_mat_handle.0) {
            let alpha = (night_factor * 1.25).clamp(0.0, 0.92);
            star_mat.base_color = Color::srgba(0.88, 0.92, 1.0, alpha);
            star_mat.emissive = LinearRgba::rgb(alpha * 2.4, alpha * 2.6, alpha * 3.2);
        }
    }
}

/// Animate water material color for a subtle caustic/flowing effect.
pub fn animate_water_system(
    time: Res<Time>,
    chunk_manager: Res<ChunkManager>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if let Some(mat) = materials.get_mut(&chunk_manager.water_material) {
        let t = time.elapsed_secs();
        // Subtle color pulse (caustic-like)
        let pulse = (t * 0.6).sin() * 0.04 + 0.04;
        mat.base_color = Color::srgba(
            0.24 + pulse * 0.5,
            0.44 + pulse * 0.4,
            0.92 + pulse * 0.2,
            0.72,
        );
    }
}

pub fn update_underwater_graphics_system(
    chunks: Query<&Chunk>,
    mut camera_q: Query<
        (
            &Transform,
            &mut Camera,
            &mut DistanceFog,
            Option<&mut VolumetricFog>,
        ),
        With<PlayerCamera>,
    >,
    time: Res<Time>,
) {
    let Ok((cam_t, mut camera, mut fog, volumetric)) = camera_q.single_mut() else {
        return;
    };
    let underwater = chunks
        .iter()
        .any(|chunk| chunk.block_at_world(cam_t.translation) == BlockKind::Water);

    if underwater {
        // Subtle animated wave effect using time for caustic-like variation
        let wave = (time.elapsed_secs() * 0.8).sin() * 0.04 + 0.04;
        let r = 0.12 + wave;
        let g = 0.32 + wave * 0.5;
        let b = 0.52 + wave * 0.3;
        camera.clear_color = ClearColorConfig::Custom(Color::srgb(r, g, b));
        fog.color = Color::srgba(r * 0.8, g * 0.9, b * 0.9, 0.62);
        fog.directional_light_color = Color::srgba(0.25, 0.50, 0.72, 0.28);
        fog.directional_light_exponent = 3.0;
        fog.falloff = FogFalloff::Linear {
            start: 3.0,
            end: 36.0,
        };
        if let Some(mut vf) = volumetric {
            vf.ambient_color = Color::srgb(0.10, 0.28, 0.46);
            vf.ambient_intensity = 0.022;
        }
    } else {
        camera.clear_color = ClearColorConfig::Custom(Color::srgb(0.46, 0.66, 0.94));
        fog.color = Color::srgba(0.58, 0.72, 0.90, 0.18);
        fog.directional_light_color = Color::srgba(1.0, 0.88, 0.66, 0.22);
        fog.directional_light_exponent = 8.0;
        fog.falloff = FogFalloff::Linear {
            start: 120.0,
            end: 360.0,
        };
        if let Some(mut vf) = volumetric {
            vf.ambient_color = Color::srgb(0.58, 0.70, 0.86);
            vf.ambient_intensity = 0.018;
        }
    }
}

pub fn update_block_outline_system(
    cam_state: Res<CameraState>,
    player_q: Query<&Transform, (With<PlayerController>, Without<BlockOutline>)>,
    chunk_query: Query<&Chunk>,
    mut outline_q: Query<(&mut Transform, &mut Visibility), With<BlockOutline>>,
    menu_state: Res<bevy::prelude::State<crate::ui::inventory::MenuState>>,
) {
    let Ok((mut outline_t, mut visibility)) = outline_q.single_mut() else {
        return;
    };
    if *menu_state.get() != crate::ui::inventory::MenuState::Playing {
        *visibility = Visibility::Hidden;
        return;
    }

    let Ok(player_t) = player_q.single() else {
        *visibility = Visibility::Hidden;
        return;
    };
    let chunk_data: Vec<Chunk> = chunk_query.iter().map(|c| c.clone()).collect();
    let origin = player_t.translation + Vec3::new(0.0, EYE_HEIGHT, 0.0);
    let dir = look_dir_from_yaw_pitch(cam_state.yaw, cam_state.pitch);

    if let Some((hit_pos, _face)) = raycast_against_chunks(&chunk_data, origin, dir, 6.0) {
        outline_t.translation = Vec3::new(
            hit_pos.x.floor() + 0.5,
            hit_pos.y.floor() + 0.5,
            hit_pos.z.floor() + 0.5,
        );
        *visibility = Visibility::Visible;
    } else {
        *visibility = Visibility::Hidden;
    }
}

// ---------------------------------------------------------------------------
// Player input
// ---------------------------------------------------------------------------

/// Handles WASD movement, jumping, block breaking (LMB) and placing (RMB).
pub fn player_input_system(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut mouse_wheel: MessageReader<MouseWheel>,
    mut physics: ResMut<PhysicsState>,
    _meshes: ResMut<Assets<Mesh>>,
    _materials: ResMut<Assets<StandardMaterial>>,
    _wm: Res<WorldModels>,
    mut commands: Commands,
    mut cam_state: ResMut<CameraState>,
    mut hud: ResMut<crate::ui::hud::HudState>,
    mut player_query: Query<
        (Entity, &mut Transform),
        (With<PlayerController>, Without<PlayerCamera>),
    >,
    chunk_query: Query<&Chunk>,
    menu_state: Res<bevy::prelude::State<crate::ui::inventory::MenuState>>,
) {
    // Freeze all player input when in UI/inventory mode
    if *menu_state.get() != crate::ui::inventory::MenuState::Playing {
        return;
    }

    let dt = time.delta_secs().min(0.05);
    cam_state.attack_timer = (cam_state.attack_timer - dt).max(0.0);
    cam_state.use_timer = (cam_state.use_timer - dt).max(0.0);

    let mut scroll_delta = 0.0;
    for ev in mouse_wheel.read() {
        scroll_delta += ev.y;
    }
    if scroll_delta.abs() > 0.0 {
        let steps = scroll_delta.signum() as isize;
        let slot = physics.hotbar_slot as isize - steps;
        physics.hotbar_slot = slot.rem_euclid(9) as usize;
    }

    if keyboard.just_pressed(KeyCode::ControlLeft) || keyboard.just_pressed(KeyCode::ControlRight) {
        physics.sprinting = !physics.sprinting;
    }

    let Ok((entity, player_t)) = player_query.single() else {
        return;
    };
    let player_orig = player_t.translation;

    if player_orig.y < VOID_DAMAGE_Y {
        hud.health = (hud.health - 6.0 * dt).max(0.0);
    }

    let spawn_y = 160.0;
    if player_orig.y < VOID_RESPAWN_Y || hud.health <= 0.0 {
        info!("Player died. Respawning...");
        let Ok((_, mut player_t)) = player_query.single_mut() else {
            return;
        };
        // Find a safe spawn position above the highest terrain near origin
        player_t.translation = Vec3::new(0.5, spawn_y, 0.5);
        physics.velocities.insert(entity, Vec3::ZERO);
        physics.sprinting = false;
        hud.health = hud.max_health;
        return;
    }

    // Movement direction from camera yaw (horizontal plane)
    let (forward, right) = {
        let yaw = cam_state.yaw;
        let f = Vec3::new(-yaw.sin(), 0.0, -yaw.cos()).normalize();
        let r = Vec3::new(yaw.cos(), 0.0, -yaw.sin()).normalize();
        (f, r)
    };

    // Collect chunk data for collision & raycasting
    let chunk_data: Vec<Chunk> = chunk_query.iter().map(|c| c.clone()).collect();

    let underwater = player_head_underwater(&chunk_data, player_orig);

    let mut wish = Vec3::ZERO;
    if keyboard.pressed(KeyCode::KeyW) {
        wish += forward;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        wish -= forward;
    }
    if keyboard.pressed(KeyCode::KeyA) {
        wish -= right;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        wish += right;
    }
    // Swimming vertical movement
    if underwater {
        if keyboard.pressed(KeyCode::Space) {
            wish.y += 1.0; // Swim up
        }
        if keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight) {
            wish.y -= 1.0; // Swim down
        }
    } else {
        wish.y = 0.0;
    }
    let moving = wish.length_squared() > 0.0;
    if !moving && !underwater {
        physics.sprinting = false;
    }
    let move_speed = if physics.sprinting && !underwater {
        SPRINT_SPEED
    } else if underwater {
        WALK_SPEED * 0.45 // Slower movement underwater
    } else {
        WALK_SPEED
    };
    if moving {
        wish = wish.normalize() * move_speed;
    }

    let vel_ref = physics.velocities.entry(entity).or_insert(Vec3::ZERO);
    let on_ground = collides_with_any_block(&chunk_data, player_orig + Vec3::new(0.0, -0.01, 0.0));
    let horizontal = Vec3::new(vel_ref.x, 0.0, vel_ref.z);
    let accel = if underwater {
        GROUND_ACCEL * 0.5 // Less acceleration underwater
    } else if on_ground {
        GROUND_ACCEL
    } else {
        AIR_ACCEL
    };
    let next_horizontal = horizontal.move_towards(wish, accel * dt);
    vel_ref.x = next_horizontal.x;
    vel_ref.z = next_horizontal.z;

    if on_ground && !moving {
        let friction_factor = (1.0 - GROUND_FRICTION).powf(dt * 20.0);
        vel_ref.x *= friction_factor;
        vel_ref.z *= friction_factor;
    }

    let move_vec = Vec3::new(vel_ref.x, 0.0, vel_ref.z) * dt;

    // Vertical movement from velocity (gravity already applied by
    // apply_gravity_system)
    let y_move = vel_ref.y * dt;

    let mut final_pos = player_orig;

    // ── Per-axis collision (hybrid order) ──
    // When jumping (moving upward), resolve Y first so the player can clear
    // waist-high obstacles before horizontal movement. When falling or
    // on ground, resolve X/Z first so the player can walk off ledges without
    // the vertical check snapping them back.

    if y_move > 0.0 {
        // ── Jumping: Y → X → Z ──
        let test_y = final_pos + Vec3::new(0.0, y_move, 0.0);
        if !collides_with_any_block(&chunk_data, test_y) {
            final_pos.y = test_y.y;
        } else {
            // Hit ceiling — check if head clears obstacle
            let (_p_min, p_max) = player_aabb_at(test_y);
            let obstacle_top = (p_max.y - 0.001).floor() as i32;
            let clearance_block = Vec3::new(final_pos.x, obstacle_top as f32 + 1.5, final_pos.z);
            if !collides_with_any_block(&chunk_data, clearance_block) {
                final_pos.y = test_y.y;
            } else {
                vel_ref.y = 0.0;
            }
        }

        // X at new Y
        let test_x = final_pos + Vec3::new(move_vec.x, 0.0, 0.0);
        if !collides_with_any_block(&chunk_data, test_x) {
            final_pos.x = test_x.x;
        } else if move_vec.x != 0.0 && on_ground {
            // Step assist when jumping from ground
            const STEP_HEIGHT: f32 = 0.6;
            let test_x_up = final_pos + Vec3::new(move_vec.x, STEP_HEIGHT, 0.0);
            if !collides_with_any_block(&chunk_data, test_x_up) {
                final_pos.x = test_x.x;
                final_pos.y = final_pos.y.max(test_x_up.y);
            }
        }

        // Z at new Y
        let test_z = final_pos + Vec3::new(0.0, 0.0, move_vec.z);
        if !collides_with_any_block(&chunk_data, test_z) {
            final_pos.z = test_z.z;
        } else if move_vec.z != 0.0 && on_ground {
            const STEP_HEIGHT: f32 = 0.6;
            let test_z_up = final_pos + Vec3::new(0.0, STEP_HEIGHT, move_vec.z);
            if !collides_with_any_block(&chunk_data, test_z_up) {
                final_pos.z = test_z.z;
                final_pos.y = final_pos.y.max(test_z_up.y);
            }
        }
    } else {
        // ── Falling/ground: X → Z → Y ──
        // X test
        let test_x = final_pos + Vec3::new(move_vec.x, 0.0, 0.0);
        if !collides_with_any_block(&chunk_data, test_x) {
            final_pos.x = test_x.x;
        } else if on_ground && move_vec.x != 0.0 {
            const STEP_HEIGHT: f32 = 0.6;
            let test_x_up = final_pos + Vec3::new(move_vec.x, STEP_HEIGHT, 0.0);
            if !collides_with_any_block(&chunk_data, test_x_up) {
                final_pos.x = test_x.x;
                final_pos.y = final_pos.y.max(test_x_up.y);
            }
        }

        // Z test
        let test_z = final_pos + Vec3::new(0.0, 0.0, move_vec.z);
        if !collides_with_any_block(&chunk_data, test_z) {
            final_pos.z = test_z.z;
        } else if on_ground && move_vec.z != 0.0 {
            const STEP_HEIGHT: f32 = 0.6;
            let test_z_up = final_pos + Vec3::new(0.0, STEP_HEIGHT, move_vec.z);
            if !collides_with_any_block(&chunk_data, test_z_up) {
                final_pos.z = test_z.z;
                final_pos.y = final_pos.y.max(test_z_up.y);
            }
        }

        // Y test at new X/Z
        let test_y = final_pos + Vec3::new(0.0, y_move, 0.0);
        if !collides_with_any_block(&chunk_data, test_y) {
            final_pos.y = test_y.y;
        } else if vel_ref.y < 0.0 {
            // Falling and hit ground — snap to surface
            if let Some(surface) = swept_landing_y(&chunk_data, final_pos, test_y.y) {
                final_pos.y = surface + 0.001;
                vel_ref.y = 0.0;
            } else {
                final_pos.y = test_y.y;
            }
        }
        // else: vel_ref.y near zero — no snap needed
    }

    // Ground check — test just below feet (expanded range)
    let on_ground = collides_with_any_block(&chunk_data, final_pos + Vec3::new(0.0, -0.01, 0.0));

    // Apply final position
    let Ok((_, mut player_t)) = player_query.single_mut() else {
        return;
    };
    player_t.translation = final_pos;
    let horizontal_velocity = Vec3::new(vel_ref.x, 0.0, vel_ref.z);
    let target_body_yaw = if horizontal_velocity.length_squared() > 0.01 {
        horizontal_velocity.x.atan2(horizontal_velocity.z) + std::f32::consts::PI
    } else {
        cam_state.yaw
    };
    let turn_speed = if moving { 12.0 } else { 6.0 };
    cam_state.body_yaw = lerp_angle(
        cam_state.body_yaw,
        target_body_yaw,
        1.0 - (-turn_speed * dt).exp(),
    );
    player_t.rotation = Quat::from_rotation_y(cam_state.body_yaw);

    // Jump (Minecraft height: ~1.25 blocks) / Swim up burst
    if keyboard.just_pressed(KeyCode::Space) {
        if underwater {
            // Swim up burst — gives upward impulse even when not holding space
            *vel_ref = Vec3::new(vel_ref.x, JUMP_VELOCITY * 0.7, vel_ref.z);
        } else if on_ground {
            *vel_ref = Vec3::new(vel_ref.x, JUMP_VELOCITY, vel_ref.z);
        }
    }

    // ── Block breaking (LMB) ──
    if mouse.just_pressed(MouseButton::Left) {
        cam_state.attack_timer = 0.5;
        let origin = player_orig + Vec3::new(0.0, EYE_HEIGHT, 0.0);
        let dir = look_dir_from_yaw_pitch(cam_state.yaw, cam_state.pitch);

        if let Some((hit_pos, _face)) = raycast_against_chunks(&chunk_data, origin, dir, 6.0) {
            let broken_kind = get_block_at_pos(&chunk_data, hit_pos);
            commands.queue(move |world: &mut World| {
                modify_block_in_chunks(world, hit_pos, BlockKind::Air);
                // Add the broken block to inventory
                if let Some(item) = block_kind_to_item(broken_kind) {
                    let mut physics = world.resource_mut::<PhysicsState>();
                    if !add_item_to_inventory(&mut physics, item) {
                        info!("Inventory full, dropped item");
                    }
                }
            });
        }
    }

    // ── Block placing (RMB) ──
    if mouse.just_pressed(MouseButton::Right) {
        cam_state.use_timer = 0.5;
        let origin = player_orig + Vec3::new(0.0, EYE_HEIGHT, 0.0);
        let dir = look_dir_from_yaw_pitch(cam_state.yaw, cam_state.pitch);

        // Find placeable block: first check hotbar slot, then main inventory
        let current_slot = physics.hotbar_slot;
        let hotbar_item = physics.hotbar_items[current_slot];
        let place_candidate = hotbar_item.and_then(|item| item.place_block());

        if let Some(place_kind) = place_candidate {
            if let Some((hit_pos, face_normal)) =
                raycast_against_chunks(&chunk_data, origin, dir, 6.0)
            {
                // Place block adjacent to the face that was hit
                let place_pos = hit_pos + face_normal;
                if !get_block_at_pos(&chunk_data, place_pos).is_solid()
                    && !block_overlaps_player(place_pos, final_pos)
                {
                    // Consume from hotbar slot first
                    let consumed = if hotbar_item.and_then(|i| i.place_block()) == Some(place_kind)
                    {
                        physics.hotbar_items[current_slot] = None;
                        true
                    } else {
                        // Check main inventory
                        consume_item_from_inventory(&mut physics, |i| {
                            i.place_block() == Some(place_kind)
                        })
                    };

                    if consumed {
                        commands.queue(move |world: &mut World| {
                            modify_block_in_chunks(world, place_pos, place_kind);
                        });
                    }
                }
            }
        }
    }
}

fn look_dir_from_yaw_pitch(yaw: f32, pitch: f32) -> Vec3 {
    // Spherical → Cartesian: yaw around Y axis, pitch up/down.
    // In Bevy's right-handed Y-up system, looking forward = -Z.
    Vec3::new(
        -yaw.sin() * pitch.cos(),
        pitch.sin(),
        -yaw.cos() * pitch.cos(),
    )
    .normalize()
}

fn angle_difference(a: f32, b: f32) -> f32 {
    let mut diff = a - b;
    while diff > std::f32::consts::PI {
        diff -= std::f32::consts::TAU;
    }
    while diff < -std::f32::consts::PI {
        diff += std::f32::consts::TAU;
    }
    diff
}

fn lerp_angle(current: f32, target: f32, factor: f32) -> f32 {
    current + angle_difference(target, current) * factor.clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Collision / raycast helpers (chunk-based)
// ---------------------------------------------------------------------------

/// Test whether the player AABB at `foot_pos` overlaps any solid block.
/// Uses the chunk's own coordinate system via `block_at_world`.
fn collides_with_any_block(chunks: &[Chunk], foot_pos: Vec3) -> bool {
    let (p_min, p_max) = player_aabb_at(foot_pos);
    // Subtract a small epsilon so blocks at exact AABB boundaries are
    // included in the iteration range (prevents pass-through at edges).
    let bx_min = (p_min.x - 0.001).floor() as i32;
    let bx_max = (p_max.x - 0.001).floor() as i32;
    let by_min = (p_min.y - 0.001).floor() as i32 - 1;
    let by_max = (p_max.y - 0.001).floor() as i32;
    let bz_min = (p_min.z - 0.001).floor() as i32;
    let bz_max = (p_max.z - 0.001).floor() as i32;

    for by in by_min..=by_max {
        for bz in bz_min..=bz_max {
            for bx in bx_min..=bx_max {
                let block_center = Vec3::new(bx as f32 + 0.5, by as f32 + 0.5, bz as f32 + 0.5);
                let bmin = block_center - Vec3::splat(0.5);
                let bmax = block_center + Vec3::splat(0.5);
                if aabb_overlap(p_min, p_max, bmin, bmax) {
                    if chunks
                        .iter()
                        .any(|c| c.block_at_world(block_center).is_solid())
                    {
                        return true;
                    }
                }
            }
        }
    }
    false
}

/// Find the highest solid block top crossed by the player's feet while falling.
fn swept_landing_y(chunks: &[Chunk], start_pos: Vec3, end_y: f32) -> Option<f32> {
    let (p_min, p_max) = player_aabb_at(start_pos);
    let bx_min = (p_min.x - 0.001).floor() as i32;
    let bx_max = (p_max.x - 0.001).floor() as i32;
    let bz_min = (p_min.z - 0.001).floor() as i32;
    let bz_max = (p_max.z - 0.001).floor() as i32;
    let y_min = end_y.floor() as i32;
    let y_max = (start_pos.y - 0.001).floor() as i32;

    let mut best_surface = None;
    for by in y_min..=y_max {
        let surface = by as f32 + 1.0;
        if !(end_y <= surface && start_pos.y >= surface) {
            continue;
        }

        for bz in bz_min..=bz_max {
            for bx in bx_min..=bx_max {
                let block_center = Vec3::new(bx as f32 + 0.5, by as f32 + 0.5, bz as f32 + 0.5);
                if chunks
                    .iter()
                    .any(|c| c.block_at_world(block_center).is_solid())
                {
                    best_surface =
                        Some(best_surface.map_or(surface, |best: f32| best.max(surface)));
                }
            }
        }
    }

    best_surface
}

/// Get block kind at a world position from chunk data.
fn get_block_at_pos(chunks: &[Chunk], world_pos: Vec3) -> BlockKind {
    for chunk in chunks {
        let kind = chunk.block_at_world(world_pos);
        if kind.is_solid() {
            return kind;
        }
    }
    BlockKind::Air
}

fn block_overlaps_player(block_pos: Vec3, player_foot_pos: Vec3) -> bool {
    let (p_min, p_max) = player_aabb_at(player_foot_pos);
    let center = Vec3::new(
        block_pos.x.floor() + 0.5,
        block_pos.y.floor() + 0.5,
        block_pos.z.floor() + 0.5,
    );
    let bmin = center - Vec3::splat(0.5);
    let bmax = center + Vec3::splat(0.5);
    aabb_overlap(p_min, p_max, bmin, bmax)
}

/// DDA (Digital Differential Analyzer) raycast against chunk blocks.
/// Returns the world position of the hit block and the face normal.
/// Based on Minecraft-accurate DDA algorithm.
fn raycast_against_chunks(
    chunks: &[Chunk],
    origin: Vec3,
    dir: Vec3,
    max_dist: f32,
) -> Option<(Vec3, Vec3)> {
    // Guard against zero-length direction
    if dir.length_squared() < 1e-10 {
        return None;
    }
    let dir = dir.normalize();

    // Starting block position
    let mut pos = IVec3::new(
        origin.x.floor() as i32,
        origin.y.floor() as i32,
        origin.z.floor() as i32,
    );

    let step = IVec3::new(
        if dir.x > 0.0 { 1 } else { -1 },
        if dir.y > 0.0 { 1 } else { -1 },
        if dir.z > 0.0 { 1 } else { -1 },
    );

    // t_delta: distance along ray to cross one full block on each axis
    let t_delta = Vec3::new(
        (1.0 / dir.x).abs(),
        (1.0 / dir.y).abs(),
        (1.0 / dir.z).abs(),
    );

    // t_max: distance along ray to reach the next voxel boundary
    let mut t_max = Vec3::new(
        if dir.x > 0.0 {
            (pos.x as f32 + 1.0) - origin.x
        } else {
            origin.x - pos.x as f32
        } * t_delta.x,
        if dir.y > 0.0 {
            (pos.y as f32 + 1.0) - origin.y
        } else {
            origin.y - pos.y as f32
        } * t_delta.y,
        if dir.z > 0.0 {
            (pos.z as f32 + 1.0) - origin.z
        } else {
            origin.z - pos.z as f32
        } * t_delta.z,
    );

    // Track which face was entered
    let mut face = Vec3::ZERO;

    // Maximum iterations: enough to cover max_dist
    let max_steps = (max_dist * 3.0) as i32 + 1;
    for _ in 0..max_steps {
        // Check current block
        let block_center = Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5);
        if chunks
            .iter()
            .any(|c| c.block_at_world(block_center).is_solid())
        {
            return Some((block_center, face));
        }

        // Step to next voxel along the shortest axis
        if t_max.x < t_max.y {
            if t_max.x < t_max.z {
                pos.x += step.x;
                face = Vec3::new(-step.x as f32, 0.0, 0.0);
                t_max.x += t_delta.x;
            } else {
                pos.z += step.z;
                face = Vec3::new(0.0, 0.0, -step.z as f32);
                t_max.z += t_delta.z;
            }
        } else {
            if t_max.y < t_max.z {
                pos.y += step.y;
                face = Vec3::new(0.0, -step.y as f32, 0.0);
                t_max.y += t_delta.y;
            } else {
                pos.z += step.z;
                face = Vec3::new(0.0, 0.0, -step.z as f32);
                t_max.z += t_delta.z;
            }
        }

        // Distance check from origin
        let current_center = Vec3::new(pos.x as f32 + 0.5, pos.y as f32 + 0.5, pos.z as f32 + 0.5);
        if (current_center - origin).length() > max_dist {
            break;
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Block modification helper (called from Command closures)
// ---------------------------------------------------------------------------

/// Modify a single block in chunk data and remesh the chunk.
fn modify_block_in_chunks(world: &mut World, world_pos: Vec3, new_kind: BlockKind) {
    // Find which chunk this position falls in
    let (cpos, _local) = chunk::world_to_chunk(world_pos);

    let chunk_e = {
        let chunks: Vec<(Entity, IVec2, f32)> = world
            .query::<(Entity, &Chunk)>()
            .iter(world)
            .map(|(e, c)| (e, c.position, c.y_world_offset))
            .collect();
        // Find matching chunk by position AND y offset
        chunks
            .iter()
            .find(|(_, p, y_off)| *p == cpos && world_pos.y >= *y_off)
            .map(|(e, _, _)| *e)
    };

    if let Some(entity) = chunk_e {
        // Convert world Y to local Y
        let chunk_comp = world.get::<Chunk>(entity).unwrap();
        let ly = chunk_comp.world_y_to_local(world_pos.y);
        let local_x = world_pos.x.floor() as i32 - cpos.x * CHUNK_SIZE as i32;
        let local_z = world_pos.z.floor() as i32 - cpos.y * CHUNK_SIZE as i32;
        if ly < 0 || ly >= CHUNK_HEIGHT as i32 {
            return;
        }
        if local_x < 0
            || local_x >= CHUNK_SIZE as i32
            || local_z < 0
            || local_z >= CHUNK_SIZE as i32
        {
            return;
        }
        let _ = chunk_comp;

        if let Some(mut chunk) = world.get_mut::<Chunk>(entity) {
            chunk.set_block(local_x as usize, ly as usize, local_z as usize, new_kind);

            // Build both opaque and water meshes
            let new_mesh = chunk::mesh_chunk(&chunk);
            let water_mesh = chunk::mesh_chunk_water(&chunk);
            drop(chunk);

            let mut meshes = world.resource_mut::<Assets<Mesh>>();
            let new_handle = meshes.add(new_mesh);
            let water_handle = meshes.add(water_mesh);
            drop(meshes);

            // Update opaque mesh
            if let Some(mut m3d) = world.get_mut::<Mesh3d>(entity) {
                m3d.0 = new_handle;
            }

            // Update water child mesh
            if let Some(children) = world.get::<Children>(entity) {
                for child in children.iter() {
                    if world.get::<WaterChunkEntity>(child).is_some() {
                        if let Some(mut wm3d) = world.get_mut::<Mesh3d>(child) {
                            wm3d.0 = water_handle;
                        }
                        break;
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Camera control (F5 toggle)
// ---------------------------------------------------------------------------

/// Switches camera mode with F5.
pub fn camera_control_system(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cam_state: ResMut<CameraState>,
) {
    if keyboard.just_pressed(KeyCode::F5) {
        cam_state.mode = match cam_state.mode {
            CameraMode::FirstPerson => CameraMode::ThirdPersonBehind,
            CameraMode::ThirdPersonBehind => CameraMode::ThirdPersonFront,
            CameraMode::ThirdPersonFront => CameraMode::FirstPerson,
        };
        info!("camera mode: {:?}", cam_state.mode);
    }
}

// ---------------------------------------------------------------------------
// Cursor grab
// ---------------------------------------------------------------------------

/// Manages cursor grab state.
/// - On game start: auto-locks cursor to center
/// - When opening inventory (Escape/E): releases cursor
/// - When closing inventory: re-locks cursor on next frame
/// - Click to lock when unlocked in Playing state
pub fn cursor_grab_system(
    mut cursor_opts: Query<&mut bevy::window::CursorOptions, With<bevy::window::PrimaryWindow>>,
    btn: Res<ButtonInput<MouseButton>>,
    _keyboard: Res<ButtonInput<KeyCode>>,
    menu_state: Res<bevy::prelude::State<crate::ui::inventory::MenuState>>,
    mut started: Local<bool>,
    mut prev_state: Local<crate::ui::inventory::MenuState>,
) {
    let Ok(mut cursor) = cursor_opts.single_mut() else {
        return;
    };

    let current_state = *menu_state.get();
    let is_playing = current_state == crate::ui::inventory::MenuState::Playing;

    if !is_playing {
        *started = true;
        *prev_state = current_state;
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
        return;
    }

    if *started && *prev_state != crate::ui::inventory::MenuState::Playing {
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
        *prev_state = current_state;
        return;
    }

    // Auto-lock on very first frame (game start)
    if !*started {
        *started = true;
        *prev_state = *menu_state.get();
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
        return;
    }

    // Detect state transition: Inventory → Playing (closed GUI)
    if *prev_state == crate::ui::inventory::MenuState::Inventory
        && *menu_state.get() == crate::ui::inventory::MenuState::Playing
    {
        cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
        cursor.visible = false;
        *prev_state = *menu_state.get();
        return;
    }

    // Detect state transition: Playing → Inventory (opened GUI)
    if *prev_state == crate::ui::inventory::MenuState::Playing
        && *menu_state.get() == crate::ui::inventory::MenuState::Inventory
    {
        cursor.grab_mode = bevy::window::CursorGrabMode::None;
        cursor.visible = true;
        *prev_state = *menu_state.get();
        return;
    }

    // Refresh prev_state if external code changed state
    *prev_state = *menu_state.get();

    // In Playing mode: click to re-lock if unlocked
    if *menu_state.get() == crate::ui::inventory::MenuState::Playing
        && cursor.grab_mode != bevy::window::CursorGrabMode::Locked
    {
        let any_click = btn.just_pressed(MouseButton::Left) || btn.just_pressed(MouseButton::Right);
        if any_click {
            cursor.grab_mode = bevy::window::CursorGrabMode::Locked;
            cursor.visible = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Collision helpers
// ---------------------------------------------------------------------------

/// AABB for the player at a given foot position.
fn player_aabb_at(pos: Vec3) -> (Vec3, Vec3) {
    let half = 0.3;
    (
        Vec3::new(pos.x - half, pos.y, pos.z - half),
        Vec3::new(
            pos.x + half,
            pos.y + crate::world::PLAYER_HEIGHT,
            pos.z + half,
        ),
    )
}

/// Axis-aligned bounding box overlap test.
/// Uses `<=` for X and Z min checks to prevent pass-through at exact
/// block boundaries. Uses strict `<` for Y max check so that standing ON
/// a block (feet exactly at block top) does NOT register as overlap.
fn aabb_overlap(min1: Vec3, max1: Vec3, min2: Vec3, max2: Vec3) -> bool {
    min1.x <= max2.x
        && max1.x > min2.x
        && min1.y < max2.y
        && max1.y > min2.y
        && min1.z <= max2.z
        && max1.z > min2.z
}
