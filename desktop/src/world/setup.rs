//! Startup systems that prepare block models and spawn the initial scene.
//!
//! - `prepare_block_models` – loads Bedrock block-model JSON, generates meshes.
//! - `spawn_sample_entities` – places camera, player marker, blocks, water, items.

use super::components::*;
use super::mesh::*;
use super::resources::*;
use crate::{GamePack, LoadedPackTextures, LoadedWorld};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::{
    CascadeShadowConfigBuilder, DirectionalLightShadowMap, NotShadowCaster, NotShadowReceiver,
    VolumetricFog, VolumetricLight,
};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::render::view::Hdr;
use shared::bedrock::parse_geometry_json;
use shared::world::BlockKind;
use std::collections::HashMap;

const SUN_POSITION: Vec3 = Vec3::new(460.0, 620.0, 440.0);

/// Load block models from the game pack and prepare mesh/material handles.
pub fn prepare_block_models(
    _commands: Commands,
    textures: Option<Res<LoadedPackTextures>>,
    mut images: ResMut<Assets<Image>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    pack: Res<GamePack>,
    mut wm: ResMut<WorldModels>,
) {
    if textures.is_none() {
        return;
    }

    // Build a simple white placeholder material using a pack texture path.
    if let Some(_path) = pack
        .0
        .resolve_in(shared::pack::PackDirectory::Textures, "blocks/batu.png")
    {
        let handle = images.add(Image::new_fill(
            Extent3d {
                width: 16,
                height: 16,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            &[255, 255, 255, 255],
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        ));

        let material = materials.add(StandardMaterial {
            base_color_texture: Some(handle.clone()),
            ..Default::default()
        });

        wm.block_meshes = HashMap::new();
        wm.block_material = Some(material);
    }

    // Pre-load block models from bedrock model JSON.
    for kind in [BlockKind::Grass, BlockKind::Dirt, BlockKind::Stone] {
        if let Some(model_path) = kind.bedrock_geometry_model_path() {
            if let Ok(json) = pack.0.load_string(model_path) {
                if let Ok(model) = parse_geometry_json(&json) {
                    if let Some(mesh_handle) = build_mesh_from_geometry(&model, &mut meshes) {
                        wm.block_meshes.insert(kind, mesh_handle);
                        continue;
                    }
                }
            }
        }
    }

    info!("prepared bedrock block models for world renderer");
}

/// Spawn player + first-person camera + terrain.
pub fn spawn_player_and_terrain(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    _wm: Res<WorldModels>,
    pack: Res<GamePack>,
    asset_server: Res<AssetServer>,
    mut images: ResMut<Assets<Image>>,
    world: Option<Res<LoadedWorld>>,
) {
    commands.insert_resource(DirectionalLightShadowMap { size: 4096 });

    // ── Lighting (sun + ambient) ──
    commands.spawn((
        DirectionalLight {
            color: Color::srgb(1.0, 0.96, 0.88),
            illuminance: 32_000.0,
            shadows_enabled: true,
            shadow_depth_bias: 0.005, // Reduced from 0.018 – less shadow acne, softer
            shadow_normal_bias: 0.8,  // Reduced from 1.8 – less peter-panning
            ..default()
        },
        Transform::from_translation(SUN_POSITION).looking_at(Vec3::ZERO, Vec3::Y),
        CascadeShadowConfigBuilder {
            num_cascades: 4,
            minimum_distance: 0.5,
            first_cascade_far_bound: 18.0, // Slightly wider first cascade
            maximum_distance: 280.0,       // Extended for taller terrain
            overlap_proportion: 0.2,
        }
        .build(),
        RenderLayers::from_layers(&[0, 1]),
        VolumetricLight,
        SunLight,
    ));
    // Set global ambient light (used when no per-camera AmbientLight is set)
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgba(0.62, 0.72, 0.86, 1.0),
        brightness: 0.50, // Increased from 0.42 – brighter daylight shadows
        affects_lightmapped_meshes: true,
    });

    spawn_sky(
        &mut commands,
        &mut meshes,
        &mut materials,
        &mut images,
        &pack,
        &asset_server,
    );
    spawn_block_outline(&mut commands, &mut meshes, &mut materials);

    // ── Sun visual (large glowing sphere) ──
    // ── Player (separate from camera) ──
    commands.spawn((
        Transform::from_translation(Vec3::new(0.5, 86.0, 0.5)),
        Visibility::Visible,
        PlayerController,
        EntityAabb::player(),
        EntityTransformState::default(),
    ));

    // ── Camera (separate sibling entity, NOT child of player) ──
    commands.spawn((
        Camera3d::default(),
        Hdr,
        Tonemapping::TonyMcMapface,
        DepthPrepass,
        NormalPrepass,
        Bloom {
            intensity: 0.025,
            low_frequency_boost: 0.20,
            ..Bloom::NATURAL
        },
        DistanceFog {
            color: Color::srgba(0.58, 0.72, 0.90, 0.18),
            directional_light_color: Color::srgba(1.0, 0.88, 0.66, 0.22),
            directional_light_exponent: 8.0,
            falloff: FogFalloff::Linear {
                start: 120.0,
                end: 360.0,
            },
        },
        VolumetricFog {
            ambient_color: Color::srgb(0.58, 0.70, 0.86),
            ambient_intensity: 0.018,
            step_count: 12,
            jitter: 0.18,
        },
        Camera {
            clear_color: ClearColorConfig::Custom(Color::srgb(0.46, 0.66, 0.94)),
            ..default()
        },
        Transform::from_translation(Vec3::new(0.5, 86.0 + 1.62, 0.5)),
        Visibility::Visible,
        PlayerCamera,
        RenderLayers::layer(0),
    ));

    // ── Terrain via chunk system ──
    super::chunk::setup_chunk_world(commands, world, meshes, materials, images, pack);

    info!("spawned player, camera, and terrain");
}

fn spawn_sky(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    images: &mut Assets<Image>,
    pack: &GamePack,
    asset_server: &AssetServer,
) {
    let sky_texture = images.add(make_sky_gradient_image());
    let sky_material = materials.add(StandardMaterial {
        base_color_texture: Some(sky_texture),
        unlit: true,
        cull_mode: None,
        perceptual_roughness: 1.0,
        reflectance: 0.02,
        ..default()
    });

    let sky_root = commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, 0.0),
            Visibility::Visible,
            SkyDome,
        ))
        .id();

    commands.entity(sky_root).with_children(|sky| {
        sky.spawn((
            Mesh3d(meshes.add(Sphere::new(850.0).mesh().uv(64, 32))),
            MeshMaterial3d(sky_material),
            Transform::default(),
            NotShadowCaster,
            NotShadowReceiver,
        ));
    });

    let sun_texture = pack
        .0
        .resolve_in(shared::pack::PackDirectory::Textures, "environment/sun.png")
        .or_else(|| {
            pack.0.resolve_in(
                shared::pack::PackDirectory::Textures,
                "environment/celestial/sun.png",
            )
        })
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()));

    let sun_material = materials.add(StandardMaterial {
        base_color_texture: sun_texture,
        base_color: Color::srgb(1.0, 0.96, 0.82),
        emissive: LinearRgba::rgb(5.0, 4.5, 3.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.entity(sky_root).with_children(|sky| {
        sky.spawn((
            Mesh3d(meshes.add(make_billboard_quad(240.0, 240.0))),
            MeshMaterial3d(sun_material),
            Transform::from_translation(SUN_POSITION),
            CelestialBillboard,
            SunVisual,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    });

    let moon_texture = pack
        .0
        .resolve_in(
            shared::pack::PackDirectory::Textures,
            "environment/celestial/moon/full_moon.png",
        )
        .or_else(|| {
            pack.0.resolve_in(
                shared::pack::PackDirectory::Textures,
                "environment/moon_phases.png",
            )
        })
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()));

    let moon_material = materials.add(StandardMaterial {
        base_color_texture: moon_texture,
        base_color: Color::srgba(0.88, 0.90, 0.98, 0.86),
        emissive: LinearRgba::rgb(1.2, 1.35, 1.7),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    commands.entity(sky_root).with_children(|sky| {
        sky.spawn((
            Mesh3d(meshes.add(make_billboard_quad(42.0, 42.0))),
            MeshMaterial3d(moon_material),
            Transform::from_translation(Vec3::new(-110.0, 104.0, -86.0)),
            CelestialBillboard,
            MoonVisual,
            NotShadowCaster,
            NotShadowReceiver,
        ));
    });

    let star_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.88, 0.92, 1.0, 0.0),
        emissive: LinearRgba::BLACK,
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let star_mesh = meshes.add(make_billboard_quad(1.0, 1.0));
    commands.entity(sky_root).with_children(|sky| {
        for i in 0..96 {
            let angle = ((i as f32 * 0.618_034).fract()) * std::f32::consts::TAU;
            let radius = 640.0 + (i % 7) as f32 * 18.0;
            let y = 230.0 + ((i * 37 % 100) as f32 / 100.0) * 360.0;
            let pos = Vec3::new(
                angle.cos() * radius,
                y,
                angle.sin() * radius + ((i * 17 % 80) as f32 - 40.0),
            );
            let size = 1.8 + (i % 3) as f32 * 0.8;
            sky.spawn((
                Mesh3d(star_mesh.clone()),
                MeshMaterial3d(star_material.clone()),
                Transform::from_translation(pos).with_scale(Vec3::splat(size)),
                CelestialBillboard,
                StarVisual,
                NotShadowCaster,
                NotShadowReceiver,
            ));
        }
    });

    let cloud_material = materials.add(StandardMaterial {
        base_color: Color::srgba(1.0, 1.0, 1.0, 0.62),
        alpha_mode: AlphaMode::AlphaToCoverage,
        unlit: true,
        cull_mode: None,
        ..default()
    });

    let cloud_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));
    commands.entity(sky_root).with_children(|sky| {
        // ── Inner ring (dense, close to player) ──
        const CLOUD_BASE_Y: f32 = 280.0;
        for ring in 0..3 {
            let angle = ring as f32 * 120.0_f32.to_radians();
            let ring_center = Vec3::new(
                angle.cos() * 240.0,
                CLOUD_BASE_Y + ring as f32 * 8.0,
                angle.sin() * 240.0,
            );
            let puffs = 8 + ring * 2;
            for i in 0..puffs {
                let a = i as f32 / puffs as f32 * std::f32::consts::TAU;
                let offset =
                    Vec3::new(a.cos() * 70.0, ((i % 3) as f32 - 1.0) * 3.0, a.sin() * 70.0);
                let scale = Vec3::new(
                    42.0 + (i % 3) as f32 * 14.0,
                    7.0 + (i % 2) as f32 * 3.0,
                    26.0 + (i % 4) as f32 * 8.0,
                );
                sky.spawn((
                    Mesh3d(cloud_mesh.clone()),
                    MeshMaterial3d(cloud_material.clone()),
                    Transform::from_translation(ring_center + offset).with_scale(scale),
                    LightweightCloud,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
        // ── Outer ring (more spread out) ──
        for (base, puffs) in [
            (Vec3::new(-520.0, CLOUD_BASE_Y - 8.0, -560.0), 14),
            (Vec3::new(-260.0, CLOUD_BASE_Y + 6.0, -620.0), 12),
            (Vec3::new(40.0, CLOUD_BASE_Y, -600.0), 11),
            (Vec3::new(330.0, CLOUD_BASE_Y - 4.0, -500.0), 15),
            (Vec3::new(560.0, CLOUD_BASE_Y - 12.0, -180.0), 10),
            (Vec3::new(-520.0, CLOUD_BASE_Y - 6.0, 160.0), 10),
            (Vec3::new(420.0, 168.0, 260.0), 12),
            (Vec3::new(-600.0, 174.0, -80.0), 11),
            (Vec3::new(-100.0, 180.0, 520.0), 13),
        ] {
            for i in 0..puffs {
                let offset = Vec3::new(
                    (i as f32 % 4.0) * 34.0,
                    ((i % 3) as f32 - 1.0) * 3.2,
                    (i as f32 / 4.0).floor() * 28.0,
                );
                let scale = Vec3::new(
                    44.0 + (i % 3) as f32 * 14.0,
                    7.0 + (i % 2) as f32 * 3.0,
                    26.0 + (i % 4) as f32 * 9.0,
                );
                sky.spawn((
                    Mesh3d(cloud_mesh.clone()),
                    MeshMaterial3d(cloud_material.clone()),
                    Transform::from_translation(base + offset).with_scale(scale),
                    LightweightCloud,
                    NotShadowCaster,
                    NotShadowReceiver,
                ));
            }
        }
    });
}

fn spawn_block_outline(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.0, 0.0, 0.0, 0.72),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    });
    let thickness = 0.025;
    let length = 1.035;
    let x_edge = meshes.add(Cuboid::new(length, thickness, thickness));
    let y_edge = meshes.add(Cuboid::new(thickness, length, thickness));
    let z_edge = meshes.add(Cuboid::new(thickness, thickness, length));

    commands
        .spawn((
            Transform::default(),
            Visibility::Hidden,
            BlockOutline,
            NotShadowCaster,
            NotShadowReceiver,
        ))
        .with_children(|outline| {
            for y in [-0.515, 0.515] {
                for z in [-0.515, 0.515] {
                    outline.spawn((
                        Mesh3d(x_edge.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0.0, y, z),
                    ));
                }
            }
            for x in [-0.515, 0.515] {
                for z in [-0.515, 0.515] {
                    outline.spawn((
                        Mesh3d(y_edge.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(x, 0.0, z),
                    ));
                }
            }
            for x in [-0.515, 0.515] {
                for y in [-0.515, 0.515] {
                    outline.spawn((
                        Mesh3d(z_edge.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(x, y, 0.0),
                    ));
                }
            }
        });
}

fn make_billboard_quad(width: f32, height: f32) -> Mesh {
    use bevy::mesh::Indices;
    use bevy::render::render_resource::PrimitiveTopology;

    let hw = width * 0.5;
    let hh = height * 0.5;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-hw, -hh, 0.0],
            [hw, -hh, 0.0],
            [hw, hh, 0.0],
            [-hw, hh, 0.0],
        ],
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4]);
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_UV_0,
        vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]],
    );
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    mesh
}

fn make_sky_gradient_image() -> Image {
    const WIDTH: u32 = 4;
    const HEIGHT: u32 = 64;
    let mut data = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);

    for y in 0..HEIGHT {
        let t = y as f32 / (HEIGHT - 1) as f32;
        let horizon = Vec3::new(0.66, 0.82, 1.0);
        let mid = Vec3::new(0.44, 0.67, 0.96);
        let zenith = Vec3::new(0.25, 0.48, 0.86);
        let color = if t < 0.45 {
            horizon.lerp(mid, t / 0.45)
        } else {
            mid.lerp(zenith, (t - 0.45) / 0.55)
        };

        for _ in 0..WIDTH {
            data.extend_from_slice(&[
                (color.x * 255.0) as u8,
                (color.y * 255.0) as u8,
                (color.z * 255.0) as u8,
                255,
            ]);
        }
    }

    Image::new(
        Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}
