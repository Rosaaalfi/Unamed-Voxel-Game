//! UI plugin for the desktop client — HUD, inventory, main menu.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::world::resources::GameSettings;

mod anchor;
pub(crate) mod hud;
pub(crate) mod inventory;

/// Dynamic GUI scale factor based on logical window height.
/// Uses Minecraft-style formula: floor(window_height / 240), clamped to [1, 4].
#[derive(Resource, Clone, Copy, Debug)]
pub struct GuiScale(pub f32);

impl Default for GuiScale {
    fn default() -> Self {
        Self(2.0) // default fallback
    }
}

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<GuiScale>()
            .add_plugins(hud::HudPlugin)
            .add_plugins(inventory::InventoryPlugin)
            .add_systems(PostUpdate, anchor::apply_anchor_positions)
            .add_systems(Update, update_gui_scale_system);
    }
}

/// Update GuiScale based on actual window height.
fn update_gui_scale_system(
    windows: Query<&Window, With<PrimaryWindow>>,
    settings: Option<Res<GameSettings>>,
    mut gui_scale: ResMut<GuiScale>,
) {
    if let Some(settings) = settings {
        gui_scale.0 = settings.gui_scale.clamp(1.0, 4.0);
        return;
    }
    let Ok(window) = windows.single() else { return };
    let height = window.resolution.height().max(1.0);
    let scale = (height / 240.0).floor().clamp(1.0, 4.0);
    gui_scale.0 = scale;
}
