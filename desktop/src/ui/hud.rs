//! Heads-up display: Minecraft-style crosshair, hearts, hotbar, and item icons.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::window::PrimaryWindow;
use shared::pack::PackDirectory;

use super::GuiScale;
use super::anchor::{Anchor, AnchorPoint};
use crate::GamePack;
use crate::ui::inventory::MenuState;
use crate::world::chunk::Chunk;
use crate::world::components::PlayerCamera;
use crate::world::resources::{CameraMode, CameraState, PhysicsState};

pub struct HudPlugin;

impl Plugin for HudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudState>()
            .add_systems(Startup, spawn_hud_root)
            .add_systems(
                Update,
                (
                    update_hotbar,
                    update_health_bar,
                    update_crosshair,
                    update_underwater_overlay,
                ),
            );
    }
}

#[derive(Resource)]
pub(crate) struct HudState {
    pub(crate) health: f32,
    pub(crate) max_health: f32,
    #[allow(dead_code)]
    hunger: f32,
    #[allow(dead_code)]
    max_hunger: f32,
    #[allow(dead_code)]
    pub(crate) selected_slot: usize,
}

impl Default for HudState {
    fn default() -> Self {
        Self {
            health: 20.0,
            max_health: 20.0,
            hunger: 20.0,
            max_hunger: 20.0,
            selected_slot: 0,
        }
    }
}

#[derive(Resource, Clone)]
struct HudTextures {
    crosshair: Option<Handle<Image>>,
    hotbar: Option<Handle<Image>>,
    hotbar_selection: Option<Handle<Image>>,
    heart_full: Option<Handle<Image>>,
    heart_half: Option<Handle<Image>>,
    heart_empty: Option<Handle<Image>>,
    grass: Option<Handle<Image>>,
    dirt: Option<Handle<Image>>,
    stone: Option<Handle<Image>>,
    sand: Option<Handle<Image>>,
    oak_log: Option<Handle<Image>>,
    oak_leaves: Option<Handle<Image>>,
    cobblestone: Option<Handle<Image>>,
    pickaxe: Option<Handle<Image>>,
    axe: Option<Handle<Image>>,
    sword: Option<Handle<Image>>,
}

#[derive(Component)]
struct HotbarSelection;

#[derive(Component)]
struct HotbarItemIcon {
    index: usize,
}

#[derive(Component)]
struct HeartIcon {
    index: usize,
}

#[derive(Component)]
struct Crosshair;

#[derive(Component)]
struct UnderwaterOverlay {
    alpha: f32,
}

fn pack_image(
    pack: &GamePack,
    asset_server: &AssetServer,
    relative_path: &str,
) -> Option<Handle<Image>> {
    pack.0
        .resolve_in(PackDirectory::Textures, relative_path)
        .map(|path| asset_server.load_override(path.to_string_lossy().to_string()))
}

pub(crate) fn image_rect(handle: Handle<Image>, x: f32, y: f32, w: f32, h: f32) -> ImageNode {
    ImageNode::new(handle).with_rect(Rect {
        min: Vec2::new(x, y),
        max: Vec2::new(x + w, y + h),
    })
}

fn px(logical_px: f32, gui_scale: f32) -> Val {
    Val::Px(logical_px * gui_scale)
}

fn spawn_hud_root(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    pack: Res<GamePack>,
    asset_server: Res<AssetServer>,
    windows: Query<&Window, With<PrimaryWindow>>,
) {
    // Compute GUI scale from actual window height (like Minecraft)
    let gs = windows
        .single()
        .map(|w| {
            (w.resolution.height().max(1.0) / 240.0)
                .floor()
                .clamp(1.0, 4.0)
        })
        .unwrap_or(2.0);
    let textures = HudTextures {
        crosshair: pack_image(&pack, &asset_server, "ui/hud/crosshair.png"),
        hotbar: pack_image(&pack, &asset_server, "ui/hud/hotbar.png"),
        hotbar_selection: pack_image(&pack, &asset_server, "ui/hud/hotbar_selection.png"),
        heart_full: pack_image(&pack, &asset_server, "ui/hud/heart/full.png"),
        heart_half: pack_image(&pack, &asset_server, "ui/hud/heart/half.png"),
        heart_empty: pack_image(&pack, &asset_server, "ui/hud/heart/container.png"),
        grass: Some(images.add(make_block_icon_image(
            &pack,
            shared::world::BlockKind::Grass,
        ))),
        dirt: Some(images.add(make_block_icon_image(&pack, shared::world::BlockKind::Dirt))),
        stone: Some(images.add(make_block_icon_image(
            &pack,
            shared::world::BlockKind::Stone,
        ))),
        sand: Some(images.add(make_block_icon_image(&pack, shared::world::BlockKind::Sand))),
        oak_log: Some(images.add(make_block_icon_image(
            &pack,
            shared::world::BlockKind::OakLog,
        ))),
        oak_leaves: Some(images.add(make_block_icon_image(
            &pack,
            shared::world::BlockKind::OakLeaves,
        ))),
        cobblestone: Some(images.add(make_block_icon_image(
            &pack,
            shared::world::BlockKind::Cobblestone,
        ))),
        pickaxe: pack_image(&pack, &asset_server, "items/beliung_kayu.png"),
        axe: pack_image(&pack, &asset_server, "items/wooden_axe.png")
            .or_else(|| pack_image(&pack, &asset_server, "items/beliung_kayu.png")),
        sword: pack_image(&pack, &asset_server, "items/wooden_sword.png")
            .or_else(|| pack_image(&pack, &asset_server, "items/beliung_kayu.png")),
    };

    commands.insert_resource(textures.clone());

    commands
        .spawn((Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Percent(100.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.08, 0.30, 0.58, 0.0)),
                Visibility::Hidden,
                UnderwaterOverlay { alpha: 0.0 },
            ));

            let mut crosshair = parent.spawn((
                Node {
                    width: px(15.0, gs),
                    height: px(15.0, gs),
                    ..default()
                },
                Anchor::new(AnchorPoint::Center),
                Crosshair,
            ));
            if let Some(image) = textures.crosshair.clone() {
                crosshair.insert(ImageNode::new(image));
            } else {
                crosshair.with_children(|p| {
                    p.spawn((
                        Text::new("+"),
                        TextFont {
                            font_size: 24.0,
                            ..default()
                        },
                        TextColor(Color::WHITE),
                    ));
                });
            }

            parent
                .spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        flex_direction: FlexDirection::Row,
                        column_gap: Val::Px(0.0),
                        width: px(90.0, gs),
                        height: px(9.0, gs),
                        ..default()
                    },
                    Anchor::new(AnchorPoint::BottomCenter)
                        .with_offset(Vec2::new(-46.0 * gs, -56.0 * gs)),
                ))
                .with_children(|bar| {
                    for i in 0..10 {
                        let mut heart = bar.spawn((
                            Node {
                                width: px(9.0, gs),
                                height: px(9.0, gs),
                                ..default()
                            },
                            HeartIcon { index: i },
                        ));
                        if let Some(image) = textures.heart_full.clone() {
                            heart.insert(ImageNode::new(image));
                        }
                    }
                });

            let mut hotbar = parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(182.0, gs),
                    height: px(22.0, gs),
                    ..default()
                },
                Anchor::new(AnchorPoint::BottomCenter).with_offset(Vec2::new(0.0, -22.0 * gs)),
            ));
            if let Some(image) = textures.hotbar.clone() {
                hotbar.insert(image_rect(image, 0.0, 0.0, 182.0, 22.0));
            } else {
                hotbar.insert(BackgroundColor(Color::NONE));
            }
            hotbar.with_children(|hotbar| {
                for i in 0..9 {
                    hotbar
                        .spawn((Node {
                            position_type: PositionType::Absolute,
                            left: px(3.0 + i as f32 * 20.0, gs),
                            top: px(3.0, gs),
                            width: px(16.0, gs),
                            height: px(16.0, gs),
                            justify_content: JustifyContent::Center,
                            align_items: AlignItems::Center,
                            ..default()
                        },))
                        .with_children(|slot| {
                            let mut icon = slot.spawn((
                                Node {
                                    width: px(14.0, gs),
                                    height: px(14.0, gs),
                                    ..default()
                                },
                                Visibility::Hidden,
                                HotbarItemIcon { index: i },
                            ));
                            if let Some(image) = textures.grass.clone() {
                                icon.insert(ImageNode::new(image));
                            }
                        });
                }

                let mut selector = hotbar.spawn((
                    Node {
                        position_type: PositionType::Absolute,
                        left: px(-1.0, gs),
                        top: px(-1.0, gs),
                        width: px(24.0, gs),
                        height: px(23.0, gs),
                        ..default()
                    },
                    HotbarSelection,
                ));
                if let Some(image) = textures.hotbar_selection.clone() {
                    selector.insert(image_rect(image, 0.0, 0.0, 24.0, 23.0));
                } else {
                    selector.insert((BorderColor::all(Color::WHITE), BackgroundColor(Color::NONE)));
                }
            });
        });
}

pub(crate) fn make_block_icon_image(pack: &GamePack, kind: shared::world::BlockKind) -> Image {
    const OUT: u32 = 32;
    let top = load_block_pixels(pack, block_texture_for(kind, "top"));
    let side = load_block_pixels(pack, block_texture_for(kind, "side"));
    let bottom = load_block_pixels(pack, block_texture_for(kind, "bottom"));
    let mut out = vec![0u8; (OUT * OUT * 4) as usize];

    draw_textured_quad(
        &mut out,
        OUT,
        &bottom,
        [[4.0, 8.0], [16.0, 14.0], [16.0, 28.0], [4.0, 22.0]],
        0.68,
    );
    draw_textured_quad(
        &mut out,
        OUT,
        &side,
        [[28.0, 8.0], [16.0, 14.0], [16.0, 28.0], [28.0, 22.0]],
        0.82,
    );
    draw_textured_quad(
        &mut out,
        OUT,
        &top,
        [[16.0, 2.0], [28.0, 8.0], [16.0, 14.0], [4.0, 8.0]],
        1.0,
    );

    Image::new(
        Extent3d {
            width: OUT,
            height: OUT,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        out,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn block_texture_for(kind: shared::world::BlockKind, face: &str) -> &'static str {
    match kind {
        shared::world::BlockKind::Grass if face == "top" => "blocks/grass_block_top.png",
        shared::world::BlockKind::Grass if face == "bottom" => "blocks/dirt.png",
        shared::world::BlockKind::Grass => "blocks/grass_block_side.png",
        shared::world::BlockKind::Dirt => "blocks/dirt.png",
        shared::world::BlockKind::Stone => "blocks/stone.png",
        shared::world::BlockKind::Sand => "blocks/sand.png",
        shared::world::BlockKind::Water => "blocks/water_still.png",
        shared::world::BlockKind::OakLog if face == "top" || face == "bottom" => {
            "blocks/oak_log_top.png"
        }
        shared::world::BlockKind::OakLog => "blocks/oak_log.png",
        shared::world::BlockKind::OakLeaves => "blocks/oak_leaves.png",
        shared::world::BlockKind::Cobblestone => "blocks/cobblestone.png",
        shared::world::BlockKind::Air => "blocks/stone.png",
    }
}

fn load_block_pixels(pack: &GamePack, relative_path: &str) -> Vec<[u8; 4]> {
    let fallback = match relative_path {
        "blocks/dirt.png" => [128, 90, 50, 255],
        "blocks/grass_block_top.png" => [80, 170, 60, 255],
        "blocks/grass_block_side.png" => [95, 140, 55, 255],
        "blocks/sand.png" => [210, 194, 125, 255],
        "blocks/water_still.png" => [70, 120, 220, 180],
        "blocks/oak_log.png" => [112, 72, 36, 255],
        "blocks/oak_log_top.png" => [158, 124, 78, 255],
        "blocks/oak_leaves.png" => [60, 135, 48, 220],
        "blocks/cobblestone.png" => [100, 100, 100, 255],
        _ => [110, 110, 110, 255],
    };
    let pixels = pack
        .0
        .resolve_in(PackDirectory::Textures, relative_path)
        .and_then(|path| image::open(path).ok())
        .map(|img| {
            img.resize_exact(16, 16, image::imageops::FilterType::Nearest)
                .to_rgba8()
        });

    let mut result = Vec::with_capacity(16 * 16);
    for y in 0..16 {
        for x in 0..16 {
            let rgba = pixels
                .as_ref()
                .map(|img| img.get_pixel(x, y).0)
                .unwrap_or(fallback);
            result.push(rgba);
        }
    }
    result
}

fn draw_textured_quad(
    out: &mut [u8],
    size: u32,
    src: &[[u8; 4]],
    points: [[f32; 2]; 4],
    shade: f32,
) {
    draw_textured_tri(
        out,
        size,
        src,
        [points[0], points[1], points[2]],
        [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
        shade,
    );
    draw_textured_tri(
        out,
        size,
        src,
        [points[0], points[2], points[3]],
        [[0.0, 0.0], [1.0, 1.0], [0.0, 1.0]],
        shade,
    );
}

fn draw_textured_tri(
    out: &mut [u8],
    size: u32,
    src: &[[u8; 4]],
    p: [[f32; 2]; 3],
    uv: [[f32; 2]; 3],
    shade: f32,
) {
    let min_x = p
        .iter()
        .map(|v| v[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as u32;
    let max_x = p
        .iter()
        .map(|v| v[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(size as f32 - 1.0) as u32;
    let min_y = p
        .iter()
        .map(|v| v[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .max(0.0) as u32;
    let max_y = p
        .iter()
        .map(|v| v[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .min(size as f32 - 1.0) as u32;
    let area = edge(p[0], p[1], p[2]);
    if area.abs() < 0.0001 {
        return;
    }

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(p[1], p[2], point) / area;
            let w1 = edge(p[2], p[0], point) / area;
            let w2 = edge(p[0], p[1], point) / area;
            if w0 < -0.001 || w1 < -0.001 || w2 < -0.001 {
                continue;
            }
            let u = (uv[0][0] * w0 + uv[1][0] * w1 + uv[2][0] * w2).clamp(0.0, 1.0);
            let v = (uv[0][1] * w0 + uv[1][1] * w1 + uv[2][1] * w2).clamp(0.0, 1.0);
            let sx = (u * 15.0).round() as usize;
            let sy = (v * 15.0).round() as usize;
            let src_px = src[sy * 16 + sx];
            let dst = ((y * size + x) * 4) as usize;
            out[dst] = (src_px[0] as f32 * shade).min(255.0) as u8;
            out[dst + 1] = (src_px[1] as f32 * shade).min(255.0) as u8;
            out[dst + 2] = (src_px[2] as f32 * shade).min(255.0) as u8;
            out[dst + 3] = src_px[3];
        }
    }
}

fn edge(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])
}

fn update_hotbar(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut physics: ResMut<PhysicsState>,
    textures: Res<HudTextures>,
    gui_scale: Res<GuiScale>,
    mut selector: Query<&mut Node, With<HotbarSelection>>,
    mut icons: Query<(&mut ImageNode, &mut Visibility, &HotbarItemIcon)>,
) {
    for (key, index) in [
        (KeyCode::Digit1, 0),
        (KeyCode::Digit2, 1),
        (KeyCode::Digit3, 2),
        (KeyCode::Digit4, 3),
        (KeyCode::Digit5, 4),
        (KeyCode::Digit6, 5),
        (KeyCode::Digit7, 6),
        (KeyCode::Digit8, 7),
        (KeyCode::Digit9, 8),
    ] {
        if keyboard.just_pressed(key) {
            physics.hotbar_slot = index;
        }
    }

    for mut node in &mut selector {
        node.left = px(-1.0 + physics.hotbar_slot as f32 * 20.0, gui_scale.0);
        node.top = px(-1.0, gui_scale.0);
        node.width = px(24.0, gui_scale.0);
        node.height = px(23.0, gui_scale.0);
    }

    for (mut image, mut visibility, icon) in &mut icons {
        let handle = match physics.hotbar_items[icon.index] {
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::Grass)) => {
                textures.grass.clone()
            }
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::Dirt)) => {
                textures.dirt.clone()
            }
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::Stone)) => {
                textures.stone.clone()
            }
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::Sand)) => {
                textures.sand.clone()
            }
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::OakLog)) => {
                textures.oak_log.clone()
            }
            Some(crate::world::resources::ItemKind::Block(shared::world::BlockKind::OakLeaves)) => {
                textures.oak_leaves.clone()
            }
            Some(crate::world::resources::ItemKind::Block(
                shared::world::BlockKind::Cobblestone,
            )) => textures.cobblestone.clone(),
            Some(crate::world::resources::ItemKind::Block(_)) => None,
            Some(crate::world::resources::ItemKind::Tool(
                crate::world::resources::ToolKind::WoodenPickaxe,
            )) => textures.pickaxe.clone(),
            Some(crate::world::resources::ItemKind::Tool(
                crate::world::resources::ToolKind::WoodenAxe,
            )) => textures.axe.clone(),
            Some(crate::world::resources::ItemKind::Tool(
                crate::world::resources::ToolKind::WoodenSword,
            )) => textures.sword.clone(),
            None => None,
        };

        if let Some(handle) = handle {
            image.image = handle;
            *visibility = Visibility::Visible;
        } else {
            *visibility = Visibility::Hidden;
        }
    }
}

fn update_health_bar(
    hud: Res<HudState>,
    textures: Res<HudTextures>,
    mut hearts: Query<(&mut ImageNode, &HeartIcon)>,
) {
    let health = hud.health.clamp(0.0, hud.max_health);
    for (mut image, heart) in &mut hearts {
        let value = health - heart.index as f32 * 2.0;
        let handle = if value >= 2.0 {
            textures.heart_full.clone()
        } else if value >= 1.0 {
            textures.heart_half.clone()
        } else {
            textures.heart_empty.clone()
        };
        if let Some(handle) = handle {
            image.image = handle;
        }
    }
}

fn update_crosshair(
    cam_state: Res<CameraState>,
    menu_state: Res<State<MenuState>>,
    mut crosshair: Query<&mut Visibility, With<Crosshair>>,
) {
    let visible =
        cam_state.mode == CameraMode::FirstPerson && *menu_state.get() == MenuState::Playing;
    for mut visibility in &mut crosshair {
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn update_underwater_overlay(
    time: Res<Time>,
    chunks: Query<&Chunk>,
    camera: Query<&Transform, With<PlayerCamera>>,
    mut overlay: Query<(
        &mut BackgroundColor,
        &mut Visibility,
        &mut UnderwaterOverlay,
    )>,
) {
    let Ok(camera_t) = camera.single() else {
        return;
    };
    let underwater = chunks
        .iter()
        .any(|chunk| chunk.block_at_world(camera_t.translation) == shared::world::BlockKind::Water);

    for (mut color, mut visibility, mut state) in &mut overlay {
        let target = if underwater { 0.34 } else { 0.0 };
        let speed = if underwater { 6.0 } else { 4.0 };
        state.alpha += (target - state.alpha) * (1.0 - (-speed * time.delta_secs()).exp());
        let wave = if underwater {
            (time.elapsed_secs() * 2.1).sin() * 0.025
        } else {
            0.0
        };
        let alpha = (state.alpha + wave).clamp(0.0, 0.40);
        *visibility = if alpha > 0.01 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        color.0 = Color::srgba(0.08, 0.30, 0.58, alpha);
    }
}
