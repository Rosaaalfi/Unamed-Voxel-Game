use bevy::prelude::*;
use shared::{
    GAME_TITLE, PACK_NAMESPACE,
    pack::{PackAtlasEntry, PackDirectory, PackManifest, PackResolver},
};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Resource, Clone)]
struct GamePack(PackResolver);

#[derive(Resource, Clone)]
struct LoadedPackManifest {
    manifest: PackManifest,
}

#[derive(Resource, Clone)]
struct LoadedPackAtlases {
    atlases: Vec<PackAtlasEntry>,
}

#[derive(Resource, Clone, Default)]
struct LoadedPackTextures {
    handles: HashMap<String, Handle<Image>>,
}

#[cfg(target_os = "android")]
fn configure_graphics_backend() {
    unsafe {
        std::env::set_var("WGPU_BACKEND", "vulkan");
    }
}

#[cfg(target_os = "ios")]
fn configure_graphics_backend() {
    unsafe {
        std::env::set_var("WGPU_BACKEND", "metal");
    }
}

#[cfg(not(any(target_os = "android", target_os = "ios")))]
fn configure_graphics_backend() {}

fn main() {
    configure_graphics_backend();

    let game_pack = GamePack(build_pack_resolver());

    App::new()
        .insert_resource(game_pack)
        .add_plugins(DefaultPlugins)
        .add_systems(Startup, bootstrap)
        .run();
}

fn build_pack_resolver() -> PackResolver {
    let client_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("pack")
        .join(PACK_NAMESPACE);
    let shared_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("shared")
        .join("pack")
        .join(PACK_NAMESPACE);

    PackResolver::new()
        .with_layer(client_root)
        .with_layer(shared_root)
}

fn bootstrap(pack: Res<GamePack>, asset_server: Res<AssetServer>, mut commands: Commands) {
    let manifest = pack
        .0
        .load_manifest()
        .expect("failed to load shared pack manifest");

    let loaded_atlases: Vec<PackAtlasEntry> = manifest
        .atlases
        .iter()
        .map(|atlas| {
            let loaded = pack
                .0
                .load_merged_atlas(&atlas.source)
                .expect("failed to load atlas metadata");

            info!("loaded atlas metadata {} from {}", atlas.name, atlas.source);

            loaded
        })
        .collect();

    let texture_pack = pack
        .0
        .resolve_in(PackDirectory::Textures, "misc/overlay_air.png");

    let mut loaded_textures = LoadedPackTextures::default();
    for atlas in &loaded_atlases {
        for sprite in &atlas.sprites {
            if let Some(path) = pack.0.resolve(sprite) {
                let handle = asset_server.load(path.to_string_lossy().to_string());
                loaded_textures.handles.insert(sprite.clone(), handle);
            }
        }
    }

    let loaded_manifest = LoadedPackManifest { manifest };
    let loaded_atlases = LoadedPackAtlases {
        atlases: loaded_atlases,
    };

    info!(
        "pack manifest namespace: {}, atlas_count: {}, texture_handles: {}",
        loaded_manifest.manifest.namespace,
        loaded_atlases.atlases.len(),
        loaded_textures.handles.len()
    );

    commands.insert_resource(loaded_manifest);
    commands.insert_resource(loaded_atlases);
    commands.insert_resource(loaded_textures);

    info!("{} mobile client ready: {:?}", GAME_TITLE, texture_pack);
}
