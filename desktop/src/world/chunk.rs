//! Chunk-based world storage and meshing.
//!
//! Instead of spawning one entity per block, blocks are grouped into
//! 16×16 chunks (in XZ). Each chunk generates a single mesh with
//! **face culling** — faces adjacent to solid blocks are omitted.
//!
//! ## Performance
//!
//! For a 48×48×5 world this drops entity count from ~11 520 to ~9,
//! and draw calls from 11 520 to **9**.

use super::components::{
    ChunkLoadAnimation, ChunkUnloadAnimation, PlayerController, WaterChunkEntity,
};
use super::resources::ChunkManager;
use crate::GamePack;
use bevy::asset::RenderAssetUsages;
use bevy::light::NotShadowCaster;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, PrimitiveTopology, TextureDimension, TextureFormat};
use shared::pack::PackDirectory;
use shared::world::BlockKind;
use std::collections::HashSet;

/// Number of blocks along X and Z per chunk.
pub const CHUNK_SIZE: usize = 16;

/// Height of the chunk block grid.
pub const CHUNK_HEIGHT: usize = 192;
const WORLD_SEED: i32 = 13_371_337;
const TERRAIN_MIN_Y: i32 = -64;
const TERRAIN_MAX_Y: i32 = 127;
/// Average sea level (Y)
const SEA_LEVEL: i32 = 46;
/// Water surface height relative to block top (fraction of block height)
const WATER_HEIGHT: f32 = 0.875;

#[derive(Clone, Copy, PartialEq, Eq)]
enum BiomeKind {
    Plains,
    Forest,
    Desert,
    Ocean,
    Swamp,
    Taiga,
    Jungle,
    Savanna,
}

// ---------------------------------------------------------------------------
// Components
// ---------------------------------------------------------------------------

/// Marks an entity as a chunk. Stores the raw block data.
#[derive(Component, Clone)]
pub struct Chunk {
    /// Chunk column position (in chunk coords).
    pub position: IVec2,
    /// World Y coordinate of local block Y=0 (lowest block in this chunk).
    pub y_world_offset: f32,
    /// 3D block grid: `[x + z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE]`.
    pub blocks: Vec<BlockKind>,
    /// LOD level: 0 = full detail, 1 = 2×2×2 merged (lower res).
    pub lod_level: u8,
}

impl Chunk {
    fn index(x: usize, y: usize, z: usize) -> usize {
        x + z * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE
    }

    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockKind {
        if x >= CHUNK_SIZE || y >= CHUNK_HEIGHT || z >= CHUNK_SIZE {
            return BlockKind::Air;
        }
        self.blocks[Self::index(x, y, z)]
    }

    #[allow(dead_code)]
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, kind: BlockKind) {
        if x < CHUNK_SIZE && y < CHUNK_HEIGHT && z < CHUNK_SIZE {
            let idx = Self::index(x, y, z);
            self.blocks[idx] = kind;
        }
    }

    /// Convert a world Y coordinate to local block Y index (relative to this chunk's y_offset).
    pub fn world_y_to_local(&self, world_y: f32) -> i32 {
        (world_y.floor() - self.y_world_offset.floor()) as i32
    }

    /// Get the block kind at a world position (uses chunk position + y_offset).
    pub fn block_at_world(&self, world_pos: Vec3) -> BlockKind {
        let bx = world_pos.x.floor() as i32 - self.position.x * CHUNK_SIZE as i32;
        let bz = world_pos.z.floor() as i32 - self.position.y * CHUNK_SIZE as i32;
        let by = self.world_y_to_local(world_pos.y);
        if bx >= 0
            && bx < CHUNK_SIZE as i32
            && bz >= 0
            && bz < CHUNK_SIZE as i32
            && by >= 0
            && (by as usize) < CHUNK_HEIGHT
        {
            self.get_block(bx as usize, by as usize, bz as usize)
        } else {
            BlockKind::Air
        }
    }
}

fn generated_block_at_world(wx: i32, wy: i32, wz: i32) -> BlockKind {
    if !(TERRAIN_MIN_Y..=TERRAIN_MAX_Y).contains(&wy) {
        return BlockKind::Air;
    }
    let height = terrain_height(wx, wz);
    if wy <= height {
        let biome = biome_at(wx, wz);
        let beach =
            height <= SEA_LEVEL + 1 || biome == BiomeKind::Desert || biome == BiomeKind::Savanna;
        let depth = height - wy;
        if wy == height {
            if beach {
                BlockKind::Sand
            } else {
                BlockKind::Grass
            }
        } else if depth <= 3 {
            if beach {
                BlockKind::Sand
            } else {
                BlockKind::Dirt
            }
        } else if depth <= 7 {
            BlockKind::Stone
        } else {
            BlockKind::Cobblestone
        }
    } else {
        let water_fill = if biome_at(wx, wz) == BiomeKind::Swamp {
            SEA_LEVEL - 1
        } else {
            SEA_LEVEL
        };
        if wy <= water_fill {
            BlockKind::Water
        } else {
            BlockKind::Air
        }
    }
}

// ---------------------------------------------------------------------------
// Meshing
// ---------------------------------------------------------------------------

/// Build a mesh for a chunk with face culling (omit faces adjacent to solid blocks).
/// Only includes opaque blocks (skips water).
/// Pre-compute the Y range of non-air blocks in a chunk to limit iteration.
fn chunk_occupied_y_range(chunk: &Chunk) -> (usize, usize) {
    let mut min_y = CHUNK_HEIGHT;
    let mut max_y = 0;
    for y in 0..CHUNK_HEIGHT {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                if chunk.get_block(x, y, z) != BlockKind::Air {
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }
    }
    if min_y > max_y {
        (0, 0) // empty chunk
    } else {
        // Pad by 1 to catch boundary faces
        (min_y.saturating_sub(1), (max_y + 2).min(CHUNK_HEIGHT))
    }
}

pub fn mesh_chunk(chunk: &Chunk) -> Mesh {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();

    let (y_start, y_end) = chunk_occupied_y_range(chunk);

    for y in y_start..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let kind = chunk.get_block(x, y, z);
                if kind == BlockKind::Air || kind == BlockKind::Water {
                    continue;
                }
                let base_pos = [x as f32, y as f32, z as f32];

                // Check each of the 6 directions
                for &(dx, dy, dz, side) in &[
                    (0, -1, 0, 1), // bottom (-Y) — side=1 for correct downward normal
                    (0, 1, 0, 0),  // top (+Y) — side=0 for correct upward normal
                    (-1, 0, 0, 0), // left (-X)
                    (1, 0, 0, 1),  // right (+X)
                    (0, 0, -1, 0), // back (-Z)
                    (0, 0, 1, 1),  // front (+Z)
                ] {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;

                    let neighbor = if ny < 0 || ny >= CHUNK_HEIGHT as i32 {
                        BlockKind::Air
                    } else if nx < 0 || nx >= CHUNK_SIZE as i32 || nz < 0 || nz >= CHUNK_SIZE as i32
                    {
                        let wx = chunk.position.x * CHUNK_SIZE as i32 + nx;
                        let wy = chunk.y_world_offset as i32 + ny;
                        let wz = chunk.position.y * CHUNK_SIZE as i32 + nz;
                        generated_block_at_world(wx, wy, wz)
                    } else {
                        chunk.get_block(nx as usize, ny as usize, nz as usize)
                    };

                    if neighbor == kind || (neighbor.is_solid() && kind != BlockKind::Water) {
                        continue;
                    }

                    let base_idx = vertices.len() as u32;
                    let (v0, v1, v2, v3) = face_verts(base_pos, dx, dy, dz);
                    let normal = [dx as f32, dy as f32, dz as f32];

                    vertices.extend_from_slice(&[v0, v1, v2, v3]);
                    normals.extend_from_slice(&[normal, normal, normal, normal]);
                    uvs.extend_from_slice(&block_face_uvs(kind, dx, dy, dz));
                    let wx = chunk.position.x * CHUNK_SIZE as i32 + x as i32;
                    let wz = chunk.position.y * CHUNK_SIZE as i32 + z as i32;
                    colors.extend_from_slice(&[block_face_color(kind, dy, wx, wz); 4]);

                    if side == 0 {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 2,
                            base_idx + 1,
                            base_idx,
                            base_idx + 3,
                            base_idx + 2,
                        ]);
                    } else {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 1,
                            base_idx + 2,
                            base_idx,
                            base_idx + 2,
                            base_idx + 3,
                        ]);
                    }
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Build a separate mesh for water blocks in a chunk (faces that touch air or non-water).
/// This uses the same face-culling logic but only emits water faces.
/// Build a water mesh with short (non-full-block) water surface.
///
/// Minecraft-style water rendering:
/// - Top face: flat quad at `y + WATER_HEIGHT` (0.875), NOT at y+1.0
/// - Side faces: only when water block touches AIR (visible water column edge)
/// - No bottom face (never visible from below)
/// - Water adjacent to other water or solid blocks hides the shared face
pub fn mesh_chunk_water(chunk: &Chunk) -> Mesh {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();

    let (y_start, y_end) = chunk_occupied_y_range(chunk);

    for y in y_start..y_end {
        for z in 0..CHUNK_SIZE {
            for x in 0..CHUNK_SIZE {
                let kind = chunk.get_block(x, y, z);
                if kind != BlockKind::Water {
                    continue;
                }
                let wx = chunk.position.x * CHUNK_SIZE as i32 + x as i32;
                let wz = chunk.position.y * CHUNK_SIZE as i32 + z as i32;

                // Helper: check if a coordinate is outside chunk bounds
                let out_of_bounds = |cx: i32, cy: i32, cz: i32| -> bool {
                    cx < 0
                        || cx >= CHUNK_SIZE as i32
                        || cy < 0
                        || cy >= CHUNK_HEIGHT as i32
                        || cz < 0
                        || cz >= CHUNK_SIZE as i32
                };

                // Helper: get neighbor block kind within this chunk
                let block_in_chunk =
                    |bx: usize, by: usize, bz: usize| -> BlockKind { chunk.get_block(bx, by, bz) };

                // ── TOP FACE (water surface, at y + WATER_HEIGHT) ──
                // Only show top surface if neighbor above is Air or non-solid
                let top_ny = (y as i32) + 1;
                let show_top = if top_ny >= CHUNK_HEIGHT as i32 {
                    true // exposed at chunk top boundary
                } else {
                    chunk.get_block(x, top_ny as usize, z) == BlockKind::Air
                };
                if show_top {
                    let wh = WATER_HEIGHT;
                    let base_idx = vertices.len() as u32;
                    let v0 = [x as f32, y as f32 + wh, z as f32];
                    let v1 = [x as f32 + 1.0, y as f32 + wh, z as f32];
                    let v2 = [x as f32 + 1.0, y as f32 + wh, z as f32 + 1.0];
                    let v3 = [x as f32, y as f32 + wh, z as f32 + 1.0];
                    let normal = [0.0, 1.0, 0.0];
                    vertices.extend_from_slice(&[v0, v1, v2, v3]);
                    normals.extend_from_slice(&[normal, normal, normal, normal]);
                    uvs.extend_from_slice(&block_face_uvs(kind, 0, 1, 0));
                    colors.extend_from_slice(&[block_face_color(kind, 1, wx, wz); 4]);
                    indices.extend_from_slice(&[
                        base_idx,
                        base_idx + 2,
                        base_idx + 1,
                        base_idx,
                        base_idx + 3,
                        base_idx + 2,
                    ]);
                }

                // ── SIDE FACES (only where water meets AIR within the same chunk) ──
                // For chunk-boundary edges: skip side faces entirely to avoid
                // visible seams (adjacent chunk may also have water, which would
                // create a doubled-face seam).
                // Order: [(-X), (+X), (-Z), (+Z)] — no bottom face
                for &(dx, dz, side) in &[] as &[(i32, i32, i32)] {
                    let nx = x as i32 + dx;
                    let nz = z as i32 + dz;
                    // Skip chunk-boundary edges — adjacent chunk may also have water
                    if out_of_bounds(nx, y as i32, nz) {
                        continue;
                    }
                    let neighbor = block_in_chunk(nx as usize, y, nz as usize);
                    // Only show side face when neighbor is Air (not Water, not solid)
                    if neighbor != BlockKind::Air {
                        continue;
                    }

                    let wh = WATER_HEIGHT;
                    let base_idx = vertices.len() as u32;
                    let (v0, v1, v2, v3) = if dx != 0 {
                        let xf = if dx > 0 { x as f32 + 1.0 } else { x as f32 };
                        (
                            [xf, y as f32, z as f32],
                            [xf, y as f32 + wh, z as f32],
                            [xf, y as f32 + wh, z as f32 + 1.0],
                            [xf, y as f32, z as f32 + 1.0],
                        )
                    } else {
                        let zf = if dz > 0 { z as f32 + 1.0 } else { z as f32 };
                        (
                            [x as f32, y as f32, zf],
                            [x as f32 + 1.0, y as f32, zf],
                            [x as f32 + 1.0, y as f32 + wh, zf],
                            [x as f32, y as f32 + wh, zf],
                        )
                    };
                    let normal = [dx as f32, 0.0, dz as f32];
                    vertices.extend_from_slice(&[v0, v1, v2, v3]);
                    normals.extend_from_slice(&[normal, normal, normal, normal]);
                    uvs.extend_from_slice(&block_face_uvs(kind, dx, 0, dz));
                    colors.extend_from_slice(&[block_face_color(kind, 0, wx, wz); 4]);

                    if side == 0 {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 2,
                            base_idx + 1,
                            base_idx,
                            base_idx + 3,
                            base_idx + 2,
                        ]);
                    } else {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 1,
                            base_idx + 2,
                            base_idx,
                            base_idx + 2,
                            base_idx + 3,
                        ]);
                    }
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Build a lower-resolution LOD mesh for a chunk by merging 2×2×2 blocks
/// into single larger cubes. Used for chunks far from the player to reduce
/// vertex count.
pub fn mesh_chunk_lod(chunk: &Chunk) -> Mesh {
    let mut vertices: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();
    let mut colors: Vec<[f32; 4]> = Vec::new();

    let (y_start, y_end) = chunk_occupied_y_range(chunk);

    let lod_step = if chunk.lod_level >= 2 { 4 } else { 2 };
    for y in (y_start.max(0)..y_end.min(CHUNK_HEIGHT - 1)).step_by(lod_step) {
        for z in (0..CHUNK_SIZE).step_by(lod_step) {
            for x in (0..CHUNK_SIZE).step_by(lod_step) {
                // Determine the dominant (most common) block type in this 2×2×2 region
                let mut counts = std::collections::HashMap::new();
                for dy in 0..lod_step {
                    for dz in 0..lod_step {
                        for dx in 0..lod_step {
                            let bx = x + dx;
                            let by = y + dy;
                            let bz = z + dz;
                            if bx < CHUNK_SIZE && by < CHUNK_HEIGHT && bz < CHUNK_SIZE {
                                let kind = chunk.get_block(bx, by, bz);
                                if kind != BlockKind::Air && kind != BlockKind::Water {
                                    *counts.entry(kind).or_insert(0u32) += 1;
                                }
                            }
                        }
                    }
                }

                if counts.is_empty() {
                    continue; // all air in this region
                }

                // Pick the most common block kind
                let majority_kind = counts
                    .into_iter()
                    .max_by_key(|&(_, count)| count)
                    .map(|(kind, _)| kind)
                    .unwrap_or(BlockKind::Grass);

                // The merged cube spans [x..x+lod_step] in block coordinates
                let merged_size = lod_step as f32;
                let base_pos = [x as f32, y as f32, z as f32];

                // Check each face direction, but only at the 2×2×2 boundary
                for &(dx, dy, dz, side) in &[
                    (0, -1, 0, 1), // bottom (-Y) — side=1 for correct downward normal
                    (0, 1, 0, 0),  // top (+Y) — side=0 for correct upward normal
                    (-1, 0, 0, 0), // left (-X)
                    (1, 0, 0, 1),  // right (+X)
                    (0, 0, -1, 0), // back (-Z)
                    (0, 0, 1, 1),  // front (+Z)
                ] {
                    // Check if this face of the merged cube is exposed
                    let nx = x as i32 + dx * lod_step as i32;
                    let ny = y as i32 + dy * lod_step as i32;
                    let nz = z as i32 + dz * lod_step as i32;

                    let neighbor_air = if nx < 0
                        || nx >= CHUNK_SIZE as i32
                        || ny < 0
                        || ny >= CHUNK_HEIGHT as i32
                        || nz < 0
                        || nz >= CHUNK_SIZE as i32
                    {
                        true // exposed at chunk boundary
                    } else {
                        // Check if the neighbor region is all air
                        let mut has_solid = false;
                        for dy2 in 0..lod_step {
                            for dz2 in 0..lod_step {
                                for dx2 in 0..lod_step {
                                    let bx = nx as usize + dx2;
                                    let by = ny as usize + dy2;
                                    let bz = nz as usize + dz2;
                                    if bx < CHUNK_SIZE && by < CHUNK_HEIGHT && bz < CHUNK_SIZE {
                                        let kind = chunk.get_block(bx, by, bz);
                                        if kind != BlockKind::Air && kind != BlockKind::Water {
                                            has_solid = true;
                                        }
                                    }
                                }
                            }
                        }
                        !has_solid
                    };

                    if !neighbor_air {
                        continue;
                    }

                    let base_idx = vertices.len() as u32;

                    // Generate face vertices for the merged cube (size = lod_step)
                    let (v0, v1, v2, v3) = if dx != 0 {
                        let xf = if dx > 0 {
                            base_pos[0] + merged_size
                        } else {
                            base_pos[0]
                        };
                        (
                            [xf, base_pos[1], base_pos[2]],
                            [xf, base_pos[1] + merged_size, base_pos[2]],
                            [xf, base_pos[1] + merged_size, base_pos[2] + merged_size],
                            [xf, base_pos[1], base_pos[2] + merged_size],
                        )
                    } else if dy != 0 {
                        let yf = if dy > 0 {
                            base_pos[1] + merged_size
                        } else {
                            base_pos[1]
                        };
                        (
                            [base_pos[0], yf, base_pos[2]],
                            [base_pos[0] + merged_size, yf, base_pos[2]],
                            [base_pos[0] + merged_size, yf, base_pos[2] + merged_size],
                            [base_pos[0], yf, base_pos[2] + merged_size],
                        )
                    } else {
                        let zf = if dz > 0 {
                            base_pos[2] + merged_size
                        } else {
                            base_pos[2]
                        };
                        (
                            [base_pos[0], base_pos[1], zf],
                            [base_pos[0] + merged_size, base_pos[1], zf],
                            [base_pos[0] + merged_size, base_pos[1] + merged_size, zf],
                            [base_pos[0], base_pos[1] + merged_size, zf],
                        )
                    };

                    let normal = [dx as f32, dy as f32, dz as f32];

                    vertices.extend_from_slice(&[v0, v1, v2, v3]);
                    normals.extend_from_slice(&[normal, normal, normal, normal]);
                    uvs.extend_from_slice(&block_face_uvs(majority_kind, dx, dy, dz));
                    let wx = chunk.position.x * CHUNK_SIZE as i32 + x as i32;
                    let wz = chunk.position.y * CHUNK_SIZE as i32 + z as i32;
                    let face_color = block_face_color(majority_kind, dy, wx, wz);
                    colors.extend_from_slice(&[face_color; 4]);

                    if side == 0 {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 2,
                            base_idx + 1,
                            base_idx,
                            base_idx + 3,
                            base_idx + 2,
                        ]);
                    } else {
                        indices.extend_from_slice(&[
                            base_idx,
                            base_idx + 1,
                            base_idx + 2,
                            base_idx,
                            base_idx + 2,
                            base_idx + 3,
                        ]);
                    }
                }
            }
        }
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

/// Return the 4 corner vertices of a face.
fn face_verts(
    pos: [f32; 3],
    dx: i32,
    dy: i32,
    dz: i32,
) -> ([f32; 3], [f32; 3], [f32; 3], [f32; 3]) {
    let (x, y, z) = (pos[0], pos[1], pos[2]);
    if dx != 0 {
        let xf = if dx > 0 { x + 1.0 } else { x };
        (
            [xf, y, z],
            [xf, y + 1.0, z],
            [xf, y + 1.0, z + 1.0],
            [xf, y, z + 1.0],
        )
    } else if dy != 0 {
        let yf = if dy > 0 { y + 1.0 } else { y };
        (
            [x, yf, z],
            [x + 1.0, yf, z],
            [x + 1.0, yf, z + 1.0],
            [x, yf, z + 1.0],
        )
    } else {
        let zf = if dz > 0 { z + 1.0 } else { z };
        (
            [x, y, zf],
            [x + 1.0, y, zf],
            [x + 1.0, y + 1.0, zf],
            [x, y + 1.0, zf],
        )
    }
}

fn block_face_uvs(kind: BlockKind, dx: i32, dy: i32, dz: i32) -> [[f32; 2]; 4] {
    let tile = match kind {
        BlockKind::Stone => 0,
        BlockKind::Dirt => 1,
        BlockKind::Grass if dy > 0 => 2,
        BlockKind::Grass if dy < 0 => 1,
        BlockKind::Grass => 3,
        BlockKind::Sand => 4,
        BlockKind::Water => 5,
        BlockKind::OakLog if dy != 0 => 7,
        BlockKind::OakLog => 6,
        BlockKind::OakLeaves => 8,
        BlockKind::Cobblestone => 9,
        BlockKind::Air => 0,
    };

    let tiles = 10.0;
    let tile_px = 16.0;
    let u_pad = 0.5 / (tile_px * tiles);
    let v_pad = 0.5 / tile_px;
    let u0 = tile as f32 / tiles + u_pad;
    let u1 = (tile as f32 + 1.0) / tiles - u_pad;
    let v0 = v_pad;
    let v1 = 1.0 - v_pad;

    if dx != 0 {
        [[u0, v1], [u0, v0], [u1, v0], [u1, v1]]
    } else if dz != 0 {
        [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
    } else {
        [[u0, v1], [u1, v1], [u1, v0], [u0, v0]]
    }
}

fn biome_grass_top_color(biome: BiomeKind) -> [f32; 4] {
    match biome {
        BiomeKind::Forest => [0.44, 0.74, 0.31, 1.0],
        BiomeKind::Plains => [0.58, 0.86, 0.38, 1.0],
        BiomeKind::Desert => [0.72, 0.70, 0.42, 1.0],
        BiomeKind::Ocean => [0.48, 0.74, 0.40, 1.0],
        BiomeKind::Swamp => [0.38, 0.56, 0.30, 1.0],
        BiomeKind::Taiga => [0.34, 0.66, 0.28, 1.0],
        BiomeKind::Jungle => [0.28, 0.80, 0.24, 1.0],
        BiomeKind::Savanna => [0.66, 0.76, 0.32, 1.0],
    }
}

fn biome_grass_side_color(biome: BiomeKind) -> [f32; 4] {
    match biome {
        BiomeKind::Forest => [0.70, 0.92, 0.60, 1.0],
        BiomeKind::Plains => [0.80, 0.96, 0.66, 1.0],
        BiomeKind::Desert => [0.90, 0.84, 0.54, 1.0],
        BiomeKind::Ocean => [0.68, 0.86, 0.64, 1.0],
        BiomeKind::Swamp => [0.56, 0.72, 0.48, 1.0],
        BiomeKind::Taiga => [0.52, 0.78, 0.44, 1.0],
        BiomeKind::Jungle => [0.46, 0.90, 0.40, 1.0],
        BiomeKind::Savanna => [0.86, 0.88, 0.52, 1.0],
    }
}

fn biome_water_color(biome: BiomeKind) -> [f32; 4] {
    match biome {
        BiomeKind::Ocean => [0.35, 0.58, 0.92, 1.0],
        BiomeKind::Desert => [0.48, 0.70, 0.96, 1.0],
        BiomeKind::Swamp => [0.28, 0.42, 0.28, 1.0],
        BiomeKind::Taiga => [0.32, 0.56, 0.80, 1.0],
        BiomeKind::Jungle => [0.38, 0.68, 0.78, 1.0],
        BiomeKind::Savanna => [0.44, 0.72, 0.90, 1.0],
        _ => [0.42, 0.64, 0.94, 1.0],
    }
}

fn biome_leaves_color(biome: BiomeKind) -> [f32; 4] {
    match biome {
        BiomeKind::Forest => [0.52, 0.82, 0.38, 1.0],
        BiomeKind::Swamp => [0.40, 0.60, 0.32, 1.0],
        BiomeKind::Taiga => [0.38, 0.72, 0.34, 1.0],
        BiomeKind::Jungle => [0.32, 0.84, 0.30, 1.0],
        _ => [0.66, 0.90, 0.52, 1.0],
    }
}

fn block_face_color(kind: BlockKind, dy: i32, x: i32, z: i32) -> [f32; 4] {
    let biome = biome_at(x, z);
    match kind {
        BlockKind::Grass if dy > 0 => biome_grass_top_color(biome),
        BlockKind::Grass if dy == 0 => biome_grass_side_color(biome),
        BlockKind::Water => biome_water_color(biome),
        BlockKind::OakLeaves => biome_leaves_color(biome),
        _ => [1.0, 1.0, 1.0, 1.0],
    }
}

// ---------------------------------------------------------------------------
// Coordinate helpers
// ---------------------------------------------------------------------------

/// Convert a world position to chunk column and local block coordinates.
pub fn world_to_chunk(world_pos: Vec3) -> (IVec2, IVec3) {
    let bx = world_pos.x.floor() as i32;
    let bz = world_pos.z.floor() as i32;
    let cx = if bx >= 0 {
        bx / CHUNK_SIZE as i32
    } else {
        (bx + 1) / CHUNK_SIZE as i32 - 1
    };
    let cz = if bz >= 0 {
        bz / CHUNK_SIZE as i32
    } else {
        (bz + 1) / CHUNK_SIZE as i32 - 1
    };
    let lx = bx - cx * CHUNK_SIZE as i32;
    let lz = bz - cz * CHUNK_SIZE as i32;
    let ly = world_pos.y.floor() as i32;
    (IVec2::new(cx, cz), IVec3::new(lx, ly, lz))
}

/// Get the block kind at a world position by checking all loaded chunks.
#[allow(dead_code)]
pub fn get_block_at(chunks: &[&Chunk], world_pos: Vec3) -> BlockKind {
    #[allow(unused_variables)]
    let (chunk_pos, local) = world_to_chunk(world_pos);
    let ly = local.y;
    if ly < 0 || ly >= CHUNK_HEIGHT as i32 {
        return BlockKind::Air;
    }
    let lx = local.x;
    let lz = local.z;
    if lx < 0 || lx >= CHUNK_SIZE as i32 || lz < 0 || lz >= CHUNK_SIZE as i32 {
        return BlockKind::Air;
    }
    for chunk in chunks {
        if chunk.position == chunk_pos {
            return chunk.get_block(lx as usize, ly as usize, lz as usize);
        }
    }
    BlockKind::Air
}

/// Set a block at a world position (returns true if any chunk was modified).
#[allow(dead_code)]
pub fn set_block_at(chunks: &mut [&mut Chunk], world_pos: Vec3, kind: BlockKind) -> bool {
    let (chunk_pos, local) = world_to_chunk(world_pos);
    let ly = local.y;
    if ly < 0 || ly >= CHUNK_HEIGHT as i32 {
        return false;
    }
    let lx = local.x;
    let lz = local.z;
    if lx < 0 || lx >= CHUNK_SIZE as i32 || lz < 0 || lz >= CHUNK_SIZE as i32 {
        return false;
    }
    for chunk in chunks {
        if chunk.position == chunk_pos {
            chunk.set_block(lx as usize, ly as usize, lz as usize, kind);
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

fn build_terrain_material(
    pack: &GamePack,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> (Handle<StandardMaterial>, Handle<StandardMaterial>) {
    const TILE: u32 = 16;
    let sources = [
        ("blocks/stone.png", [110, 110, 110, 255]),
        ("blocks/dirt.png", [128, 90, 50, 255]),
        ("blocks/grass_block_top.png", [80, 170, 60, 255]),
        ("blocks/grass_block_side.png", [95, 140, 55, 255]),
        ("blocks/sand.png", [210, 194, 125, 255]),
        ("blocks/water_still.png", [70, 120, 220, 255]),
        ("blocks/oak_log.png", [112, 72, 36, 255]),
        ("blocks/oak_log_top.png", [158, 124, 78, 255]),
        ("blocks/oak_leaves.png", [60, 135, 48, 210]),
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

    let image = Image::new(
        Extent3d {
            width: TILE * sources.len() as u32,
            height: TILE,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        atlas,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    let handle = images.add(image);

    let terrain = materials.add(StandardMaterial {
        base_color_texture: Some(handle.clone()),
        base_color: Color::srgb(0.74, 0.78, 0.70),
        perceptual_roughness: 0.94,
        metallic: 0.0,
        reflectance: 0.05,
        ..default()
    });
    let water = materials.add(StandardMaterial {
        base_color_texture: Some(handle),
        base_color: Color::srgba(0.36, 0.58, 1.0, 0.72),
        alpha_mode: AlphaMode::Blend,
        perceptual_roughness: 0.55,
        metallic: 0.0,
        reflectance: 0.08,
        cull_mode: None,
        ..default()
    });
    (terrain, water)
}

fn generate_chunk_blocks(chunk_pos: IVec2) -> Vec<BlockKind> {
    let mut blocks = vec![BlockKind::Air; CHUNK_SIZE * CHUNK_SIZE * CHUNK_HEIGHT];

    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = chunk_pos.x * CHUNK_SIZE as i32 + x as i32;
            let wz = chunk_pos.y * CHUNK_SIZE as i32 + z as i32;
            let height = terrain_height(wx, wz);
            let biome = biome_at(wx, wz);
            let beach = height <= SEA_LEVEL + 1
                || biome == BiomeKind::Desert
                || biome == BiomeKind::Savanna;

            // Swamp: lower terrain, more water
            let surface_block = if biome == BiomeKind::Swamp && height <= SEA_LEVEL + 1 {
                BlockKind::Grass
            } else if beach {
                BlockKind::Sand
            } else {
                BlockKind::Grass
            };

            // Taiga: podzol-like (dirt) surface
            let surface = if biome == BiomeKind::Taiga && !beach {
                BlockKind::Dirt
            } else {
                surface_block
            };

            for wy in TERRAIN_MIN_Y..=height.min(TERRAIN_MAX_Y) {
                let ly = (wy - TERRAIN_MIN_Y) as usize;
                let depth = height - wy;
                let kind = if wy == height {
                    surface
                } else if depth <= 3 {
                    if beach {
                        BlockKind::Sand
                    } else {
                        BlockKind::Dirt
                    }
                } else if depth <= 7 {
                    BlockKind::Stone
                } else {
                    BlockKind::Cobblestone
                };
                let idx = x + z * CHUNK_SIZE + ly * CHUNK_SIZE * CHUNK_SIZE;
                blocks[idx] = kind;
            }

            // Swamp: water fills up to just below surface
            let water_fill = if biome == BiomeKind::Swamp {
                SEA_LEVEL - 1
            } else {
                SEA_LEVEL
            };
            if height < water_fill {
                for wy in (height + 1)..=water_fill.min(TERRAIN_MAX_Y) {
                    let ly = (wy - TERRAIN_MIN_Y) as usize;
                    let idx = x + z * CHUNK_SIZE + ly * CHUNK_SIZE * CHUNK_SIZE;
                    blocks[idx] = BlockKind::Water;
                }
            }
        }
    }

    // ── Tree generation per biome ──
    for z in 2..CHUNK_SIZE.saturating_sub(2) {
        for x in 2..CHUNK_SIZE.saturating_sub(2) {
            let wx = chunk_pos.x * CHUNK_SIZE as i32 + x as i32;
            let wz = chunk_pos.y * CHUNK_SIZE as i32 + z as i32;
            let h = terrain_height(wx, wz);
            if h <= SEA_LEVEL + 1 || h + 7 > TERRAIN_MAX_Y {
                continue;
            }
            let biome = biome_at(wx, wz);
            let density = match biome {
                BiomeKind::Forest => 67u32,
                BiomeKind::Jungle => 47u32,
                BiomeKind::Taiga => 57u32,
                BiomeKind::Swamp => 77u32,
                _ => continue,
            };
            if hash2(wx / 5, wz / 5, WORLD_SEED + 91) % density != 0 {
                continue;
            }
            match biome {
                BiomeKind::Forest | BiomeKind::Swamp => {
                    place_oak_tree(&mut blocks, x, z, h + 1);
                }
                BiomeKind::Taiga => {
                    place_pine_tree(&mut blocks, x, z, h + 1);
                }
                BiomeKind::Jungle => {
                    place_jungle_tree(&mut blocks, x, z, h + 1);
                }
                _ => {}
            }
        }
    }

    // ── Swamp vines ──
    for z in 0..CHUNK_SIZE {
        for x in 0..CHUNK_SIZE {
            let wx = chunk_pos.x * CHUNK_SIZE as i32 + x as i32;
            let wz = chunk_pos.y * CHUNK_SIZE as i32 + z as i32;
            if biome_at(wx, wz) != BiomeKind::Swamp {
                continue;
            }
            // Add a bit more surface water/grass variation
        }
    }

    blocks
}

fn place_pine_tree(blocks: &mut [BlockKind], x: usize, z: usize, base_y: i32) {
    let trunk_height = 5 + (hash2(x as i32, z as i32, WORLD_SEED + 311) % 4) as i32;
    for wy in base_y..(base_y + trunk_height) {
        set_local_block(blocks, x as i32, wy, z as i32, BlockKind::OakLog);
    }
    // Tapered pine/spruce foliage
    let top = base_y + trunk_height;
    for dy in -3i32..=1 {
        let radius = if dy >= 0 {
            1
        } else if dy == -1 {
            2
        } else {
            2 - (dy.abs() - 2)
        };
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() + dz.abs() > radius + 1 {
                    continue;
                }
                set_local_block(
                    blocks,
                    x as i32 + dx,
                    top + dy,
                    z as i32 + dz,
                    BlockKind::OakLeaves,
                );
            }
        }
    }
}

fn place_jungle_tree(blocks: &mut [BlockKind], x: usize, z: usize, base_y: i32) {
    let trunk_height = 6 + (hash2(x as i32, z as i32, WORLD_SEED + 411) % 5) as i32;
    // 2x2 trunk
    for wy in base_y..(base_y + trunk_height) {
        for dx in 0..2 {
            for dz in 0..2 {
                set_local_block(blocks, x as i32 + dx, wy, z as i32 + dz, BlockKind::OakLog);
            }
        }
    }
    // Sprawling canopy
    let top = base_y + trunk_height;
    for dy in -3i32..=2 {
        let radius: i32 = if dy >= 1 {
            3
        } else if dy == 0 {
            3
        } else {
            2
        };
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                let dist: i32 = dx.abs() + dz.abs();
                if dist > radius + 1 || (dist == 0 && dy < 1) {
                    continue;
                }
                set_local_block(
                    blocks,
                    x as i32 + dx,
                    top + dy,
                    z as i32 + dz,
                    BlockKind::OakLeaves,
                );
            }
        }
    }
}

fn terrain_height(x: i32, z: i32) -> i32 {
    let mut h = terrain_height_raw(x, z);
    // Flatten terrain in Swamp
    if biome_at(x, z) == BiomeKind::Swamp {
        h = (h + SEA_LEVEL) / 2;
    }
    h
}

fn biome_at(x: i32, z: i32) -> BiomeKind {
    let moisture = value_noise(
        x as f32 * 0.004 + 71.0,
        z as f32 * 0.004 - 23.0,
        WORLD_SEED + 31,
    );
    let temp = value_noise(
        x as f32 * 0.003 - 19.0,
        z as f32 * 0.003 + 41.0,
        WORLD_SEED + 47,
    );
    let h = terrain_height_raw(x, z);
    if h < SEA_LEVEL - 1 {
        BiomeKind::Ocean
    } else if temp > 0.62 && moisture < 0.42 {
        BiomeKind::Desert
    } else if temp > 0.58 && moisture > 0.42 && moisture < 0.65 {
        BiomeKind::Savanna
    } else if moisture > 0.72 {
        if temp > 0.45 {
            BiomeKind::Jungle
        } else {
            BiomeKind::Taiga
        }
    } else if moisture > 0.56 {
        if temp > 0.55 {
            BiomeKind::Forest
        } else if temp > 0.30 {
            BiomeKind::Taiga
        } else {
            BiomeKind::Taiga
        }
    } else if moisture > 0.40 && temp > 0.50 {
        BiomeKind::Swamp
    } else {
        BiomeKind::Plains
    }
}

fn terrain_height_raw(x: i32, z: i32) -> i32 {
    // Continental base — large scale rolling terrain
    let continent = fbm_noise(x as f32 * 0.003, z as f32 * 0.003, WORLD_SEED) * 28.0;
    // Hills / mid-scale variation
    let hills = fbm_noise(
        x as f32 * 0.007 + 31.0,
        z as f32 * 0.007 - 17.0,
        WORLD_SEED + 7,
    ) * 18.0;
    // Mountainous peaks (high frequency, large amplitude)
    let mountains = fbm_noise(
        x as f32 * 0.004 + 11.0,
        z as f32 * 0.004 - 7.0,
        WORLD_SEED + 17,
    )
    .max(0.0)
        * 72.0;
    // Noise caves/depth carving
    let carving = (fbm_noise(
        x as f32 * 0.012 + 99.0,
        z as f32 * 0.012 - 88.0,
        WORLD_SEED + 27,
    ) * 0.5
        + 0.5)
        .max(0.3);

    // Blend between plains/hills and mountains based on a mask
    let mountain_mask = (value_noise(x as f32 * 0.0015, z as f32 * 0.0015, WORLD_SEED + 13) * 2.0
        - 0.5)
        .clamp(0.0, 1.0);

    let base = continent + hills * (1.0 - mountain_mask) + mountains * mountain_mask;
    let height = base * carving + SEA_LEVEL as f32;
    height.round() as i32
}

fn fbm_noise(x: f32, z: f32, seed: i32) -> f32 {
    let mut amp = 0.5;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for octave in 0..4 {
        sum += value_noise(x * freq, z * freq, seed + octave * 97) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm * 2.0 - 1.0
}

fn value_noise(x: f32, z: f32, seed: i32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let xf = smoothstep(x - x.floor());
    let zf = smoothstep(z - z.floor());
    let a = hash_unit(x0, z0, seed);
    let b = hash_unit(x0 + 1, z0, seed);
    let c = hash_unit(x0, z0 + 1, seed);
    let d = hash_unit(x0 + 1, z0 + 1, seed);
    let x1 = a + (b - a) * xf;
    let x2 = c + (d - c) * xf;
    x1 + (x2 - x1) * zf
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn hash_unit(x: i32, z: i32, seed: i32) -> f32 {
    (hash2(x, z, seed) as f32) / (u32::MAX as f32)
}

fn hash2(x: i32, z: i32, seed: i32) -> u32 {
    let mut h = seed as u32;
    h ^= (x as u32).wrapping_mul(0x9E37_79B9);
    h = h.rotate_left(13);
    h ^= (z as u32).wrapping_mul(0x85EB_CA6B);
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^ (h >> 15)
}

fn place_oak_tree(blocks: &mut [BlockKind], x: usize, z: usize, base_y: i32) {
    let trunk_height = 4 + (hash2(x as i32, z as i32, WORLD_SEED + 211) % 3) as i32;
    for wy in base_y..(base_y + trunk_height) {
        set_local_block(blocks, x as i32, wy, z as i32, BlockKind::OakLog);
    }
    let leaf_center = base_y + trunk_height;
    for dy in -2i32..=2 {
        let radius: i32 = if dy.abs() == 2 { 1 } else { 2 };
        for dz in -radius..=radius {
            for dx in -radius..=radius {
                if dx.abs() + dz.abs() > radius + 1 {
                    continue;
                }
                set_local_block(
                    blocks,
                    x as i32 + dx,
                    leaf_center + dy,
                    z as i32 + dz,
                    BlockKind::OakLeaves,
                );
            }
        }
    }
}

fn set_local_block(blocks: &mut [BlockKind], x: i32, wy: i32, z: i32, kind: BlockKind) {
    if x < 0
        || x >= CHUNK_SIZE as i32
        || z < 0
        || z >= CHUNK_SIZE as i32
        || !(TERRAIN_MIN_Y..=TERRAIN_MAX_Y).contains(&wy)
    {
        return;
    }
    let y = (wy - TERRAIN_MIN_Y) as usize;
    let idx = x as usize + z as usize * CHUNK_SIZE + y * CHUNK_SIZE * CHUNK_SIZE;
    blocks[idx] = kind;
}

fn target_lod_for(player_chunk: IVec2, chunk_pos: IVec2) -> u8 {
    let dist = (chunk_pos.x - player_chunk.x)
        .abs()
        .max((chunk_pos.y - player_chunk.y).abs());
    if dist >= 6 {
        2
    } else if dist >= 3 {
        1
    } else {
        0
    }
}

/// Generate the flat world, store it in ChunkManager, and initially spawn chunks
/// within render distance around the origin.
pub fn setup_chunk_world(
    mut commands: Commands,
    _world: Option<Res<crate::LoadedWorld>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    pack: Res<GamePack>,
) {
    let min_world_y = TERRAIN_MIN_Y;
    let render_radius = 7;
    let initial_radius = 2;

    let (terrain_material, water_material_initial) =
        build_terrain_material(&pack, &mut images, &mut materials);
    let mut loaded_chunks = HashSet::new();

    for cx in -initial_radius..=initial_radius {
        for cz in -initial_radius..=initial_radius {
            let chunk_pos = IVec2::new(cx, cz);
            let blocks = generate_chunk_blocks(chunk_pos);
            let world_x = cx as f32 * CHUNK_SIZE as f32;
            let world_z = cz as f32 * CHUNK_SIZE as f32;
            let chunk_translation = Vec3::new(world_x, min_world_y as f32, world_z);

            // Build meshes before moving chunk data
            let chunk = Chunk {
                position: chunk_pos,
                y_world_offset: min_world_y as f32,
                blocks,
                lod_level: target_lod_for(IVec2::ZERO, chunk_pos),
            };
            let opaque_mesh = meshes.add(if chunk.lod_level > 0 {
                mesh_chunk_lod(&chunk)
            } else {
                mesh_chunk(&chunk)
            });
            let water_mesh_handle = meshes.add(mesh_chunk_water(&chunk));
            let mat_handle = terrain_material.clone();

            commands
                .spawn((
                    chunk,
                    Mesh3d(opaque_mesh),
                    MeshMaterial3d(mat_handle),
                    Transform::from_translation(chunk_translation),
                    ChunkLoadAnimation { age: 0.0 },
                ))
                .with_children(|parent| {
                    // Water child entity — auto-despawned when chunk unloads
                    parent.spawn((
                        Mesh3d(water_mesh_handle),
                        MeshMaterial3d(water_material_initial.clone()),
                        Transform::default(),
                        WaterChunkEntity,
                        NotShadowCaster,
                    ));
                });

            loaded_chunks.insert(chunk_pos);
        }
    }

    commands.insert_resource(ChunkManager {
        render_radius,
        loaded_chunks,
        terrain_material,
        water_material: water_material_initial,
    });

    info!(
        "spawned terrain chunks around origin (radius {})",
        render_radius
    );
}

/// Dynamically load/unload chunks based on the player's current position
/// and the configured render distance.
pub fn update_chunk_loading(
    mut commands: Commands,
    player_query: Query<&Transform, With<PlayerController>>,
    mut chunk_manager: ResMut<ChunkManager>,
    mut meshes: ResMut<Assets<Mesh>>,
    _materials: ResMut<Assets<StandardMaterial>>,
    chunk_query: Query<(Entity, &Chunk, Option<&ChunkUnloadAnimation>)>,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };

    let player_cx = (player_transform.translation.x / CHUNK_SIZE as f32).floor() as i32;
    let player_cz = (player_transform.translation.z / CHUNK_SIZE as f32).floor() as i32;

    let radius = chunk_manager.render_radius;

    let min_world_y = TERRAIN_MIN_Y;

    // ── Compute the set of chunk positions that SHOULD be loaded ──
    let mut desired_chunks = HashSet::new();
    for dx in -radius..=radius {
        for dz in -radius..=radius {
            desired_chunks.insert(IVec2::new(player_cx + dx, player_cz + dz));
        }
    }
    let player_chunk = IVec2::new(player_cx, player_cz);
    let mut desired_order: Vec<IVec2> = desired_chunks.iter().copied().collect();
    desired_order.sort_by_key(|p| {
        (p.x - player_chunk.x)
            .abs()
            .max((p.y - player_chunk.y).abs())
    });

    // ── Despawn chunks that are no longer within render distance ──
    let mut unloaded_this_frame = 0;
    let mut lod_updates_this_frame = 0;
    for (entity, chunk, unloading) in &chunk_query {
        if !desired_chunks.contains(&chunk.position) {
            if unloading.is_none() {
                commands
                    .entity(entity)
                    .insert(ChunkUnloadAnimation { age: 0.0 });
                unloaded_this_frame += 1;
                if unloaded_this_frame >= 2 {
                    break;
                }
            }
        }
    }

    // ── Spawn newly-needed chunks ──
    let mut spawned_this_frame = 0;
    for &chunk_pos in &desired_order {
        if chunk_manager.loaded_chunks.contains(&chunk_pos) {
            continue;
        }

        let lod_level = target_lod_for(IVec2::new(player_cx, player_cz), chunk_pos);

        let blocks = generate_chunk_blocks(chunk_pos);

        let chunk = Chunk {
            position: chunk_pos,
            y_world_offset: min_world_y as f32,
            blocks,
            lod_level,
        };

        let mesh_handle = meshes.add(if lod_level > 0 {
            mesh_chunk_lod(&chunk)
        } else {
            mesh_chunk(&chunk)
        });
        let water_mesh_handle = meshes.add(mesh_chunk_water(&chunk));
        let mat_handle = chunk_manager.terrain_material.clone();
        let water_mat = chunk_manager.water_material.clone();

        let world_x = chunk_pos.x as f32 * CHUNK_SIZE as f32;
        let world_z = chunk_pos.y as f32 * CHUNK_SIZE as f32;

        commands
            .spawn((
                chunk,
                Mesh3d(mesh_handle),
                MeshMaterial3d(mat_handle),
                Transform::from_translation(Vec3::new(world_x, min_world_y as f32, world_z)),
                ChunkLoadAnimation { age: 0.0 },
            ))
            .with_children(|parent| {
                parent.spawn((
                    Mesh3d(water_mesh_handle),
                    MeshMaterial3d(water_mat),
                    Transform::default(),
                    WaterChunkEntity,
                    NotShadowCaster,
                ));
            });

        chunk_manager.loaded_chunks.insert(chunk_pos);
        spawned_this_frame += 1;
        if spawned_this_frame >= 2 {
            break;
        }
    }

    // ── LOD transitions: update meshing for chunks whose LOD level changed ──
    for (entity, chunk, unloading) in &chunk_query {
        if unloading.is_some() {
            continue;
        }
        if !desired_chunks.contains(&chunk.position) {
            continue; // already scheduled for removal
        }
        let target_lod = target_lod_for(IVec2::new(player_cx, player_cz), chunk.position);

        if chunk.lod_level != target_lod {
            // Update LOD and remesh via command closure
            let e = entity;
            commands.queue(move |world: &mut World| {
                if let Some(mut c) = world.get_mut::<Chunk>(e) {
                    c.lod_level = target_lod;
                    let new_mesh = if target_lod > 0 {
                        mesh_chunk_lod(&c)
                    } else {
                        mesh_chunk(&c)
                    };
                    let mut meshes = world.resource_mut::<Assets<Mesh>>();
                    let new_handle = meshes.add(new_mesh);
                    drop(meshes);
                    if let Some(mut m3d) = world.get_mut::<Mesh3d>(e) {
                        m3d.0 = new_handle;
                    }
                }
            });
            lod_updates_this_frame += 1;
            if lod_updates_this_frame >= 1 {
                break;
            }
        }
    }
}

pub fn animate_chunk_loads(
    time: Res<Time>,
    mut commands: Commands,
    mut chunks: Query<(Entity, &mut Transform, &mut ChunkLoadAnimation), With<Chunk>>,
) {
    for (entity, mut transform, mut anim) in &mut chunks {
        anim.age += time.delta_secs();
        let t = (anim.age / 0.38).clamp(0.0, 1.0);
        let eased = 1.0 - (1.0 - t) * (1.0 - t);
        transform.scale = Vec3::splat(0.985 + eased * 0.015);
        if t >= 1.0 {
            transform.scale = Vec3::ONE;
            commands.entity(entity).remove::<ChunkLoadAnimation>();
        }
    }
}

pub fn animate_chunk_unloads(
    time: Res<Time>,
    mut commands: Commands,
    mut chunk_manager: ResMut<ChunkManager>,
    mut chunks: Query<(Entity, &Chunk, &mut Transform, &mut ChunkUnloadAnimation)>,
) {
    for (entity, chunk, mut transform, mut anim) in &mut chunks {
        anim.age += time.delta_secs();
        let t = (anim.age / 0.26).clamp(0.0, 1.0);
        transform.scale = Vec3::splat(1.0 - t * 0.025);
        if t >= 1.0 {
            chunk_manager.loaded_chunks.remove(&chunk.position);
            commands.entity(entity).despawn();
        }
    }
}

pub fn update_water_flow(
    time: Res<Time>,
    mut step_accum: Local<f32>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut chunks: Query<
        (&mut Chunk, &Children, &mut Mesh3d),
        (Without<WaterChunkEntity>, With<Chunk>),
    >,
    mut water_meshes: Query<&mut Mesh3d, (With<WaterChunkEntity>, Without<Chunk>)>,
) {
    *step_accum += time.delta_secs();
    if *step_accum < 0.75 {
        return;
    }
    *step_accum = 0.0;

    let mut changed_chunks = 0;
    for (mut chunk, children, mut terrain_mesh) in &mut chunks {
        if changed_chunks >= 1 {
            break;
        }

        let original = chunk.blocks.clone();
        let mut next = original.clone();
        let mut changes = 0;

        for y in (0..CHUNK_HEIGHT).rev() {
            for z in 1..CHUNK_SIZE.saturating_sub(1) {
                for x in 1..CHUNK_SIZE.saturating_sub(1) {
                    if original[Chunk::index(x, y, z)] != BlockKind::Water {
                        continue;
                    }

                    if y > 0 && original[Chunk::index(x, y - 1, z)] == BlockKind::Air {
                        next[Chunk::index(x, y - 1, z)] = BlockKind::Water;
                        changes += 1;
                    } else if y > 0 {
                        for (nx, nz) in [(x - 1, z), (x + 1, z), (x, z - 1), (x, z + 1)] {
                            let idx = Chunk::index(nx, y, nz);
                            let below_idx = Chunk::index(nx, y - 1, nz);
                            if original[idx] == BlockKind::Air
                                && original[below_idx] != BlockKind::Air
                                && original[below_idx] != BlockKind::Water
                            {
                                next[idx] = BlockKind::Water;
                                changes += 1;
                                break;
                            }
                        }
                    }

                    if changes >= 12 {
                        break;
                    }
                }
                if changes >= 12 {
                    break;
                }
            }
            if changes >= 12 {
                break;
            }
        }

        if changes == 0 {
            continue;
        }

        chunk.blocks = next;
        let terrain = if chunk.lod_level > 0 {
            mesh_chunk_lod(&chunk)
        } else {
            mesh_chunk(&chunk)
        };
        terrain_mesh.0 = meshes.add(terrain);
        let water_handle = meshes.add(mesh_chunk_water(&chunk));
        for child in children.iter() {
            if let Ok(mut water_mesh) = water_meshes.get_mut(child) {
                water_mesh.0 = water_handle.clone();
                break;
            }
        }
        changed_chunks += 1;
    }
}
