//! Anchor-based UI positioning system.
//!
//! Provides an [`Anchor`] component that controls where a UI node is placed
//! relative to the window edges, similar to Unity / Godot anchor presets.
//!
//! # Usage
//!
//! ```ignore
//! commands.spawn((
//!     Node { width: Val::Px(16.0), height: Val::Px(16.0), ..default() },
//!     Anchor::new(AnchorPoint::Center),
//!     Crosshair,
//! ));
//! ```

use bevy::prelude::*;
use bevy::window::PrimaryWindow;

// ---------------------------------------------------------------------------
// Anchor point preset
// ---------------------------------------------------------------------------

/// The nine standard anchor presets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum AnchorPoint {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

// ---------------------------------------------------------------------------
// Anchor component
// ---------------------------------------------------------------------------

/// Attach this to any UI [`Node`] to have its position automatically
/// computed from the chosen anchor point + pixel offset every frame.
#[derive(Debug, Clone, Copy, Component)]
pub struct Anchor {
    /// Which edge/corner/center the node snaps to.
    pub point: AnchorPoint,
    /// Additional pixel offset **after** anchoring.
    /// +X = right, +Y = up (screen-space).
    pub offset: Vec2,
}

impl Anchor {
    pub const fn new(point: AnchorPoint) -> Self {
        Self {
            point,
            offset: Vec2::ZERO,
        }
    }

    pub const fn with_offset(mut self, offset: Vec2) -> Self {
        self.offset = offset;
        self
    }
}

// ---------------------------------------------------------------------------
// System
// ---------------------------------------------------------------------------

/// Applies anchor positioning every frame based on the primary window size.
///
/// Each anchored node must also have a [`ComputedNode`] (automatically added
/// by the UI layout) so we know its final layout size.
pub fn apply_anchor_positions(
    window: Query<&Window, With<PrimaryWindow>>,
    mut query: Query<(&mut Node, &Anchor, &ComputedNode)>,
) {
    let Ok(window) = window.single() else { return };
    let win_size = Vec2::new(window.resolution.width(), window.resolution.height());

    for (mut node, anchor, computed) in &mut query {
        let size = computed.size();
        let pos = anchor_position(anchor.point, win_size, size, anchor.offset);

        // Reset to absolute positioning, then set the anchor values
        node.position_type = PositionType::Absolute;
        node.left = Val::Px(pos.x);
        node.right = Val::Auto;
        node.top = Val::Px(pos.y);
        node.bottom = Val::Auto;
    }
}

/// Compute the pixel position (top-left corner of the node) based on anchor,
/// window size, element size, and offset.
fn anchor_position(point: AnchorPoint, win_size: Vec2, elem_size: Vec2, offset: Vec2) -> Vec2 {
    let x = match point {
        AnchorPoint::TopLeft | AnchorPoint::CenterLeft | AnchorPoint::BottomLeft => offset.x,
        AnchorPoint::TopCenter | AnchorPoint::Center | AnchorPoint::BottomCenter => {
            (win_size.x - elem_size.x) * 0.5 + offset.x
        }
        AnchorPoint::TopRight | AnchorPoint::CenterRight | AnchorPoint::BottomRight => {
            win_size.x - elem_size.x + offset.x
        }
    };
    let y = match point {
        AnchorPoint::TopLeft | AnchorPoint::TopCenter | AnchorPoint::TopRight => offset.y,
        AnchorPoint::CenterLeft | AnchorPoint::Center | AnchorPoint::CenterRight => {
            (win_size.y - elem_size.y) * 0.5 + offset.y
        }
        AnchorPoint::BottomLeft | AnchorPoint::BottomCenter | AnchorPoint::BottomRight => {
            win_size.y - elem_size.y + offset.y
        }
    };
    Vec2::new(x, y)
}
