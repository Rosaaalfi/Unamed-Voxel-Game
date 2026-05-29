//! Inventory and pause menu overlays.

use bevy::app::AppExit;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use shared::pack::PackDirectory;

use crate::GamePack;
use crate::ui::GuiScale;
use crate::world::resources::{
    ChunkBuilderMode, GameSettings, GraphicsQuality, ItemKind, PhysicsState, ToolKind,
};

pub struct InventoryPlugin;

impl Plugin for InventoryPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<MenuState>()
            .init_resource::<InventoryCursor>()
            .add_systems(OnEnter(MenuState::Inventory), spawn_inventory_ui)
            .add_systems(OnExit(MenuState::Inventory), despawn_inventory_ui)
            .add_systems(OnEnter(MenuState::Pause), spawn_pause_ui)
            .add_systems(OnExit(MenuState::Pause), despawn_pause_ui)
            .add_systems(OnEnter(MenuState::Settings), spawn_settings_ui)
            .add_systems(OnExit(MenuState::Settings), despawn_pause_ui)
            .add_systems(OnEnter(MenuState::VideoSettings), spawn_video_settings_ui)
            .add_systems(OnExit(MenuState::VideoSettings), despawn_pause_ui)
            .add_systems(
                Update,
                toggle_inventory.run_if(in_state(MenuState::Playing)),
            )
            .add_systems(Update, open_pause_menu.run_if(in_state(MenuState::Playing)))
            .add_systems(
                Update,
                handle_inventory_close.run_if(in_state(MenuState::Inventory)),
            )
            .add_systems(
                Update,
                (
                    handle_inventory_slot_interactions,
                    update_inventory_hotbar_ui,
                )
                    .run_if(in_state(MenuState::Inventory)),
            )
            .add_systems(
                Update,
                handle_pause_buttons.run_if(in_state(MenuState::Pause)),
            )
            .add_systems(
                Update,
                handle_settings_buttons.run_if(in_state(MenuState::Settings)),
            )
            .add_systems(
                Update,
                handle_video_settings_buttons.run_if(in_state(MenuState::VideoSettings)),
            );
    }
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States)]
pub(crate) enum MenuState {
    #[default]
    Playing,
    Inventory,
    Pause,
    Settings,
    VideoSettings,
}

#[derive(Component)]
struct InventoryRoot;

#[derive(Component, Clone, Copy)]
enum InventorySlotKind {
    Main(usize),
    Hotbar(usize),
}

#[derive(Component)]
struct InventorySlotButton {
    kind: InventorySlotKind,
}

#[derive(Component)]
struct InventorySlotIcon {
    kind: InventorySlotKind,
}

#[derive(Component)]
struct InventoryCarryIcon;

#[derive(Component)]
struct PauseRoot;

#[derive(Component, Clone, Copy)]
enum PauseButtonAction {
    BackToGame,
    Settings,
    InviteFriend,
    ExitGame,
}

#[derive(Component, Clone, Copy)]
enum SettingsButtonAction {
    Fov,
    SkinCustomization,
    VideoSettings,
    Language,
    ResourcePacks,
    FriendsOnline,
    MusicSounds,
    Controls,
    ChatSettings,
    Accessibility,
    Done,
}

#[derive(Component, Clone, Copy)]
enum VideoButtonAction {
    RenderDistance,
    Graphics,
    Vsync,
    Fullscreen,
    FullscreenResolution,
    ChunkBuilder,
    MaxFramerate,
    SmoothLighting,
    GuiScale,
    ViewBobbing,
    FirstPersonRightTranslation,
    FirstPersonRightScale,
    FirstPersonRightRotation,
    FirstPersonRightMatrix,
    FirstPersonLeftTranslation,
    FirstPersonLeftScale,
    FirstPersonLeftRotation,
    FirstPersonLeftMatrix,
    Done,
}

#[derive(Resource, Default)]
struct InventoryCursor {
    carried: Option<ItemKind>,
}

#[derive(Resource, Clone)]
struct InventoryIconTextures {
    grass: Handle<Image>,
    dirt: Handle<Image>,
    stone: Handle<Image>,
    sand: Handle<Image>,
    oak_log: Handle<Image>,
    oak_leaves: Handle<Image>,
    cobblestone: Handle<Image>,
    pickaxe: Option<Handle<Image>>,
    axe: Option<Handle<Image>>,
    sword: Option<Handle<Image>>,
}

fn toggle_inventory(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyE) {
        next_state.set(MenuState::Inventory);
    }
}

fn open_pause_menu(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(MenuState::Pause);
    }
}

fn menu_button_style(gs: f32) -> Node {
    Node {
        width: px(200.0, gs),
        height: px(20.0, gs),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    }
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

fn image_rect(handle: Handle<Image>, x: f32, y: f32, w: f32, h: f32) -> ImageNode {
    ImageNode::new(handle).with_rect(Rect {
        min: Vec2::new(x, y),
        max: Vec2::new(x + w, y + h),
    })
}

fn px(logical_px: f32, gui_scale: f32) -> Val {
    Val::Px(logical_px * gui_scale)
}

fn source_px(logical_px: f32) -> f32 {
    logical_px
}

fn spawn_inventory_ui(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    pack: Res<GamePack>,
    asset_server: Res<AssetServer>,
    _physics: Res<PhysicsState>,
    gui_scale: Res<GuiScale>,
) {
    let gs = gui_scale.0;
    let panel_image = pack_image(&pack, &asset_server, "ui/gui/container/inventory.png");
    let pickaxe_image = pack_image(&pack, &asset_server, "items/beliung_kayu.png");
    let icon_textures = InventoryIconTextures {
        grass: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::Grass,
        )),
        dirt: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::Dirt,
        )),
        stone: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::Stone,
        )),
        sand: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::Sand,
        )),
        oak_log: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::OakLog,
        )),
        oak_leaves: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::OakLeaves,
        )),
        cobblestone: images.add(crate::ui::hud::make_block_icon_image(
            &pack,
            shared::world::BlockKind::Cobblestone,
        )),
        pickaxe: pickaxe_image.clone(),
        axe: pack_image(&pack, &asset_server, "items/wooden_axe.png")
            .or_else(|| pickaxe_image.clone()),
        sword: pack_image(&pack, &asset_server, "items/wooden_sword.png").or(pickaxe_image),
    };
    commands.insert_resource(icon_textures.clone());

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.04, 0.05, 0.08, 0.58)),
            InventoryRoot,
        ))
        .with_children(|parent| {
            let mut panel = parent.spawn((
                Node {
                    position_type: PositionType::Relative,
                    width: px(176.0, gs),
                    height: px(166.0, gs),
                    ..default()
                },
                BackgroundColor(Color::NONE),
            ));
            if let Some(image) = panel_image {
                panel.insert(crate::ui::hud::image_rect(
                    image,
                    0.0,
                    0.0,
                    source_px(176.0),
                    source_px(166.0),
                ));
            }
            panel.with_children(|panel| {
                for slot in 0..27 {
                    let col = slot % 9;
                    let row = slot / 9;
                    spawn_inventory_slot(
                        panel,
                        InventorySlotKind::Main(slot),
                        7.0 + col as f32 * 18.0,
                        88.0 + row as f32 * 18.0,
                        &icon_textures,
                        gs,
                    );
                }
                for slot in 0..9 {
                    spawn_inventory_slot(
                        panel,
                        InventorySlotKind::Hotbar(slot),
                        7.0 + slot as f32 * 18.0,
                        142.0,
                        &icon_textures,
                        gs,
                    );
                }
            });
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    width: px(12.0, gs),
                    height: px(12.0, gs),
                    ..default()
                },
                ImageNode::new(icon_textures.grass.clone()),
                Visibility::Hidden,
                InventoryCarryIcon,
            ));
        });
}

fn spawn_inventory_slot(
    parent: &mut ChildSpawnerCommands,
    kind: InventorySlotKind,
    left: f32,
    top: f32,
    textures: &InventoryIconTextures,
    gui_scale: f32,
) {
    parent
        .spawn((
            Button,
            Node {
                position_type: PositionType::Absolute,
                left: px(left, gui_scale),
                top: px(top, gui_scale),
                width: px(16.0, gui_scale),
                height: px(16.0, gui_scale),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            BackgroundColor(Color::NONE),
            InventorySlotButton { kind },
        ))
        .with_children(|slot_node| {
            slot_node.spawn((
                Node {
                    width: px(12.0, gui_scale),
                    height: px(12.0, gui_scale),
                    ..default()
                },
                ImageNode::new(textures.grass.clone()),
                InventorySlotIcon { kind },
            ));
        });
}

fn despawn_inventory_ui(mut commands: Commands, root: Query<Entity, With<InventoryRoot>>) {
    for entity in &root {
        commands.entity(entity).despawn();
    }
}

fn item_icon_handle(item: ItemKind, textures: &InventoryIconTextures) -> Option<Handle<Image>> {
    match item {
        ItemKind::Block(shared::world::BlockKind::Grass) => Some(textures.grass.clone()),
        ItemKind::Block(shared::world::BlockKind::Dirt) => Some(textures.dirt.clone()),
        ItemKind::Block(shared::world::BlockKind::Stone) => Some(textures.stone.clone()),
        ItemKind::Block(shared::world::BlockKind::Sand) => Some(textures.sand.clone()),
        ItemKind::Block(shared::world::BlockKind::OakLog) => Some(textures.oak_log.clone()),
        ItemKind::Block(shared::world::BlockKind::OakLeaves) => Some(textures.oak_leaves.clone()),
        ItemKind::Block(shared::world::BlockKind::Cobblestone) => {
            Some(textures.cobblestone.clone())
        }
        ItemKind::Block(shared::world::BlockKind::Water | shared::world::BlockKind::Air) => None,
        ItemKind::Tool(ToolKind::WoodenPickaxe) => textures.pickaxe.clone(),
        ItemKind::Tool(ToolKind::WoodenAxe) => textures.axe.clone(),
        ItemKind::Tool(ToolKind::WoodenSword) => textures.sword.clone(),
    }
}

fn update_inventory_hotbar_ui(
    windows: Query<&Window, With<PrimaryWindow>>,
    physics: Res<PhysicsState>,
    cursor: Res<InventoryCursor>,
    textures: Res<InventoryIconTextures>,
    mut slot_icons: Query<
        (&InventorySlotIcon, &mut ImageNode, &mut Visibility),
        Without<InventoryCarryIcon>,
    >,
    mut carry_icons: Query<(&mut Node, &mut ImageNode, &mut Visibility), With<InventoryCarryIcon>>,
) {
    for (slot_icon, mut image, mut visibility) in &mut slot_icons {
        let item = match slot_icon.kind {
            InventorySlotKind::Main(index) => physics.inventory_items[index],
            InventorySlotKind::Hotbar(index) => physics.hotbar_items[index],
        };
        if let Some(kind) = item {
            if let Some(handle) = item_icon_handle(kind, &textures) {
                image.image = handle;
                *visibility = Visibility::Visible;
            } else {
                *visibility = Visibility::Hidden;
            }
        } else {
            *visibility = Visibility::Hidden;
        }
    }

    let Ok((mut node, mut image, mut visibility)) = carry_icons.single_mut() else {
        return;
    };
    if let Some(kind) = cursor.carried {
        if let Some(handle) = item_icon_handle(kind, &textures) {
            image.image = handle;
            *visibility = Visibility::Visible;
        }
        if let Ok(window) = windows.single() {
            if let Some(pos) = window.cursor_position() {
                node.left = Val::Px(pos.x + 8.0);
                node.top = Val::Px(pos.y + 8.0);
            }
        }
    } else {
        *visibility = Visibility::Hidden;
    }
}

fn handle_inventory_slot_interactions(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut cursor: ResMut<InventoryCursor>,
    mut physics: ResMut<PhysicsState>,
    mut slots: Query<(&Interaction, &InventorySlotButton), (Changed<Interaction>, With<Button>)>,
) {
    if keyboard.just_pressed(KeyCode::KeyQ) {
        cursor.carried = None;
    }

    for (interaction, slot) in &mut slots {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let held = cursor.carried.take();
        let slot_item = match slot.kind {
            InventorySlotKind::Main(index) => {
                let slot_item = physics.inventory_items[index];
                physics.inventory_items[index] = held;
                slot_item
            }
            InventorySlotKind::Hotbar(index) => {
                let slot_item = physics.hotbar_items[index];
                physics.hotbar_items[index] = held;
                physics.hotbar_slot = index;
                slot_item
            }
        };
        cursor.carried = slot_item;
    }
}

fn handle_inventory_close(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
) {
    if keyboard.just_pressed(KeyCode::KeyE) {
        next_state.set(MenuState::Playing);
    }
}

fn spawn_pause_ui(
    mut commands: Commands,
    pack: Res<GamePack>,
    asset_server: Res<AssetServer>,
    gui_scale: Res<GuiScale>,
) {
    let gs = gui_scale.0;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(4.0, gs),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.46)),
            PauseRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Game Menu"),
                TextFont {
                    font_size: 14.0 * gs,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            root.spawn((Node {
                width: Val::Px(1.0),
                height: px(4.0, gs),
                ..default()
            },));
            pause_button(
                root,
                &pack,
                &asset_server,
                "Back to Game",
                PauseButtonAction::BackToGame,
                gs,
            );
            pause_button(
                root,
                &pack,
                &asset_server,
                "Settings",
                PauseButtonAction::Settings,
                gs,
            );
            pause_button(
                root,
                &pack,
                &asset_server,
                "Invite Friend",
                PauseButtonAction::InviteFriend,
                gs,
            );
            pause_button(
                root,
                &pack,
                &asset_server,
                "Exit Game",
                PauseButtonAction::ExitGame,
                gs,
            );
        });
}

fn pause_button(
    parent: &mut ChildSpawnerCommands,
    pack: &GamePack,
    asset_server: &AssetServer,
    label: &str,
    action: PauseButtonAction,
    gs: f32,
) {
    let button_image = pack_image(pack, asset_server, "ui/gui/sprites/widget/button.png");
    let mut button = parent.spawn((
        Button,
        menu_button_style(gs),
        BackgroundColor(Color::NONE),
        action,
    ));
    if let Some(image) = button_image {
        button.insert(image_rect(image, 0.0, 0.0, 200.0, 20.0));
    }
    button.with_children(|button| {
        button.spawn((
            Text::new(label),
            TextFont {
                font_size: 10.0 * gs,
                ..default()
            },
            TextColor(Color::WHITE),
        ));
    });
}

fn despawn_pause_ui(mut commands: Commands, root: Query<Entity, With<PauseRoot>>) {
    for entity in &root {
        commands.entity(entity).despawn();
    }
}

fn spawn_settings_ui(mut commands: Commands, gui_scale: Res<GuiScale>) {
    let gs = gui_scale.0;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(6.0, gs),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.46)),
            PauseRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Settings"),
                TextFont {
                    font_size: 14.0 * gs,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            root.spawn((Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(8.0, gs),
                ..default()
            },))
                .with_children(|cols| {
                    cols.spawn((Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4.0, gs),
                        ..default()
                    },))
                        .with_children(|left| {
                            settings_button(left, "FOV", SettingsButtonAction::Fov, gs);
                            settings_button(
                                left,
                                "Skin Customization",
                                SettingsButtonAction::SkinCustomization,
                                gs,
                            );
                            settings_button(
                                left,
                                "Video Settings",
                                SettingsButtonAction::VideoSettings,
                                gs,
                            );
                            settings_button(left, "Language", SettingsButtonAction::Language, gs);
                            settings_button(
                                left,
                                "Resource Packs",
                                SettingsButtonAction::ResourcePacks,
                                gs,
                            );
                        });
                    cols.spawn((Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4.0, gs),
                        ..default()
                    },))
                        .with_children(|right| {
                            settings_button(
                                right,
                                "Friends & Online",
                                SettingsButtonAction::FriendsOnline,
                                gs,
                            );
                            settings_button(
                                right,
                                "Music & Sounds",
                                SettingsButtonAction::MusicSounds,
                                gs,
                            );
                            settings_button(right, "Controls", SettingsButtonAction::Controls, gs);
                            settings_button(
                                right,
                                "Chat Settings",
                                SettingsButtonAction::ChatSettings,
                                gs,
                            );
                            settings_button(
                                right,
                                "Accessibility",
                                SettingsButtonAction::Accessibility,
                                gs,
                            );
                        });
                });
            settings_button(root, "Done", SettingsButtonAction::Done, gs);
        });
}

fn settings_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: SettingsButtonAction,
    gs: f32,
) {
    parent
        .spawn((
            Button,
            menu_button_style(gs),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            action,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 10.0 * gs,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn spawn_video_settings_ui(
    mut commands: Commands,
    gui_scale: Res<GuiScale>,
    settings: Res<GameSettings>,
) {
    let gs = gui_scale.0;
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                flex_direction: FlexDirection::Column,
                row_gap: px(5.0, gs),
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.50)),
            PauseRoot,
        ))
        .with_children(|root| {
            root.spawn((
                Text::new("Video Settings"),
                TextFont {
                    font_size: 14.0 * gs,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
            root.spawn((Node {
                flex_direction: FlexDirection::Row,
                column_gap: px(8.0, gs),
                ..default()
            },))
                .with_children(|cols| {
                    cols.spawn((Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4.0, gs),
                        ..default()
                    },))
                        .with_children(|left| {
                            video_button(
                                left,
                                &format!("Render Distance: {}", settings.render_distance),
                                VideoButtonAction::RenderDistance,
                                gs,
                            );
                            video_button(
                                left,
                                &format!("Graphics: {:?}", settings.graphics_quality),
                                VideoButtonAction::Graphics,
                                gs,
                            );
                            video_button(
                                left,
                                bool_label("VSync", settings.vsync),
                                VideoButtonAction::Vsync,
                                gs,
                            );
                            video_button(
                                left,
                                if settings.fullscreen {
                                    "Fullscreen"
                                } else {
                                    "Windowed"
                                },
                                VideoButtonAction::Fullscreen,
                                gs,
                            );
                            video_button(
                                left,
                                &format!(
                                    "Fullscreen Resolution: {}x{}",
                                    settings.fullscreen_resolution.0,
                                    settings.fullscreen_resolution.1
                                ),
                                VideoButtonAction::FullscreenResolution,
                                gs,
                            );
                            video_button(
                                left,
                                &format!("Chunk Builder: {:?}", settings.chunk_builder),
                                VideoButtonAction::ChunkBuilder,
                                gs,
                            );
                            video_button(
                                left,
                                &format!("Max Framerate: {}", settings.max_framerate),
                                VideoButtonAction::MaxFramerate,
                                gs,
                            );
                            video_button(
                                left,
                                bool_label("Smooth Lighting", settings.smooth_lighting),
                                VideoButtonAction::SmoothLighting,
                                gs,
                            );
                            video_button(
                                left,
                                &format!("GUI Scale: {:.0}", settings.gui_scale),
                                VideoButtonAction::GuiScale,
                                gs,
                            );
                            video_button(
                                left,
                                bool_label("View Bobbing", settings.view_bobbing),
                                VideoButtonAction::ViewBobbing,
                                gs,
                            );
                        });
                    cols.spawn((Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(4.0, gs),
                        ..default()
                    },))
                        .with_children(|right| {
                            video_button(
                                right,
                                "Right Hand Translation",
                                VideoButtonAction::FirstPersonRightTranslation,
                                gs,
                            );
                            video_button(
                                right,
                                "Right Hand Scale",
                                VideoButtonAction::FirstPersonRightScale,
                                gs,
                            );
                            video_button(
                                right,
                                "Right Hand Rotation",
                                VideoButtonAction::FirstPersonRightRotation,
                                gs,
                            );
                            video_button(
                                right,
                                "Right Hand Matrix",
                                VideoButtonAction::FirstPersonRightMatrix,
                                gs,
                            );
                            video_button(
                                right,
                                "Left Hand Translation",
                                VideoButtonAction::FirstPersonLeftTranslation,
                                gs,
                            );
                            video_button(
                                right,
                                "Left Hand Scale",
                                VideoButtonAction::FirstPersonLeftScale,
                                gs,
                            );
                            video_button(
                                right,
                                "Left Hand Rotation",
                                VideoButtonAction::FirstPersonLeftRotation,
                                gs,
                            );
                            video_button(
                                right,
                                "Left Hand Matrix",
                                VideoButtonAction::FirstPersonLeftMatrix,
                                gs,
                            );
                        });
                });
            video_button(root, "Done", VideoButtonAction::Done, gs);
        });
}

fn bool_label(name: &'static str, value: bool) -> &'static str {
    if value {
        match name {
            "VSync" => "VSync: ON",
            "Smooth Lighting" => "Smooth Lighting: ON",
            "View Bobbing" => "View Bobbing: ON",
            _ => "ON",
        }
    } else {
        match name {
            "VSync" => "VSync: OFF",
            "Smooth Lighting" => "Smooth Lighting: OFF",
            "View Bobbing" => "View Bobbing: OFF",
            _ => "OFF",
        }
    }
}

fn video_button(
    parent: &mut ChildSpawnerCommands,
    label: &str,
    action: VideoButtonAction,
    gs: f32,
) {
    parent
        .spawn((
            Button,
            menu_button_style(gs),
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.55)),
            action,
        ))
        .with_children(|button| {
            button.spawn((
                Text::new(label),
                TextFont {
                    font_size: 9.0 * gs,
                    ..default()
                },
                TextColor(Color::WHITE),
            ));
        });
}

fn handle_pause_buttons(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
    mut exit: MessageWriter<AppExit>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &PauseButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(MenuState::Playing);
    }

    for (interaction, mut color, action) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                match action {
                    PauseButtonAction::BackToGame => next_state.set(MenuState::Playing),
                    PauseButtonAction::Settings => next_state.set(MenuState::Settings),
                    PauseButtonAction::InviteFriend => info!("invite friend requested"),
                    PauseButtonAction::ExitGame => {
                        exit.write(AppExit::Success);
                    }
                }
                color.0 = Color::srgba(0.65, 0.65, 0.65, 0.18);
            }
            Interaction::Hovered => {
                color.0 = Color::srgba(1.0, 1.0, 1.0, 0.12);
            }
            Interaction::None => {
                color.0 = Color::NONE;
            }
        }
    }
}

fn handle_settings_buttons(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &SettingsButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(MenuState::Pause);
    }
    for (interaction, mut color, action) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                match action {
                    SettingsButtonAction::VideoSettings => next_state.set(MenuState::VideoSettings),
                    SettingsButtonAction::Done => next_state.set(MenuState::Pause),
                    _ => info!("settings option selected"),
                }
                color.0 = Color::srgba(0.65, 0.65, 0.65, 0.18);
            }
            Interaction::Hovered => color.0 = Color::srgba(1.0, 1.0, 1.0, 0.12),
            Interaction::None => color.0 = Color::srgba(0.0, 0.0, 0.0, 0.55),
        }
    }
}

fn handle_video_settings_buttons(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut next_state: ResMut<NextState<MenuState>>,
    mut settings: ResMut<GameSettings>,
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &VideoButtonAction),
        (Changed<Interaction>, With<Button>),
    >,
) {
    if keyboard.just_pressed(KeyCode::Escape) {
        next_state.set(MenuState::Settings);
    }
    for (interaction, mut color, action) in &mut buttons {
        match *interaction {
            Interaction::Pressed => {
                match action {
                    VideoButtonAction::RenderDistance => {
                        settings.render_distance += 1;
                        if settings.render_distance > 16 {
                            settings.render_distance = 4;
                        }
                    }
                    VideoButtonAction::Graphics => {
                        settings.graphics_quality = match settings.graphics_quality {
                            GraphicsQuality::Low => GraphicsQuality::Medium,
                            GraphicsQuality::Medium => GraphicsQuality::Fancy,
                            GraphicsQuality::Fancy => GraphicsQuality::Fabulous,
                            GraphicsQuality::Fabulous => GraphicsQuality::Low,
                        };
                    }
                    VideoButtonAction::Vsync => settings.vsync = !settings.vsync,
                    VideoButtonAction::Fullscreen => settings.fullscreen = !settings.fullscreen,
                    VideoButtonAction::FullscreenResolution => {
                        settings.fullscreen_resolution = match settings.fullscreen_resolution {
                            (1280, 720) => (1600, 900),
                            (1600, 900) => (1920, 1080),
                            (1920, 1080) => (2560, 1440),
                            _ => (1280, 720),
                        };
                    }
                    VideoButtonAction::ChunkBuilder => {
                        settings.chunk_builder = match settings.chunk_builder {
                            ChunkBuilderMode::Threaded => ChunkBuilderMode::Single,
                            ChunkBuilderMode::Single => ChunkBuilderMode::Threaded,
                        };
                    }
                    VideoButtonAction::MaxFramerate => {
                        settings.max_framerate = match settings.max_framerate {
                            30 => 60,
                            60 => 120,
                            120 => 240,
                            _ => 30,
                        };
                    }
                    VideoButtonAction::SmoothLighting => {
                        settings.smooth_lighting = !settings.smooth_lighting
                    }
                    VideoButtonAction::GuiScale => {
                        settings.gui_scale += 1.0;
                        if settings.gui_scale > 4.0 {
                            settings.gui_scale = 1.0;
                        }
                    }
                    VideoButtonAction::ViewBobbing => {
                        settings.view_bobbing = !settings.view_bobbing
                    }
                    VideoButtonAction::FirstPersonRightTranslation => {
                        settings.first_person_right.translation.x += 0.02;
                    }
                    VideoButtonAction::FirstPersonRightScale => {
                        settings.first_person_right.scale *= 1.05;
                    }
                    VideoButtonAction::FirstPersonRightRotation => {
                        settings.first_person_right.left_rotation *=
                            Quat::from_rotation_y(5.0_f32.to_radians());
                    }
                    VideoButtonAction::FirstPersonRightMatrix => {
                        settings.first_person_right.matrix =
                            Mat4::from_translation(settings.first_person_right.translation);
                    }
                    VideoButtonAction::FirstPersonLeftTranslation => {
                        settings.first_person_left.translation.x -= 0.02;
                    }
                    VideoButtonAction::FirstPersonLeftScale => {
                        settings.first_person_left.scale *= 1.05;
                    }
                    VideoButtonAction::FirstPersonLeftRotation => {
                        settings.first_person_left.left_rotation *=
                            Quat::from_rotation_y(-5.0_f32.to_radians());
                    }
                    VideoButtonAction::FirstPersonLeftMatrix => {
                        settings.first_person_left.matrix =
                            Mat4::from_translation(settings.first_person_left.translation);
                    }
                    VideoButtonAction::Done => next_state.set(MenuState::Settings),
                }
                color.0 = Color::srgba(0.65, 0.65, 0.65, 0.18);
            }
            Interaction::Hovered => color.0 = Color::srgba(1.0, 1.0, 1.0, 0.12),
            Interaction::None => color.0 = Color::srgba(0.0, 0.0, 0.0, 0.55),
        }
    }
}
