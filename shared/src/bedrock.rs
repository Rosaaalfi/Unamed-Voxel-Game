use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct BedrockGeometryFile {
    #[serde(rename = "minecraft:geometry")]
    pub geometries: Vec<BedrockGeometry>,
}

#[derive(Debug, Deserialize)]
pub struct BedrockGeometry {
    pub description: Option<BedrockGeometryDescription>,
    pub bones: Vec<BedrockBone>,
}

#[derive(Debug, Deserialize)]
pub struct BedrockGeometryDescription {
    pub identifier: Option<String>,
    pub texture_width: Option<f32>,
    pub texture_height: Option<f32>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BedrockBone {
    pub name: String,
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub pivot: Option<[f32; 3]>,
    #[serde(default)]
    pub rotation: Option<[f32; 3]>,
    #[serde(default)]
    pub cubes: Vec<BedrockCube>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct BedrockCube {
    pub origin: [f32; 3],
    pub size: [f32; 3],
    #[serde(default)]
    pub inflate: Option<f32>,
    /// Bedrock geometry UVs. Supports both old box UV arrays and per-face UV objects.
    #[serde(default)]
    pub uv: Option<BedrockGeometryUv>,
    /// Legacy UV region size in texture pixels (defaults to cube size if absent).
    #[serde(default)]
    pub uv_size: Option<[f32; 2]>,
    /// Whether to mirror the UVs (used for symmetrical body parts)
    #[serde(default)]
    pub mirror: Option<bool>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum BedrockGeometryUv {
    Box([f32; 2]),
    PerFace(HashMap<String, BedrockGeometryFaceUv>),
}

#[derive(Debug, Deserialize, Clone, Copy)]
pub struct BedrockGeometryFaceUv {
    pub uv: [f32; 2],
    pub uv_size: [f32; 2],
}

#[derive(Debug, Deserialize)]
pub struct BedrockBlockModel {
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub textures: HashMap<String, String>,
    #[serde(default)]
    pub elements: Vec<BedrockBlockElement>,
}

#[derive(Debug, Deserialize)]
pub struct BedrockBlockElement {
    pub from: [f32; 3],
    pub to: [f32; 3],
    #[serde(default)]
    pub faces: HashMap<String, BedrockBlockFace>,
}

#[derive(Debug, Deserialize)]
pub struct BedrockBlockFace {
    pub texture: String,
    #[serde(default)]
    pub uv: Option<[f32; 4]>,
    #[serde(default)]
    pub rotation: Option<i32>,
}

pub fn parse_geometry_json(input: &str) -> serde_json::Result<BedrockGeometryFile> {
    serde_json::from_str(input)
}

pub fn parse_block_model_json(input: &str) -> serde_json::Result<BedrockBlockModel> {
    serde_json::from_str(input)
}
