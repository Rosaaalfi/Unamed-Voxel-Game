use bevy::asset::{AssetPlugin, UnapprovedPathMode};
use bevy::prelude::*;
use shared::{
    GAME_TITLE, PACK_NAMESPACE,
    pack::{PackAtlasEntry, PackDirectory, PackManifest, PackResolver},
};
use std::collections::HashMap;
use std::{env, path::PathBuf};
mod player;
use player::PlayerPlugin;
mod world;
use world::WorldPlugin;
mod ui;
use ui::UiPlugin;
mod debug;
use debug::DebugPlugin;
use game_server::start_in_background;
use shared::world::FlatWorldChunk;
use std::io::Read;
use std::net::TcpStream;
#[derive(Resource, Clone)]
#[allow(dead_code)]
struct LoadedWorld(FlatWorldChunk);

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

fn main() {
    // Auto-start local server for single-player (background thread)
    let _server_handle = start_in_background();

    unsafe {
        env::set_var("WGPU_BACKEND", "vulkan");
    }

    let game_pack = GamePack(build_pack_resolver());

    App::new()
        .insert_resource(game_pack)
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    // Keep sandboxing strict by default, then explicitly allow selected paths via load_override.
                    unapproved_path_mode: UnapprovedPathMode::Deny,
                    ..default()
                })
                .set(ImagePlugin::default_nearest()),
        )
        .add_plugins(PlayerPlugin)
        .add_plugins(WorldPlugin)
        .add_plugins(UiPlugin)
        .add_plugins(DebugPlugin)
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

    let locale_pack = pack.0.resolve_in(PackDirectory::Locale, "en-us.json");

    let mut loaded_textures = LoadedPackTextures::default();
    for atlas in &loaded_atlases {
        for sprite in &atlas.sprites {
            if let Some(path) = pack.0.resolve(sprite) {
                let handle = asset_server.load_override(path.to_string_lossy().to_string());
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

    info!("{} desktop client ready: {:?}", GAME_TITLE, locale_pack);

    // Try to fetch world state from local server
    match TcpStream::connect("127.0.0.1:4000") {
        Ok(mut stream) => {
            let mut buf = String::new();
            if stream.read_to_string(&mut buf).is_ok() {
                if let Ok(world) = serde_json::from_str::<FlatWorldChunk>(&buf) {
                    info!("fetched world from server: {} cells", world.cells.len());
                    commands.insert_resource(LoadedWorld(world));
                } else {
                    warn!("failed to parse world JSON from server");
                }
            } else {
                warn!("failed to read response from world server");
            }
        }
        Err(_) => {
            warn!("could not connect to local world server");
        }
    }
}
