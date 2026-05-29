use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{NoFrustumCulling, RenderLayers};
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use serde_json::Value;
use shared::bedrock::{BedrockGeometryFile, BedrockGeometryUv};
use shared::pack::PackDirectory;

use crate::GamePack;
use crate::world::components::PlayerController;
use crate::world::resources::CameraMode;
use crate::world::resources::CameraState;
use crate::world::resources::ItemKind;
use crate::world::resources::PhysicsState;
use crate::world::resources::ToolKind;

pub struct PlayerPlugin;

const PLAYER_ATTACK_DURATION: f32 = 0.5;
const PLAYER_USE_DURATION: f32 = 0.5;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Startup,
            (spawn_player, reparent_player_model.after(spawn_player)),
        )
        .add_systems(
            Update,
            (
                animate_player,
                player_model_visibility,
                manage_first_person_hands,
                manage_third_person_held_item,
                first_person_shadow_proxy_visibility,
            ),
        )
        .add_systems(PostStartup, spawn_first_person_hand)
        .add_systems(
            PostStartup,
            spawn_third_person_held_item.after(spawn_first_person_hand),
        )
        .add_systems(PostStartup, spawn_first_person_shadow_proxy);
    }
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks the root entity of the Bedrock visual player model.
#[derive(Component)]
pub struct PlayerModelRoot;

/// Marks arm bones that stay visible in first-person view.
#[derive(Component)]
pub struct FirstPersonArm;

/// Marks the separately-spawned first-person right hand (child of camera).
#[derive(Component)]
pub struct FirstPersonHand;

#[derive(Component)]
pub struct FirstPersonHeldItem;

#[derive(Component)]
pub struct ThirdPersonHeldItem;

#[derive(Component)]
pub struct FirstPersonShadowProxy;

#[derive(Resource, Clone, Default)]
struct FirstPersonHeldAssets {
    grass_mesh: Handle<Mesh>,
    dirt_mesh: Handle<Mesh>,
    stone_mesh: Handle<Mesh>,
    sand_mesh: Handle<Mesh>,
    oak_log_mesh: Handle<Mesh>,
    oak_leaves_mesh: Handle<Mesh>,
    cobblestone_mesh: Handle<Mesh>,
    block_material: Handle<StandardMaterial>,
    pickaxe_mesh: Handle<Mesh>,
    pickaxe_material: Handle<StandardMaterial>,
    axe_mesh: Handle<Mesh>,
    axe_material: Handle<StandardMaterial>,
    sword_mesh: Handle<Mesh>,
    sword_material: Handle<StandardMaterial>,
}

#[derive(Resource, Default)]
struct PlayerAnimationData {
    root: Option<Value>,
}

#[derive(Component)]
struct PlayerBoneName(String);

#[derive(Component, Clone, Copy)]
struct RestPose {
    translation: Vec3,
    rotation: Quat,
}

#[derive(Component, Clone, Copy, PartialEq, Eq)]
enum BoneRole {
    Head,
    ArmLeft,
    ArmRight,
    LegLeft,
    LegRight,
}

#[derive(Clone, Copy)]
struct BoneRuntime {
    entity: Entity,
    pivot: Vec3,
}

fn bedrock_units_to_world(v: [f32; 3]) -> Vec3 {
    Vec3::new(v[0] / 16.0, v[1] / 16.0, v[2] / 16.0)
}

fn role_from_bone_name(name: &str) -> Option<BoneRole> {
    match name {
        "h_ph_head" => Some(BoneRole::Head),
        "pra_right_arm" => Some(BoneRole::ArmRight),
        "pla_left_arm" => Some(BoneRole::ArmLeft),
        "prl_right_leg" => Some(BoneRole::LegRight),
        "pll_left_leg" => Some(BoneRole::LegLeft),
        _ => None,
    }
}

/// Build a mesh for a single Bedrock cube with proper UV coordinates
/// using the standard box-unwrap layout.
///
/// Texture layout (size = [w, h, d]):
/// ```text
///           u+d  u+d+w   u+d+w+d  u+d+w+d+w
///      u     |     |        |        |
/// v    +-----+-----+--------+--------+
///      |West |North| East   | South  |
/// v+h  +-----+-----+--------+--------+
///      | Top |           | Bottom    |
/// v+h+d+-----+           +-----------+
/// ```
/// Normalized UV = pixel / texture_dimension.
fn build_cube_mesh(
    size: [f32; 3],
    uv: Option<&BedrockGeometryUv>,
    uv_size: Option<[f32; 2]>,
    texture_width: f32,
    texture_height: f32,
) -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    let [w, h, d] = size;
    let tw = texture_width.max(1.0);
    let th = texture_height.max(1.0);

    // UV anchor (default to 0,0 if not specified)
    let (u, v) = match uv {
        Some(BedrockGeometryUv::Box(origin)) => (origin[0], origin[1]),
        _ => (0.0, 0.0),
    };
    // UV size defaults to cube's width × height
    let [uw, uh] = uv_size.unwrap_or([w, h]);
    let face_uv = |name: &str, fallback: (f32, f32, f32, f32)| -> (f32, f32, f32, f32) {
        if let Some(BedrockGeometryUv::PerFace(faces)) = uv {
            if let Some(face) = faces.get(name) {
                let u0 = face.uv[0];
                let v0 = face.uv[1];
                let u1 = u0 + face.uv_size[0];
                let v1 = v0 + face.uv_size[1];
                return (u0, v0, u1, v1);
            }
        }
        fallback
    };

    // Normalize pixel coordinates to 0-1
    let nu = |x: f32| x / tw;
    let nv = |y: f32| y / th;

    // Face vertex positions (relative to cube center, 0.5 units per block)
    let half = 0.5;
    let hw = w / 16.0 * half; // half-width in world units
    let hh = h / 16.0 * half; // half-height in world units
    let hd = d / 16.0 * half; // half-depth in world units

    // The 8 corners of the box (local space, centered)
    let corners: [[f32; 3]; 8] = [
        [-hw, -hh, -hd], // 0: bottom-left-back
        [hw, -hh, -hd],  // 1: bottom-right-back
        [hw, hh, -hd],   // 2: top-right-back
        [-hw, hh, -hd],  // 3: top-left-back
        [-hw, -hh, hd],  // 4: bottom-left-front
        [hw, -hh, hd],   // 5: bottom-right-front
        [hw, hh, hd],    // 6: top-right-front
        [-hw, hh, hd],   // 7: top-left-front
    ];

    // Box-unwrap UV layout (pixel coords, face by face)
    // Front (south, +Z): uv at (u+d+w, v) size (w, h)
    // Right (east, +X): uv at (u+d+w+d+w, v) size (d, h)
    // Back (north, -Z): uv at (u+d, v) size (w, h)
    // Left (west, -X): uv at (u, v) size (d, h)
    // Top (+Y): uv at (u+d, v+h) size (w, d)
    // Bottom (-Y): uv at (u+d+w, v+h) size (w, d)
    //
    // Note: Using Bedrock box-UV convention where faces wrap around.

    let (u_front, u_right, u_back, u_left, u_top, u_bottom) = (
        u + d + uw,     // front starts after right
        u + d + uw + d, // right after front
        u + d,          // back after left
        u,              // left is first
        u + d,          // top aligned with back
        u + d + uw,     // bottom aligned with right
    );

    let (v_side, v_top, v_bottom) = (
        v,      // side faces at v
        v + uh, // top below sides
        v + uh, // bottom also below sides
    );

    // Each face: 4 vertices (positions + UVs) + 6 triangle indices
    // Face order: +Z (front), -Z (back), +X (right), -X (left), +Y (top), -Y (bottom)
    let face_data: [(usize, usize, usize, usize, &str, f32, f32, f32, f32); 6] = [
        // front (+Z): corners 4,5,6,7, UV at front region
        (
            4,
            5,
            6,
            7,
            "south",
            u_front,
            v_side,
            u_front + uw,
            v_side + uh,
        ),
        // back (-Z): corners 1,0,3,2, UV at back region
        (
            1,
            0,
            3,
            2,
            "north",
            u_back,
            v_side,
            u_back + uw,
            v_side + uh,
        ),
        // right (+X): corners 5,1,2,6, UV at right region
        (
            5,
            1,
            2,
            6,
            "east",
            u_right,
            v_side,
            u_right + d,
            v_side + uh,
        ),
        // left (-X): corners 0,4,7,3, UV at left region
        (0, 4, 7, 3, "west", u_left, v_side, u_left + d, v_side + uh),
        // top (+Y): corners ordered for outward +Y normal
        (7, 6, 2, 3, "up", u_top, v_top, u_top + uw, v_top + d),
        // bottom (-Y): corners ordered for outward -Y normal
        (
            0,
            1,
            5,
            4,
            "down",
            u_bottom,
            v_bottom,
            u_bottom + uw,
            v_bottom + d,
        ),
    ];

    let mut vertices: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(24);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(24);
    let mut indices: Vec<u32> = Vec::with_capacity(36);

    for (i0, i1, i2, i3, face_name, u_min, v_min, u_max, v_max) in &face_data {
        let base = vertices.len() as u32;
        let (u_min, v_min, u_max, v_max) = face_uv(face_name, (*u_min, *v_min, *u_max, *v_max));

        // 4 corners of this face
        let p0 = corners[*i0];
        let p1 = corners[*i1];
        let p2 = corners[*i2];
        let p3 = corners[*i3];

        // Normal: cross product of two edges
        let edge1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
        let edge2 = [p3[0] - p0[0], p3[1] - p0[1], p3[2] - p0[2]];
        let normal = [
            edge1[1] * edge2[2] - edge1[2] * edge2[1],
            edge1[2] * edge2[0] - edge1[0] * edge2[2],
            edge1[0] * edge2[1] - edge1[1] * edge2[0],
        ];
        // Normalize
        let len = (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
        let n = if len > 0.0 {
            [normal[0] / len, normal[1] / len, normal[2] / len]
        } else {
            [0.0, 0.0, 0.0]
        };

        vertices.extend_from_slice(&[p0, p1, p2, p3]);
        normals.extend_from_slice(&[n, n, n, n]);
        uvs.extend_from_slice(&[
            [nu(u_min), nv(v_max)], // bottom-left
            [nu(u_max), nv(v_max)], // bottom-right
            [nu(u_max), nv(v_min)], // top-right
            [nu(u_min), nv(v_min)], // top-left
        ]);

        // Two triangles per face (CCW winding)
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

fn spawn_player(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    asset_server: Res<AssetServer>,
    pack: Res<GamePack>,
) {
    // Fallback simple style if Bedrock model cannot be loaded.
    let _fallback_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    let player_material = if let Some(texture_path) = pack
        .0
        .resolve_in(PackDirectory::Textures, "entities/player.png")
    {
        materials.add(StandardMaterial {
            base_color_texture: Some(
                asset_server.load_override(texture_path.to_string_lossy().to_string()),
            ),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            alpha_mode: AlphaMode::Mask(0.1),
            ..default()
        })
    } else {
        materials.add(Color::srgb(0.9, 0.8, 0.6))
    };

    let animation_root = pack
        .0
        .load_string("animations/player/player.animation.json")
        .ok()
        .and_then(|json| serde_json::from_str::<Value>(&json).ok());
    commands.insert_resource(PlayerAnimationData {
        root: animation_root,
    });

    let root_entity = commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            GlobalTransform::default(),
            Visibility::Visible,
            PlayerModelRoot,
        ))
        .id();

    let model_path = "models/entities/player.geo.json";
    let model_data = pack.0.load_string(model_path);

    if let Ok(model_json) = model_data {
        if let Ok(parsed) = serde_json::from_str::<BedrockGeometryFile>(&model_json) {
            if let Some(geometry) = parsed.geometries.first() {
                let tex_w = geometry
                    .description
                    .as_ref()
                    .and_then(|d| d.texture_width)
                    .unwrap_or(64.0);
                let tex_h = geometry
                    .description
                    .as_ref()
                    .and_then(|d| d.texture_height)
                    .unwrap_or(64.0);

                let mut pending = geometry.bones.clone();
                let mut built: std::collections::HashMap<String, BoneRuntime> =
                    std::collections::HashMap::new();

                while !pending.is_empty() {
                    let mut progress = false;
                    let mut i = 0;

                    while i < pending.len() {
                        let bone = &pending[i];
                        let parent_ready = bone
                            .parent
                            .as_ref()
                            .map(|p| built.contains_key(p))
                            .unwrap_or(true);

                        if !parent_ready {
                            i += 1;
                            continue;
                        }

                        let bone = pending.remove(i);
                        let parent_runtime =
                            bone.parent.as_ref().and_then(|p| built.get(p).copied());

                        let pivot = bedrock_units_to_world(bone.pivot.unwrap_or([0.0, 0.0, 0.0]));
                        let local_translation = if let Some(parent) = parent_runtime {
                            pivot - parent.pivot
                        } else {
                            pivot
                        };

                        let rest_rotation = bone
                            .rotation
                            .map(|rotation| {
                                rotation_degrees_to_quat(Vec3::new(
                                    rotation[0],
                                    rotation[1],
                                    rotation[2],
                                ))
                            })
                            .unwrap_or(Quat::IDENTITY);

                        let mut entity_commands = commands.spawn((
                            Transform::from_translation(local_translation)
                                .with_rotation(rest_rotation),
                            GlobalTransform::default(),
                            Visibility::Visible,
                            PlayerBoneName(bone.name.clone()),
                            RestPose {
                                translation: local_translation,
                                rotation: rest_rotation,
                            },
                        ));

                        if let Some(role) = role_from_bone_name(&bone.name) {
                            entity_commands.insert(role);
                            // Arm bones stay visible in first person
                            if role == BoneRole::ArmLeft || role == BoneRole::ArmRight {
                                entity_commands.insert(FirstPersonArm);
                            }
                        }

                        let bone_entity = entity_commands.id();

                        if let Some(parent) = parent_runtime {
                            commands.entity(parent.entity).add_child(bone_entity);
                        } else {
                            commands.entity(root_entity).add_child(bone_entity);
                        }

                        // Spawn all cubes of this bone as children.
                        for cube in &bone.cubes {
                            let inflate = cube.inflate.unwrap_or(0.0);
                            let cube_size_px = [
                                cube.size[0] + inflate * 2.0,
                                cube.size[1] + inflate * 2.0,
                                cube.size[2] + inflate * 2.0,
                            ];

                            let _cube_world_size = Vec3::new(
                                cube_size_px[0] / 16.0,
                                cube_size_px[1] / 16.0,
                                cube_size_px[2] / 16.0,
                            );

                            let center = Vec3::new(
                                cube.origin[0] + cube.size[0] * 0.5,
                                cube.origin[1] + cube.size[1] * 0.5,
                                cube.origin[2] + cube.size[2] * 0.5,
                            ) / 16.0;

                            let local = center - pivot;

                            // Build per-cube mesh with correct UVs
                            // UV size defaults to cube width/height if not specified
                            let uv_size = cube.uv_size.unwrap_or([cube.size[0], cube.size[1]]);
                            let cube_mesh = build_cube_mesh(
                                cube_size_px,
                                cube.uv.as_ref(),
                                Some(uv_size),
                                tex_w,
                                tex_h,
                            );
                            let mesh_handle = meshes.add(cube_mesh);

                            commands.entity(bone_entity).with_children(|parent| {
                                parent.spawn((
                                    Mesh3d(mesh_handle),
                                    MeshMaterial3d(player_material.clone()),
                                    Transform::from_translation(local),
                                    Visibility::Visible,
                                ));
                            });
                        }

                        built.insert(
                            bone.name,
                            BoneRuntime {
                                entity: bone_entity,
                                pivot,
                            },
                        );

                        progress = true;
                    }

                    if !progress {
                        warn!(
                            "bedrock model has unresolved bone parent chain, stopping at partial hierarchy"
                        );
                        break;
                    }
                }

                info!(
                    "spawned player using bedrock geometry model: {}",
                    model_path
                );
            }
        }
    }

    // Scene lighting is owned by world::setup so shadow intensity follows the
    // real light sources instead of a hidden local fill light near the player.
}

fn animate_player(
    time: Res<Time>,
    keyboard: Res<ButtonInput<KeyCode>>,
    cam_state: Res<CameraState>,
    physics: Res<PhysicsState>,
    animations: Res<PlayerAnimationData>,
    mut locomotion_blend: Local<f32>,
    mut previous_locomotion: Local<f32>,
    mut locomotion_phase: Local<f32>,
    mut q_bones: Query<(&BoneRole, &PlayerBoneName, &RestPose, &mut Transform)>,
) {
    let dt = time.delta_secs().min(0.05);
    let t = time.elapsed_secs();

    // Detect movement
    let walking = keyboard.pressed(KeyCode::KeyW)
        || keyboard.pressed(KeyCode::KeyS)
        || keyboard.pressed(KeyCode::KeyA)
        || keyboard.pressed(KeyCode::KeyD);
    let action_active = cam_state.attack_timer > 0.0 || cam_state.use_timer > 0.0;
    let target_locomotion = if walking {
        if physics.sprinting { 1.35 } else { 1.0 }
    } else {
        0.0
    };
    *locomotion_blend += (target_locomotion - *locomotion_blend) * (1.0 - (-8.0 * dt).exp());
    let locomotion = *locomotion_blend;
    let moving_anim = locomotion > 0.03;
    let run_blend = ((locomotion - 1.0) / 0.35).clamp(0.0, 1.0);
    let cycle_hz = if moving_anim {
        1.0 + run_blend * 0.5
    } else {
        0.0
    };
    *locomotion_phase = (*locomotion_phase + dt * cycle_hz * std::f32::consts::TAU)
        .rem_euclid(std::f32::consts::TAU);
    if moving_anim != (*previous_locomotion > 0.03) {
        *locomotion_phase = locomotion_phase.rem_euclid(std::f32::consts::TAU);
    }
    *previous_locomotion = locomotion;

    let anim_time = (1.0 - *locomotion_phase / std::f32::consts::TAU).fract();
    let walk_anim = animations.sample("animation.model.walk", anim_time);
    let run_anim = animations.sample("animation.model.run", anim_time);
    let attack_anim = animations.sample(
        "animation.model.attack_fist",
        PLAYER_ATTACK_DURATION - cam_state.attack_timer.max(0.0),
    );
    let use_anim = animations.sample(
        "animation.model.attack_item",
        PLAYER_USE_DURATION - cam_state.use_timer.max(0.0),
    );
    let attack_weight = if cam_state.attack_timer > 0.0 {
        click_curve(cam_state.attack_timer, PLAYER_ATTACK_DURATION)
    } else {
        0.0
    };
    let use_weight = if cam_state.use_timer > 0.0 {
        click_curve(cam_state.use_timer, PLAYER_USE_DURATION)
    } else {
        0.0
    };
    let smooth = 1.0 - (-18.0 * dt).exp();

    for (role, bone_name, rest, mut transform) in &mut q_bones {
        let (fallback_rot, swing_axis, swing_speed, swing_amp) = match role {
            BoneRole::Head => {
                let head_yaw = angle_difference(cam_state.yaw, cam_state.body_yaw)
                    .clamp(-75.0_f32.to_radians(), 75.0_f32.to_radians());
                let head_pitch = cam_state
                    .pitch
                    .clamp(-60.0_f32.to_radians(), 60.0_f32.to_radians());
                (
                    Quat::from_rotation_y(head_yaw) * Quat::from_rotation_x(head_pitch),
                    Vec3::Y,
                    0.0,
                    0.0,
                )
            }
            BoneRole::ArmLeft => {
                if action_active {
                    (Quat::from_rotation_x(0.1), Vec3::X, 0.0, 0.0)
                } else if moving_anim {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                } else {
                    // Idle: arms hang slightly forward
                    (Quat::from_rotation_x(0.1), Vec3::X, 1.5, 0.03)
                }
            }
            BoneRole::ArmRight => {
                let attack = click_curve(cam_state.attack_timer, PLAYER_ATTACK_DURATION);
                let use_action = click_curve(cam_state.use_timer, PLAYER_USE_DURATION);
                if attack > 0.0 {
                    (
                        Quat::from_rotation_x(-1.35 * attack)
                            * Quat::from_rotation_z(-0.20 * attack),
                        Vec3::X,
                        0.0,
                        0.0,
                    )
                } else if use_action > 0.0 {
                    (
                        Quat::from_rotation_x(0.70 * use_action)
                            * Quat::from_rotation_y(-0.20 * use_action),
                        Vec3::X,
                        0.0,
                        0.0,
                    )
                } else if moving_anim {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                } else {
                    (Quat::from_rotation_x(0.1), Vec3::X, 1.5, -0.03)
                }
            }
            BoneRole::LegLeft => {
                if moving_anim {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                } else {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                }
            }
            BoneRole::LegRight => {
                if moving_anim {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                } else {
                    (Quat::IDENTITY, Vec3::X, 0.0, 0.0)
                }
            }
        };

        let mut target_rot = rest.rotation * fallback_rot;
        let mut target_translation = rest.translation;

        if moving_anim {
            let json_pose = blend_pose(
                walk_anim.bone(&bone_name.0),
                run_anim.bone(&bone_name.0),
                run_blend,
            );
            if let Some(rot) = json_pose.rotation {
                target_rot = if *role == BoneRole::Head {
                    rest.rotation * rot * fallback_rot
                } else {
                    rest.rotation * rot
                };
            }
            if let Some(pos) = json_pose.position {
                target_translation = rest.translation + pos * locomotion.min(1.0);
            }
        }

        if attack_weight > 0.0 {
            if let Some(pose) = attack_anim.bone(&bone_name.0) {
                if let Some(rot) = pose.rotation {
                    target_rot = target_rot.slerp(rest.rotation * rot, attack_weight);
                }
                if let Some(pos) = pose.position {
                    target_translation =
                        target_translation.lerp(rest.translation + pos, attack_weight);
                }
            }
        } else if use_weight > 0.0 {
            if let Some(pose) = use_anim.bone(&bone_name.0) {
                if let Some(rot) = pose.rotation {
                    target_rot = target_rot.slerp(rest.rotation * rot, use_weight);
                }
                if let Some(pos) = pose.position {
                    target_translation =
                        target_translation.lerp(rest.translation + pos, use_weight);
                }
            }
        }

        if swing_speed > 0.0 && swing_amp != 0.0 {
            let swing_angle = (t * swing_speed).sin() * swing_amp;
            target_rot *= Quat::from_axis_angle(swing_axis, swing_angle);
        }

        transform.rotation = transform.rotation.slerp(target_rot, smooth);
        transform.translation = transform.translation.lerp(target_translation, smooth);
    }
}

/// Reparents the visual player model under the PlayerController entity,
/// so the model follows the gameplay character.
fn reparent_player_model(
    mut commands: Commands,
    model: Query<Entity, (With<PlayerModelRoot>, Without<PlayerController>)>,
    controller: Query<Entity, (With<PlayerController>, Without<PlayerModelRoot>)>,
) {
    let Ok(model_entity) = model.single() else {
        return;
    };
    let Ok(controller_entity) = controller.single() else {
        return;
    };
    commands.entity(controller_entity).add_child(model_entity);
    info!("reparented visual player model under PlayerController");
}

/// Hides the full player model in first person (shadows still cast).
/// In first person, the model becomes fully transparent but remains in the
/// scene so it contributes to shadow maps. In third person it's fully visible.
fn player_model_visibility(
    mut commands: Commands,
    cam_state: Res<CameraState>,
    model_root: Query<Entity, With<PlayerModelRoot>>,
    children: Query<&Children>,
) {
    let is_first_person = cam_state.mode == CameraMode::FirstPerson;
    let Ok(root) = model_root.single() else {
        return;
    };

    // In first person, hide the model hierarchy so only the hand is visible.
    // The model won't cast shadows when hidden — proper shadow-only rendering
    // requires a separate shadow pass (future enhancement).
    let layer = if is_first_person { 1 } else { 0 };
    set_render_layer_recursive(root, layer, &children, &mut commands);
}

/// Recursively set visibility on all descendants.
fn set_render_layer_recursive(
    entity: Entity,
    layer: usize,
    children: &Query<&Children>,
    commands: &mut Commands,
) {
    commands.entity(entity).insert(RenderLayers::layer(layer));
    if let Ok(kids) = children.get(entity) {
        let slice: &[Entity] = kids;
        for &child in slice {
            set_render_layer_recursive(child, layer, children, commands);
        }
    }
}

/// Manages first-person hand visibility and animation.
/// Uses a separately-spawned hand entity as a child of the camera.
fn manage_first_person_hands(
    cam_state: Res<CameraState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    _mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    physics: Res<PhysicsState>,
    held_assets: Option<Res<FirstPersonHeldAssets>>,
    mut first_person_parts: ParamSet<(
        Query<(&mut Transform, &mut Visibility), With<FirstPersonHand>>,
        Query<
            (
                &mut Transform,
                &mut Visibility,
                &mut Mesh3d,
                &mut MeshMaterial3d<StandardMaterial>,
            ),
            With<FirstPersonHeldItem>,
        >,
        Query<(&mut Visibility, Option<&BoneRole>), With<FirstPersonArm>>,
    )>,
) {
    let is_first = cam_state.mode == CameraMode::FirstPerson;

    // Toggle visibility for Bedrock arms
    {
        let mut arms = first_person_parts.p2();
        for (mut visibility, _role) in &mut arms {
            if is_first {
                // Hide both arms in first person (the separate hand replaces them)
                *visibility = Visibility::Hidden;
            } else {
                *visibility = Visibility::Visible;
            }
        }
    }

    // Animate the separate first-person hand
    let w_pressed = keyboard.pressed(KeyCode::KeyW);
    let s_pressed = keyboard.pressed(KeyCode::KeyS);
    let a_pressed = keyboard.pressed(KeyCode::KeyA);
    let d_pressed = keyboard.pressed(KeyCode::KeyD);
    let walking = w_pressed || s_pressed || a_pressed || d_pressed;
    let just_swapped = keyboard.just_pressed(KeyCode::KeyQ);

    // Animation state machine
    let t = time.elapsed_secs();
    let swing: f32;

    if cam_state.attack_timer > 0.0 {
        swing = click_curve(cam_state.attack_timer, PLAYER_ATTACK_DURATION);
    } else if walking {
        // Walk animation: smooth sinusoidal bob
        swing = (t * 4.0).sin() * 0.4;
    } else if cam_state.use_timer > 0.0 {
        swing = click_curve(cam_state.use_timer, PLAYER_USE_DURATION) * 0.45;
    } else if just_swapped {
        // Swap item animation: small twist
        swing = 0.2;
    } else {
        // Idle: very subtle breathing motion
        swing = (t * 1.5).sin() * 0.02;
    }

    {
        let mut hand_query = first_person_parts.p0();
        for (mut transform, mut visibility) in &mut hand_query {
            if is_first {
                *visibility = Visibility::Visible;
                transform.translation = Vec3::new(0.64, -0.58, -0.92);
                transform.rotation = Quat::from_rotation_x(-0.72)
                    * Quat::from_rotation_y(-0.54)
                    * Quat::from_rotation_z(0.30);
                transform.scale = Vec3::new(0.58, 1.18, 0.58);
                if swing.abs() > 0.01 {
                    transform.rotation = transform.rotation
                        * Quat::from_rotation_z(-swing * 0.20)
                        * Quat::from_rotation_x(swing * 0.38);
                    transform.translation.y = -0.58 - swing.abs() * 0.030;
                    transform.translation.z = -0.92 - swing.abs() * 0.050;
                }
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }

    {
        let mut held_query = first_person_parts.p1();
        for (mut transform, mut visibility, mut mesh, mut material) in &mut held_query {
            if is_first {
                let Some(assets) = held_assets.as_ref() else {
                    *visibility = Visibility::Hidden;
                    continue;
                };

                let Some((next_mesh, next_material, is_block)) =
                    held_item_handles(physics.hotbar_items[physics.hotbar_slot], assets)
                else {
                    *visibility = Visibility::Hidden;
                    continue;
                };

                mesh.0 = next_mesh.clone();
                material.0 = next_material.clone();
                *visibility = Visibility::Visible;

                if is_block {
                    transform.translation = Vec3::new(0.50, -0.42, -1.08);
                    transform.rotation = Quat::from_rotation_x(-0.30)
                        * Quat::from_rotation_y(-0.62)
                        * Quat::from_rotation_z(0.03);
                    transform.scale = Vec3::splat(0.20);
                } else {
                    transform.translation = Vec3::new(0.50, -0.41, -0.78);
                    transform.rotation = Quat::from_rotation_x(-0.35)
                        * Quat::from_rotation_y(-0.66)
                        * Quat::from_rotation_z(-0.54);
                    transform.scale = Vec3::splat(0.86);
                }

                if swing.abs() > 0.01 {
                    transform.rotation = transform.rotation
                        * Quat::from_rotation_x(swing * 0.62)
                        * Quat::from_rotation_z(-swing * 0.28);
                    transform.translation.y -= swing.abs() * 0.04;
                    transform.translation.z -= swing.abs() * 0.04;
                }
            } else {
                *visibility = Visibility::Hidden;
            }
        }
    }
}

fn manage_third_person_held_item(
    cam_state: Res<CameraState>,
    physics: Res<PhysicsState>,
    held_assets: Option<Res<FirstPersonHeldAssets>>,
    mut held_query: Query<
        (
            &mut Visibility,
            &mut Mesh3d,
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Transform,
        ),
        With<ThirdPersonHeldItem>,
    >,
) {
    let visible_in_camera = cam_state.mode != CameraMode::FirstPerson;
    for (mut visibility, mut mesh, mut material, mut transform) in &mut held_query {
        if !visible_in_camera {
            *visibility = Visibility::Hidden;
            continue;
        }

        let Some(assets) = held_assets.as_ref() else {
            *visibility = Visibility::Hidden;
            continue;
        };

        let Some((next_mesh, next_material, is_block)) =
            held_item_handles(physics.hotbar_items[physics.hotbar_slot], assets)
        else {
            *visibility = Visibility::Hidden;
            continue;
        };

        mesh.0 = next_mesh.clone();
        material.0 = next_material.clone();
        *visibility = Visibility::Visible;

        if is_block {
            *transform = Transform::from_xyz(-0.03, -0.36, -0.18)
                .with_rotation(
                    Quat::from_rotation_x(-0.08)
                        * Quat::from_rotation_y(-0.18)
                        * Quat::from_rotation_z(0.18),
                )
                .with_scale(Vec3::splat(0.16));
        } else {
            *transform = Transform::from_xyz(-0.02, -0.34, -0.18)
                .with_rotation(
                    Quat::from_rotation_x(-0.42)
                        * Quat::from_rotation_y(-0.12)
                        * Quat::from_rotation_z(0.38),
                )
                .with_scale(Vec3::splat(0.56));
        }
    }
}

fn held_item_handles<'a>(
    item: Option<ItemKind>,
    assets: &'a FirstPersonHeldAssets,
) -> Option<(&'a Handle<Mesh>, &'a Handle<StandardMaterial>, bool)> {
    match item? {
        ItemKind::Block(shared::world::BlockKind::Grass) => {
            Some((&assets.grass_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::Dirt) => {
            Some((&assets.dirt_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::Stone) => {
            Some((&assets.stone_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::Sand) => {
            Some((&assets.sand_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::OakLog) => {
            Some((&assets.oak_log_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::OakLeaves) => {
            Some((&assets.oak_leaves_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::Cobblestone) => {
            Some((&assets.cobblestone_mesh, &assets.block_material, true))
        }
        ItemKind::Block(shared::world::BlockKind::Air | shared::world::BlockKind::Water) => None,
        ItemKind::Tool(ToolKind::WoodenPickaxe) => {
            Some((&assets.pickaxe_mesh, &assets.pickaxe_material, false))
        }
        ItemKind::Tool(ToolKind::WoodenAxe) => {
            Some((&assets.axe_mesh, &assets.axe_material, false))
        }
        ItemKind::Tool(ToolKind::WoodenSword) => {
            Some((&assets.sword_mesh, &assets.sword_material, false))
        }
    }
}

/// Spawns a simple first-person right hand as a child of the camera.
/// This runs after the camera is created (PostStartup).
fn spawn_first_person_hand(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    camera_query: Query<Entity, With<crate::world::components::PlayerCamera>>,
    pack: Res<GamePack>,
    asset_server: Res<AssetServer>,
) {
    let Ok(camera_entity) = camera_query.single() else {
        warn!("spawn_first_person_hand: no PlayerCamera found");
        return;
    };

    // Load player texture for the hand
    let hand_material = if let Some(texture_path) = pack
        .0
        .resolve_in(PackDirectory::Textures, "entities/player.png")
    {
        materials.add(StandardMaterial {
            base_color_texture: Some(
                asset_server.load_override(texture_path.to_string_lossy().to_string()),
            ),
            perceptual_roughness: 1.0,
            metallic: 0.0,
            alpha_mode: AlphaMode::Mask(0.1),
            cull_mode: None, // render both sides so hand is always visible
            unlit: true,
            ..default()
        })
    } else {
        materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.8, 0.6),
            cull_mode: None,
            unlit: true,
            ..default()
        })
    };

    let hand_mesh = load_first_person_arm_mesh(&pack, &mut meshes)
        .unwrap_or_else(|| meshes.add(Cuboid::new(0.25, 0.35, 0.25)));

    let block_atlas = images.add(make_block_item_atlas(&pack));
    let block_material = materials.add(StandardMaterial {
        base_color_texture: Some(block_atlas),
        base_color: Color::WHITE,
        perceptual_roughness: 0.88,
        reflectance: 0.12,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let pickaxe_texture = pack
        .0
        .resolve_in(PackDirectory::Textures, "items/beliung_kayu.png")
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()));
    let pickaxe_material = materials.add(StandardMaterial {
        base_color_texture: pickaxe_texture,
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let axe_texture = pack
        .0
        .resolve_in(PackDirectory::Textures, "items/wooden_axe.png")
        .or_else(|| {
            pack.0
                .resolve_in(PackDirectory::Textures, "items/beliung_kayu.png")
        })
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()));
    let sword_texture = pack
        .0
        .resolve_in(PackDirectory::Textures, "items/wooden_sword.png")
        .or_else(|| {
            pack.0
                .resolve_in(PackDirectory::Textures, "items/beliung_kayu.png")
        })
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()));
    let axe_material = materials.add(StandardMaterial {
        base_color_texture: axe_texture,
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let sword_material = materials.add(StandardMaterial {
        base_color_texture: sword_texture,
        base_color: Color::WHITE,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let held_assets = FirstPersonHeldAssets {
        grass_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::Grass)),
        dirt_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::Dirt)),
        stone_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::Stone)),
        sand_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::Sand)),
        oak_log_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::OakLog)),
        oak_leaves_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::OakLeaves)),
        cobblestone_mesh: meshes.add(make_block_item_mesh(shared::world::BlockKind::Cobblestone)),
        block_material: block_material.clone(),
        pickaxe_mesh: meshes.add(make_first_person_item_quad()),
        pickaxe_material: pickaxe_material.clone(),
        axe_mesh: meshes.add(make_first_person_item_quad()),
        axe_material,
        sword_mesh: meshes.add(make_first_person_item_quad()),
        sword_material,
    };
    commands.insert_resource(held_assets.clone());

    commands.entity(camera_entity).with_children(|parent| {
        parent.spawn((
            Mesh3d(hand_mesh),
            MeshMaterial3d(hand_material),
            Transform::from_translation(Vec3::new(0.64, -0.58, -0.92))
                .with_rotation(
                    Quat::from_rotation_x(-0.72)
                        * Quat::from_rotation_y(-0.54)
                        * Quat::from_rotation_z(0.30),
                )
                .with_scale(Vec3::new(0.58, 1.18, 0.58)),
            Visibility::Visible, // visible by default; manage_first_person_hands handles hiding
            FirstPersonHand,
            RenderLayers::layer(0),
            NotShadowCaster,
        ));
        parent.spawn((
            Mesh3d(held_assets.grass_mesh.clone()),
            MeshMaterial3d(held_assets.block_material.clone()),
            Transform::from_translation(Vec3::new(0.50, -0.42, -1.08))
                .with_rotation(
                    Quat::from_rotation_x(-0.30)
                        * Quat::from_rotation_y(-0.62)
                        * Quat::from_rotation_z(0.03),
                )
                .with_scale(Vec3::splat(0.20)),
            Visibility::Visible,
            FirstPersonHeldItem,
            RenderLayers::layer(0),
            NotShadowCaster,
        ));
    });

    info!("spawned first-person hand entity");
}

fn spawn_third_person_held_item(
    mut commands: Commands,
    held_assets: Option<Res<FirstPersonHeldAssets>>,
    arm_query: Query<(Entity, &BoneRole)>,
    bone_names: Query<(Entity, &PlayerBoneName)>,
) {
    let Some(assets) = held_assets.as_ref() else {
        warn!("spawn_third_person_held_item: first-person held assets not ready");
        return;
    };
    let right_hand = bone_names
        .iter()
        .find(|(_, name)| name.0 == "pra_right_arm")
        .map(|(entity, _)| entity)
        .or_else(|| {
            arm_query
                .iter()
                .find(|(_, role)| **role == BoneRole::ArmRight)
                .map(|(entity, _)| entity)
        });
    let Some(right_hand) = right_hand else {
        warn!("spawn_third_person_held_item: no right arm/forearm bone found");
        return;
    };

    commands.entity(right_hand).with_children(|arm| {
        arm.spawn((
            Mesh3d(assets.grass_mesh.clone()),
            MeshMaterial3d(assets.block_material.clone()),
            Transform::from_xyz(-0.03, -0.36, -0.18)
                .with_rotation(
                    Quat::from_rotation_x(-0.08)
                        * Quat::from_rotation_y(-0.18)
                        * Quat::from_rotation_z(0.18),
                )
                .with_scale(Vec3::splat(0.16)),
            Visibility::Hidden,
            ThirdPersonHeldItem,
            RenderLayers::layer(0),
            NotShadowCaster,
        ));
    });
}

#[allow(dead_code)]
fn _legacy_find_right_arm(arm_query: &Query<(Entity, &BoneRole)>) -> Option<Entity> {
    arm_query
        .iter()
        .find(|(_, role)| **role == BoneRole::ArmRight)
        .map(|(entity, _)| entity)
}

fn spawn_first_person_shadow_proxy(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    controller: Query<Entity, With<PlayerController>>,
) {
    let Ok(controller_entity) = controller.single() else {
        warn!("spawn_first_person_shadow_proxy: no PlayerController found");
        return;
    };

    let material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.08, 0.08),
        perceptual_roughness: 1.0,
        ..default()
    });
    let body = meshes.add(Cuboid::new(0.55, 0.72, 0.28));
    let head = meshes.add(Cuboid::new(0.50, 0.50, 0.50));
    let arm = meshes.add(Cuboid::new(0.20, 0.70, 0.22));
    let leg = meshes.add(Cuboid::new(0.22, 0.72, 0.22));

    commands.entity(controller_entity).with_children(|player| {
        player
            .spawn((
                Transform::default(),
                Visibility::Hidden,
                FirstPersonShadowProxy,
                RenderLayers::layer(1),
                NoFrustumCulling,
            ))
            .with_children(|proxy| {
                for (mesh, transform) in [
                    (body.clone(), Transform::from_xyz(0.0, 1.03, 0.0)),
                    (head.clone(), Transform::from_xyz(0.0, 1.60, 0.0)),
                    (arm.clone(), Transform::from_xyz(-0.42, 1.02, 0.0)),
                    (arm.clone(), Transform::from_xyz(0.42, 1.02, 0.0)),
                    (leg.clone(), Transform::from_xyz(-0.15, 0.36, 0.0)),
                    (leg.clone(), Transform::from_xyz(0.15, 0.36, 0.0)),
                ] {
                    proxy.spawn((
                        Mesh3d(mesh),
                        MeshMaterial3d(material.clone()),
                        transform,
                        RenderLayers::layer(1),
                        NoFrustumCulling,
                    ));
                }
            });
    });
}

fn first_person_shadow_proxy_visibility(
    cam_state: Res<CameraState>,
    mut proxy: Query<&mut Visibility, With<FirstPersonShadowProxy>>,
) {
    let visible = cam_state.mode == CameraMode::FirstPerson;
    for mut visibility in &mut proxy {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

#[derive(Clone, Copy, Default)]
struct SampledBonePose {
    rotation: Option<Quat>,
    position: Option<Vec3>,
}

struct SampledAnimation<'a> {
    root: Option<&'a Value>,
    time: f32,
}

impl PlayerAnimationData {
    fn sample(&self, name: &str, time: f32) -> SampledAnimation<'_> {
        let root = self
            .root
            .as_ref()
            .and_then(|root| root.get("animations"))
            .and_then(|animations| animations.get(name));
        let length = root
            .and_then(|animation| animation.get("animation_length"))
            .and_then(Value::as_f64)
            .unwrap_or(1.0) as f32;
        let looped = root
            .and_then(|animation| animation.get("loop"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let time = if looped && length > 0.0 {
            time.rem_euclid(length)
        } else {
            time.clamp(0.0, length.max(0.0))
        };

        SampledAnimation { root, time }
    }
}

impl SampledAnimation<'_> {
    fn bone(&self, name: &str) -> Option<SampledBonePose> {
        let bone = self.root?.get("bones")?.get(name)?;
        Some(SampledBonePose {
            rotation: bone
                .get("rotation")
                .and_then(|track| sample_vec3_track(track, self.time))
                .map(rotation_degrees_to_quat),
            position: bone
                .get("position")
                .and_then(|track| sample_vec3_track(track, self.time))
                .map(|value| value / 16.0),
        })
    }
}

fn blend_pose(
    a: Option<SampledBonePose>,
    b: Option<SampledBonePose>,
    weight: f32,
) -> SampledBonePose {
    let weight = weight.clamp(0.0, 1.0);
    match (a, b) {
        (Some(a), Some(b)) => SampledBonePose {
            rotation: match (a.rotation, b.rotation) {
                (Some(a), Some(b)) => Some(a.slerp(b, weight)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
            position: match (a.position, b.position) {
                (Some(a), Some(b)) => Some(a.lerp(b, weight)),
                (Some(a), None) => Some(a),
                (None, Some(b)) => Some(b),
                (None, None) => None,
            },
        },
        (Some(a), None) => a,
        (None, Some(b)) => b,
        (None, None) => SampledBonePose::default(),
    }
}

fn sample_vec3_track(track: &Value, time: f32) -> Option<Vec3> {
    if let Some(array) = track.as_array() {
        return array_to_vec3(array, time);
    }

    let object = track.as_object()?;
    let mut keys = object
        .iter()
        .filter_map(|(key, value)| key.parse::<f32>().ok().map(|key| (key, value)))
        .collect::<Vec<_>>();
    keys.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));

    let (first_time, first_value) = *keys.first()?;
    if time <= first_time {
        return value_to_vec3(first_value, time);
    }
    let (last_time, last_value) = *keys.last()?;
    if time >= last_time {
        return value_to_vec3(last_value, time);
    }

    for pair in keys.windows(2) {
        let (a_time, a_value) = pair[0];
        let (b_time, b_value) = pair[1];
        if time >= a_time && time <= b_time {
            let a = value_to_vec3(a_value, time)?;
            let b = value_to_vec3(b_value, time)?;
            let alpha = ((time - a_time) / (b_time - a_time).max(0.0001)).clamp(0.0, 1.0);
            return Some(a.lerp(b, smoothstep(alpha)));
        }
    }

    None
}

fn value_to_vec3(value: &Value, time: f32) -> Option<Vec3> {
    array_to_vec3(value.as_array()?, time)
}

fn array_to_vec3(array: &[Value], time: f32) -> Option<Vec3> {
    Some(Vec3::new(
        eval_anim_value(array.first()?, time)?,
        eval_anim_value(array.get(1)?, time)?,
        eval_anim_value(array.get(2)?, time)?,
    ))
}

fn eval_anim_value(value: &Value, time: f32) -> Option<f32> {
    if let Some(number) = value.as_f64() {
        return Some(number as f32);
    }
    eval_anim_expr(value.as_str()?, time)
}

fn eval_anim_expr(expr: &str, time: f32) -> Option<f32> {
    let expr = expr.trim();
    if let Ok(number) = expr.parse::<f32>() {
        return Some(number);
    }

    if let Some((a, b)) = split_top_level_operator(expr, &['+', '-']) {
        return if expr.as_bytes().get(a.len()) == Some(&b'+') {
            Some(eval_anim_expr(a, time)? + eval_anim_expr(b, time)?)
        } else {
            Some(eval_anim_expr(a, time)? - eval_anim_expr(b, time)?)
        };
    }
    if let Some((a, b)) = split_top_level_operator(expr, &['*', '/']) {
        return if expr.as_bytes().get(a.len()) == Some(&b'*') {
            Some(eval_anim_expr(a, time)? * eval_anim_expr(b, time)?)
        } else {
            Some(eval_anim_expr(a, time)? / eval_anim_expr(b, time)?)
        };
    }
    if let Some(stripped) = expr.strip_prefix('-') {
        return Some(-eval_anim_expr(stripped, time)?);
    }
    if let Some(inner) = strip_enclosing_parens(expr) {
        return eval_anim_expr(inner, time);
    }

    if let Some((a, b)) = split_top_level(expr, "+") {
        return Some(eval_anim_expr(a, time)? + eval_anim_expr(b, time)?);
    }
    if let Some((a, b)) = split_top_level(expr, "-") {
        return Some(eval_anim_expr(a, time)? - eval_anim_expr(b, time)?);
    }
    if let Some((a, b)) = split_top_level(expr, "*") {
        return Some(eval_anim_expr(a, time)? * eval_anim_expr(b, time)?);
    }
    if let Some(inner) = expr
        .strip_prefix("Math.sin(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return Some((eval_anim_expr(inner, time)?).to_radians().sin());
    }
    if let Some(inner) = expr
        .strip_prefix("Math.cos(")
        .and_then(|s| s.strip_suffix(')'))
    {
        return Some((eval_anim_expr(inner, time)?).to_radians().cos());
    }
    if let Some(inner) = expr
        .strip_prefix("Math.max(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (a, b) = split_top_level(inner, ",")?;
        return Some(eval_anim_expr(a, time)?.max(eval_anim_expr(b, time)?));
    }
    if let Some(inner) = expr
        .strip_prefix("Math.min(")
        .and_then(|s| s.strip_suffix(')'))
    {
        let (a, b) = split_top_level(inner, ",")?;
        return Some(eval_anim_expr(a, time)?.min(eval_anim_expr(b, time)?));
    }
    if expr == "q.anim_time" {
        return Some(time);
    }

    None
}

fn strip_enclosing_parens(expr: &str) -> Option<&str> {
    let inner = expr.strip_prefix('(')?.strip_suffix(')')?;
    let mut depth = 0i32;
    for (index, ch) in expr.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 && index < expr.len() - 1 {
                    return None;
                }
            }
            _ => {}
        }
    }
    Some(inner.trim())
}

fn split_top_level_operator<'a>(expr: &'a str, operators: &[char]) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    let mut previous_non_space: Option<char> = None;
    let chars = expr.char_indices().collect::<Vec<_>>();
    for &(index, ch) in chars.iter().rev() {
        match ch {
            ')' => depth += 1,
            '(' => depth -= 1,
            _ => {}
        }
        if depth != 0 || !operators.contains(&ch) {
            if !ch.is_whitespace() {
                previous_non_space = Some(ch);
            }
            continue;
        }
        let left = expr[..index].trim();
        let right = expr[index + ch.len_utf8()..].trim();
        if left.is_empty() || right.is_empty() {
            continue;
        }
        let unary = matches!(ch, '+' | '-')
            && matches!(
                left.chars().rev().find(|c| !c.is_whitespace()),
                None | Some('(' | ',' | '+' | '-' | '*' | '/')
            );
        if unary {
            continue;
        }
        let _ = previous_non_space;
        return Some((left, right));
    }
    None
}

fn split_top_level<'a>(expr: &'a str, needle: &str) -> Option<(&'a str, &'a str)> {
    let mut depth = 0i32;
    for (index, ch) in expr.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth -= 1,
            _ => {}
        }
        if depth == 0 && expr[index..].starts_with(needle) {
            return Some((expr[..index].trim(), expr[index + needle.len()..].trim()));
        }
    }
    None
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn rotation_degrees_to_quat(rotation: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::XYZ,
        rotation.x.to_radians(),
        rotation.y.to_radians(),
        rotation.z.to_radians(),
    )
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

fn click_curve(timer: f32, duration: f32) -> f32 {
    if timer <= 0.0 {
        return 0.0;
    }
    let phase = 1.0 - (timer / duration).clamp(0.0, 1.0);
    (std::f32::consts::PI * phase).sin()
}

fn load_first_person_arm_mesh(pack: &GamePack, meshes: &mut Assets<Mesh>) -> Option<Handle<Mesh>> {
    let model_json = pack.0.load_string("models/entities/player.geo.json").ok()?;
    let parsed = serde_json::from_str::<BedrockGeometryFile>(&model_json).ok()?;
    let geometry = parsed.geometries.first()?;
    let tex_w = geometry
        .description
        .as_ref()
        .and_then(|d| d.texture_width)
        .unwrap_or(64.0);
    let tex_h = geometry
        .description
        .as_ref()
        .and_then(|d| d.texture_height)
        .unwrap_or(64.0);

    let cube = geometry
        .bones
        .iter()
        .find(|bone| bone.name == "prfa_right_forearm")
        .or_else(|| {
            geometry
                .bones
                .iter()
                .find(|bone| bone.name == "pra_right_arm")
        })?
        .cubes
        .first()?;

    let inflate = cube.inflate.unwrap_or(0.0);
    let cube_size_px = [
        cube.size[0] + inflate * 2.0,
        cube.size[1] + inflate * 2.0,
        cube.size[2] + inflate * 2.0,
    ];
    let mesh = build_cube_mesh(
        cube_size_px,
        cube.uv.as_ref(),
        cube.uv_size.or(Some([cube.size[0], cube.size[1]])),
        tex_w,
        tex_h,
    );

    Some(meshes.add(mesh))
}

fn make_block_item_atlas(pack: &GamePack) -> Image {
    const TILE: u32 = 16;
    let sources = [
        ("blocks/stone.png", [110, 110, 110, 255]),
        ("blocks/dirt.png", [128, 90, 50, 255]),
        ("blocks/grass_block_top.png", [80, 170, 60, 255]),
        ("blocks/grass_block_side.png", [95, 140, 55, 255]),
        ("blocks/sand.png", [210, 194, 125, 255]),
        ("blocks/oak_log.png", [112, 72, 36, 255]),
        ("blocks/oak_log_top.png", [158, 124, 78, 255]),
        ("blocks/oak_leaves.png", [60, 135, 48, 220]),
        ("blocks/cobblestone.png", [100, 100, 100, 255]),
    ];

    let mut atlas = vec![255u8; (TILE * sources.len() as u32 * TILE * 4) as usize];
    for (tile_index, (relative_path, fallback)) in sources.iter().enumerate() {
        let pixels = pack
            .0
            .resolve_in(PackDirectory::Textures, relative_path)
            .and_then(|path| image::open(path).ok())
            .map(|img| {
                img.resize_exact(TILE, TILE, image::imageops::FilterType::Nearest)
                    .to_rgba8()
            });

        for y in 0..TILE {
            for x in 0..TILE {
                let dst_x = tile_index as u32 * TILE + x;
                let dst = ((y * TILE * sources.len() as u32 + dst_x) * 4) as usize;
                let mut rgba = pixels
                    .as_ref()
                    .map(|img| img.get_pixel(x, y).0)
                    .unwrap_or(*fallback);
                if tile_index == 2 {
                    rgba[0] = ((rgba[0] as u16 * 105) / 255).min(255) as u8;
                    rgba[1] = ((rgba[1] as u16 * 180) / 255).min(255) as u8;
                    rgba[2] = ((rgba[2] as u16 * 85) / 255).min(255) as u8;
                }
                atlas[dst..dst + 4].copy_from_slice(&rgba);
            }
        }
    }

    Image::new(
        Extent3d {
            width: TILE * sources.len() as u32,
            height: TILE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        atlas,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn make_block_item_mesh(kind: shared::world::BlockKind) -> Mesh {
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    let mut indices = Vec::new();

    for &(normal, verts, tile) in &[
        (
            [0.0, 0.0, -1.0],
            [
                [-0.5, -0.5, -0.5],
                [-0.5, 0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, -0.5, -0.5],
            ],
            block_item_tile(kind, "side"),
        ),
        (
            [0.0, 0.0, 1.0],
            [
                [0.5, -0.5, 0.5],
                [0.5, 0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, -0.5, 0.5],
            ],
            block_item_tile(kind, "side"),
        ),
        (
            [-1.0, 0.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, 0.5, 0.5],
                [-0.5, 0.5, -0.5],
                [-0.5, -0.5, -0.5],
            ],
            block_item_tile(kind, "side"),
        ),
        (
            [1.0, 0.0, 0.0],
            [
                [0.5, -0.5, -0.5],
                [0.5, 0.5, -0.5],
                [0.5, 0.5, 0.5],
                [0.5, -0.5, 0.5],
            ],
            block_item_tile(kind, "side"),
        ),
        (
            [0.0, 1.0, 0.0],
            [
                [-0.5, 0.5, -0.5],
                [-0.5, 0.5, 0.5],
                [0.5, 0.5, 0.5],
                [0.5, 0.5, -0.5],
            ],
            block_item_tile(kind, "top"),
        ),
        (
            [0.0, -1.0, 0.0],
            [
                [-0.5, -0.5, 0.5],
                [-0.5, -0.5, -0.5],
                [0.5, -0.5, -0.5],
                [0.5, -0.5, 0.5],
            ],
            block_item_tile(kind, "bottom"),
        ),
    ] {
        let base = positions.len() as u32;
        positions.extend_from_slice(&verts);
        normals.extend_from_slice(&[normal; 4]);
        uvs.extend_from_slice(&tile_uvs(tile));
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices))
}

fn block_item_tile(kind: shared::world::BlockKind, face: &str) -> usize {
    match kind {
        shared::world::BlockKind::Stone => 0,
        shared::world::BlockKind::Dirt => 1,
        shared::world::BlockKind::Grass if face == "top" => 2,
        shared::world::BlockKind::Grass if face == "bottom" => 1,
        shared::world::BlockKind::Grass => 3,
        shared::world::BlockKind::Sand => 4,
        shared::world::BlockKind::OakLog if face == "top" || face == "bottom" => 6,
        shared::world::BlockKind::OakLog => 5,
        shared::world::BlockKind::OakLeaves => 7,
        shared::world::BlockKind::Cobblestone => 8,
        shared::world::BlockKind::Water => 0,
        shared::world::BlockKind::Air => 0,
    }
}

fn tile_uvs(tile: usize) -> [[f32; 2]; 4] {
    let tiles = 9.0;
    let tile_px = 16.0;
    let u_pad = 0.5 / (tile_px * tiles);
    let v_pad = 0.5 / tile_px;
    let u0 = tile as f32 / tiles + u_pad;
    let u1 = (tile as f32 + 1.0) / tiles - u_pad;
    let v0 = v_pad;
    let v1 = 1.0 - v_pad;
    [[u0, v1], [u0, v0], [u1, v0], [u1, v1]]
}

fn make_first_person_item_quad() -> Mesh {
    use bevy::asset::RenderAssetUsages;
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    let w = 0.30;
    let h = 0.30;
    let d = 0.035;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-w, -h, d],
            [w, -h, d],
            [w, h, d],
            [-w, h, d],
            [w, -h, -d],
            [-w, -h, -d],
            [-w, h, -d],
            [w, h, -d],
            [-w, h, d],
            [w, h, d],
            [w, h, -d],
            [-w, h, -d],
            [-w, -h, -d],
            [w, -h, -d],
            [w, -h, d],
            [-w, -h, d],
            [w, -h, d],
            [w, -h, -d],
            [w, h, -d],
            [w, h, d],
            [-w, -h, -d],
            [-w, -h, d],
            [-w, h, d],
            [-w, h, -d],
        ],
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        vec![
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 0.0, -1.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, -1.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
        ],
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.0, 0.0],
            [0.0, 0.03],
            [1.0, 0.03],
            [1.0, 0.0],
            [0.0, 0.0],
            [0.0, 1.0],
            [1.0, 1.0],
            [1.0, 0.97],
            [0.0, 0.97],
            [0.97, 1.0],
            [1.0, 1.0],
            [1.0, 0.0],
            [0.97, 0.0],
            [0.0, 1.0],
            [0.03, 1.0],
            [0.03, 0.0],
            [0.0, 0.0],
        ],
    );
    let mut indices = Vec::new();
    for base in (0..24).step_by(4) {
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    mesh.insert_indices(Indices::U32(indices));
    mesh
}
