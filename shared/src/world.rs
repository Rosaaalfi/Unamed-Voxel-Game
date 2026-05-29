use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
pub enum BlockKind {
    Air,
    Grass,
    Dirt,
    Stone,
    Sand,
    Water,
    OakLog,
    OakLeaves,
    Cobblestone,
}

impl BlockKind {
    pub const fn texture_path(self) -> Option<&'static str> {
        match self {
            Self::Air => None,
            Self::Grass => Some("textures/blocks/grass_block_top.png"),
            Self::Dirt => Some("textures/blocks/dirt.png"),
            Self::Stone => Some("textures/blocks/stone.png"),
            Self::Sand => Some("textures/blocks/sand.png"),
            Self::Water => Some("textures/blocks/water_still.png"),
            Self::OakLog => Some("textures/blocks/oak_log.png"),
            Self::OakLeaves => Some("textures/blocks/oak_leaves.png"),
            Self::Cobblestone => Some("textures/blocks/cobblestone.png"),
        }
    }

    pub const fn bedrock_geometry_model_path(self) -> Option<&'static str> {
        match self {
            Self::Air => None,
            Self::Grass => Some("models/blocks/grass_block.geo.json"),
            Self::Dirt => Some("models/blocks/dirt_block.geo.json"),
            Self::Stone => Some("models/blocks/stone_block.geo.json"),
            Self::Sand | Self::Water | Self::OakLog | Self::OakLeaves | Self::Cobblestone => None,
        }
    }

    pub const fn is_solid(self) -> bool {
        !matches!(self, Self::Air | Self::Water)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatWorldConfig {
    pub surface_y: i32,
    pub grass_layers: i32,
    pub dirt_layers: i32,
    pub stone_layers: i32,
}

impl Default for FlatWorldConfig {
    fn default() -> Self {
        Self {
            surface_y: 0,
            grass_layers: 1,
            dirt_layers: 2,
            stone_layers: 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockPosition {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlockCell {
    pub position: BlockPosition,
    pub kind: BlockKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlatWorldChunk {
    pub width: i32,
    pub depth: i32,
    pub cells: Vec<BlockCell>,
}

impl FlatWorldChunk {
    pub fn generate(config: FlatWorldConfig, width: i32, depth: i32) -> Self {
        let mut cells = Vec::new();

        for x in 0..width {
            for z in 0..depth {
                for layer in 0..config.grass_layers {
                    cells.push(BlockCell {
                        position: BlockPosition {
                            x,
                            y: config.surface_y - layer,
                            z,
                        },
                        kind: BlockKind::Grass,
                    });
                }

                for layer in 0..config.dirt_layers {
                    cells.push(BlockCell {
                        position: BlockPosition {
                            x,
                            y: config.surface_y - config.grass_layers - layer,
                            z,
                        },
                        kind: BlockKind::Dirt,
                    });
                }

                for layer in 0..config.stone_layers {
                    cells.push(BlockCell {
                        position: BlockPosition {
                            x,
                            y: config.surface_y - config.grass_layers - config.dirt_layers - layer,
                            z,
                        },
                        kind: BlockKind::Stone,
                    });
                }
            }
        }

        Self {
            width,
            depth,
            cells,
        }
    }

    pub fn block_at(&self, x: i32, y: i32, z: i32) -> Option<BlockKind> {
        self.cells
            .iter()
            .find(|cell| cell.position.x == x && cell.position.y == y && cell.position.z == z)
            .map(|cell| cell.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_requested_flat_layers() {
        let world = FlatWorldChunk::generate(FlatWorldConfig::default(), 1, 1);

        assert_eq!(world.block_at(0, 0, 0), Some(BlockKind::Grass));
        assert_eq!(world.block_at(0, -1, 0), Some(BlockKind::Dirt));
        assert_eq!(world.block_at(0, -2, 0), Some(BlockKind::Dirt));
        assert_eq!(world.block_at(0, -3, 0), Some(BlockKind::Stone));
        assert_eq!(world.block_at(0, -4, 0), Some(BlockKind::Stone));
        assert_eq!(world.block_at(0, -5, 0), None);
    }
}
