//! Mesh-building utilities.
//!
//! Converts Bedrock geometry / block-model descriptions into Bevy `Mesh`
//! assets ready for rendering.
//!
//! # Entry points
//! - `build_mesh_from_geometry`   – full Bedrock geometry (player, mobs, items).
//! - `build_mesh_from_block_model`– Bedrock block model (stone, dirt, …).
//! - `make_unit_cube`             – quick 1×1 cube for fallback / water blocks.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::Indices;
use bevy::prelude::*;
use bevy::render::render_resource::PrimitiveTopology;
use shared::bedrock::{
    BedrockBlockElement, BedrockBlockFace, BedrockGeometryFile, BedrockGeometryUv,
};
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Build a mesh from a Bedrock geometry file (player, mobs, items).
#[allow(dead_code)]
pub fn build_mesh_from_geometry(
    geo: &BedrockGeometryFile,
    meshes: &mut Assets<Mesh>,
) -> Option<Handle<Mesh>> {
    let mut positions: Vec<[f32; 3]> = Vec::new();
    let mut normals: Vec<[f32; 3]> = Vec::new();
    let mut uvs: Vec<[f32; 2]> = Vec::new();
    let mut indices: Vec<u32> = Vec::new();

    for geo_entry in &geo.geometries {
        for bone in &geo_entry.bones {
            for cube in &bone.cubes {
                let origin = cube.origin;
                let size = cube.size;
                let from = [
                    origin[0] as f32 / 16.0 - 0.5,
                    origin[1] as f32 / 16.0,
                    origin[2] as f32 / 16.0 - 0.5,
                ];
                let to = [
                    from[0] + size[0] as f32 / 16.0,
                    from[1] + size[1] as f32 / 16.0,
                    from[2] + size[2] as f32 / 16.0,
                ];

                let elem = BedrockBlockElement {
                    from: [from[0] * 16.0 + 8.0, from[1] * 16.0, from[2] * 16.0 + 8.0],
                    to: [to[0] * 16.0 + 8.0, to[1] * 16.0, to[2] * 16.0 + 8.0],
                    faces: geometry_cube_faces(cube),
                };

                append_element_mesh(&elem, &mut positions, &mut normals, &mut uvs, &mut indices);
            }
        }
    }

    if positions.is_empty() {
        return None;
    }

    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices));

    Some(meshes.add(mesh))
}

fn geometry_cube_faces(cube: &shared::bedrock::BedrockCube) -> HashMap<String, BedrockBlockFace> {
    let mut faces = HashMap::new();
    for name in ["north", "south", "west", "east", "up", "down"] {
        faces.insert(
            name.to_string(),
            BedrockBlockFace {
                texture: String::new(),
                uv: geometry_face_rect(cube, name),
                rotation: None,
            },
        );
    }
    faces
}

fn geometry_face_rect(cube: &shared::bedrock::BedrockCube, face: &str) -> Option<[f32; 4]> {
    match cube.uv.as_ref()? {
        BedrockGeometryUv::PerFace(faces) => {
            let uv = faces.get(face)?;
            Some([
                uv.uv[0],
                uv.uv[1],
                uv.uv[0] + uv.uv_size[0],
                uv.uv[1] + uv.uv_size[1],
            ])
        }
        BedrockGeometryUv::Box([u, v]) => {
            let w = cube.size[0];
            let h = cube.size[1];
            Some([*u, *v, *u + w, *v + h])
        }
    }
}

/// Append the six faces of a block element to the given vertex/index arrays.
pub fn append_element_mesh(
    element: &BedrockBlockElement,
    positions: &mut Vec<[f32; 3]>,
    normals: &mut Vec<[f32; 3]>,
    uvs: &mut Vec<[f32; 2]>,
    indices: &mut Vec<u32>,
) {
    let from = element.from;
    let to = element.to;

    let x0 = from[0] / 16.0 - 0.5;
    let y0 = from[1] / 16.0;
    let z0 = from[2] / 16.0 - 0.5;
    let x1 = to[0] / 16.0 - 0.5;
    let y1 = to[1] / 16.0;
    let z1 = to[2] / 16.0 - 0.5;

    // North (-z)
    if let Some(face) = element.faces.get("north") {
        let base = positions.len() as u32;
        positions.extend(&[[x0, y0, z0], [x0, y1, z0], [x1, y1, z0], [x1, y0, z0]]);
        normals.extend(&[[0.0, 0.0, -1.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // South (+z)
    if let Some(face) = element.faces.get("south") {
        let base = positions.len() as u32;
        positions.extend(&[[x1, y0, z1], [x1, y1, z1], [x0, y1, z1], [x0, y0, z1]]);
        normals.extend(&[[0.0, 0.0, 1.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // West (-x)
    if let Some(face) = element.faces.get("west") {
        let base = positions.len() as u32;
        positions.extend(&[[x0, y0, z1], [x0, y1, z1], [x0, y1, z0], [x0, y0, z0]]);
        normals.extend(&[[-1.0, 0.0, 0.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // East (+x)
    if let Some(face) = element.faces.get("east") {
        let base = positions.len() as u32;
        positions.extend(&[[x1, y0, z0], [x1, y1, z0], [x1, y1, z1], [x1, y0, z1]]);
        normals.extend(&[[1.0, 0.0, 0.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Up (+y)
    if let Some(face) = element.faces.get("up") {
        let base = positions.len() as u32;
        positions.extend(&[[x0, y1, z0], [x0, y1, z1], [x1, y1, z1], [x1, y1, z0]]);
        normals.extend(&[[0.0, 1.0, 0.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    // Down (-y)
    if let Some(face) = element.faces.get("down") {
        let base = positions.len() as u32;
        positions.extend(&[[x0, y0, z1], [x0, y0, z0], [x1, y0, z0], [x1, y0, z1]]);
        normals.extend(&[[0.0, -1.0, 0.0]; 4]);
        append_uvs_from_face(face, uvs);
        indices.extend(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
}

/// Push UV coordinates for a single face.
pub fn append_uvs_from_face(face: &BedrockBlockFace, uvs: &mut Vec<[f32; 2]>) {
    if let Some(uv_rect) = face.uv {
        let u0 = uv_rect[0] / 16.0;
        let v0 = uv_rect[1] / 16.0;
        let u1 = uv_rect[2] / 16.0;
        let v1 = uv_rect[3] / 16.0;

        uvs.push([u0, v1]);
        uvs.push([u0, v0]);
        uvs.push([u1, v0]);
        uvs.push([u1, v1]);
    } else {
        uvs.push([0.0, 1.0]);
        uvs.push([0.0, 0.0]);
        uvs.push([1.0, 0.0]);
        uvs.push([1.0, 1.0]);
    }
}

/// Create a simple 1×1 unit cube mesh (centered at origin).
#[allow(dead_code)]
pub fn make_unit_cube(meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
    let positions: Vec<[f32; 3]> = vec![
        // front
        [-0.5, 0.0, -0.5],
        [-0.5, 1.0, -0.5],
        [0.5, 1.0, -0.5],
        [0.5, 0.0, -0.5],
        // back
        [0.5, 0.0, 0.5],
        [0.5, 1.0, 0.5],
        [-0.5, 1.0, 0.5],
        [-0.5, 0.0, 0.5],
        // left
        [-0.5, 0.0, 0.5],
        [-0.5, 1.0, 0.5],
        [-0.5, 1.0, -0.5],
        [-0.5, 0.0, -0.5],
        // right
        [0.5, 0.0, -0.5],
        [0.5, 1.0, -0.5],
        [0.5, 1.0, 0.5],
        [0.5, 0.0, 0.5],
        // up
        [-0.5, 1.0, -0.5],
        [-0.5, 1.0, 0.5],
        [0.5, 1.0, 0.5],
        [0.5, 1.0, -0.5],
        // down
        [-0.5, 0.0, 0.5],
        [-0.5, 0.0, -0.5],
        [0.5, 0.0, -0.5],
        [0.5, 0.0, 0.5],
    ];
    let normals: Vec<[f32; 3]> = vec![[0.0, 0.0, -1.0]; 4]
        .into_iter()
        .chain(vec![[0.0, 0.0, 1.0]; 4])
        .chain(vec![[-1.0, 0.0, 0.0]; 4])
        .chain(vec![[1.0, 0.0, 0.0]; 4])
        .chain(vec![[0.0, 1.0, 0.0]; 4])
        .chain(vec![[0.0, -1.0, 0.0]; 4])
        .collect();
    let uvs: Vec<[f32; 2]> = vec![[0.0, 1.0], [0.0, 0.0], [1.0, 0.0], [1.0, 1.0]]
        .into_iter()
        .cycle()
        .take(24)
        .collect();
    let indices: Vec<u32> = vec![
        0, 1, 2, 0, 2, 3, 4, 5, 6, 4, 6, 7, 8, 9, 10, 8, 10, 11, 12, 13, 14, 12, 14, 15, 16, 17,
        18, 16, 18, 19, 20, 21, 22, 20, 22, 23,
    ];

    let mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(Indices::U32(indices));

    meshes.add(mesh)
}
