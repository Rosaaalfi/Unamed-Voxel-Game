use bevy::prelude::*;
use shared::{
    GAME_TITLE,
    world::{FlatWorldChunk, FlatWorldConfig},
};
use std::io::Write;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;

#[derive(Resource, Clone)]
pub struct WorldResource(pub Arc<FlatWorldChunk>);

fn build_world_arc() -> Arc<FlatWorldChunk> {
    let flat_world = FlatWorldChunk::generate(FlatWorldConfig::default(), 16, 16);
    info!(
        "{} server building flat world: {} blocks",
        GAME_TITLE,
        flat_world.cells.len()
    );
    Arc::new(flat_world)
}

fn serve_world_tcp(addr: &str, world: Arc<FlatWorldChunk>) {
    let listener = match TcpListener::bind(addr) {
        Ok(listener) => listener,
        Err(error) => {
            error!("failed to bind world TCP listener on {}: {}", addr, error);
            return;
        }
    };
    info!("world TCP server listening on {}", addr);

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let world_clone = world.clone();
                thread::spawn(move || handle_client(s, world_clone));
            }
            Err(e) => {
                error!("error accepting connection: {}", e);
            }
        }
    }
}

fn handle_client(mut stream: TcpStream, world: Arc<FlatWorldChunk>) {
    // For simplicity: on any connection, return entire world as JSON
    if let Ok(json) = serde_json::to_string(&*world) {
        let _ = stream.write_all(json.as_bytes());
    }
}

pub fn start_blocking() {
    let world = build_world_arc();

    // Start TCP server thread
    let listener_world = world.clone();
    thread::spawn(move || {
        serve_world_tcp("127.0.0.1:4000", listener_world);
    });

    App::new()
        .insert_resource(WorldResource(world))
        .add_plugins(MinimalPlugins)
        .run();
}

pub fn start_in_background() -> thread::JoinHandle<()> {
    thread::spawn(|| {
        start_blocking();
    })
}
