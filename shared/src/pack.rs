use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackDirectory {
    Atlases,
    Locale,
    Models,
    Shaders,
    Sounds,
    Textures,
}

impl PackDirectory {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Atlases => "atlases",
            Self::Locale => "locale",
            Self::Models => "models",
            Self::Shaders => "shaders",
            Self::Sounds => "sounds",
            Self::Textures => "textures",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackLayer {
    pub root: PathBuf,
}

#[derive(Debug, Clone, Default)]
pub struct PackResolver {
    layers: Vec<PackLayer>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackManifest {
    pub namespace: String,
    #[serde(default)]
    pub atlases: Vec<PackAtlasEntry>,
    #[serde(default)]
    pub locale: Vec<String>,
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub shaders: Vec<String>,
    #[serde(default)]
    pub sounds: Vec<String>,
    #[serde(default)]
    pub textures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackAtlasEntry {
    pub name: String,
    pub source: String,
    #[serde(default)]
    pub sprites: Vec<String>,
}

impl PackResolver {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_layer(mut self, root: impl Into<PathBuf>) -> Self {
        self.layers.push(PackLayer { root: root.into() });
        self
    }

    pub fn push_layer(&mut self, root: impl Into<PathBuf>) -> &mut Self {
        self.layers.push(PackLayer { root: root.into() });
        self
    }

    pub fn resolve(&self, relative_path: impl AsRef<Path>) -> Option<PathBuf> {
        self.layers.iter().find_map(|layer| {
            let candidate = layer.root.join(relative_path.as_ref());
            candidate.exists().then_some(candidate)
        })
    }

    pub fn resolve_all(&self, relative_path: impl AsRef<Path>) -> Vec<PathBuf> {
        self.layers
            .iter()
            .filter_map(|layer| {
                let candidate = layer.root.join(relative_path.as_ref());
                candidate.exists().then_some(candidate)
            })
            .collect()
    }

    pub fn resolve_in(
        &self,
        directory: PackDirectory,
        relative_path: impl AsRef<Path>,
    ) -> Option<PathBuf> {
        self.resolve(Path::new(directory.as_str()).join(relative_path.as_ref()))
    }

    pub fn resolve_all_in(
        &self,
        directory: PackDirectory,
        relative_path: impl AsRef<Path>,
    ) -> Vec<PathBuf> {
        self.resolve_all(Path::new(directory.as_str()).join(relative_path.as_ref()))
    }

    pub fn load_string(&self, relative_path: impl AsRef<Path>) -> std::io::Result<String> {
        let path = self.resolve(relative_path).ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, "pack asset not found")
        })?;

        std::fs::read_to_string(path)
    }

    pub fn load_json<T>(&self, relative_path: impl AsRef<Path>) -> std::io::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let contents = self.load_string(relative_path)?;

        serde_json::from_str(&contents)
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
    }

    pub fn load_json_in<T>(
        &self,
        directory: PackDirectory,
        relative_path: impl AsRef<Path>,
    ) -> std::io::Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        self.load_json(Path::new(directory.as_str()).join(relative_path.as_ref()))
    }

    pub fn load_manifest(&self) -> std::io::Result<PackManifest> {
        self.load_json(Path::new("pack.json"))
    }

    pub fn load_merged_atlas(&self, source: &str) -> std::io::Result<PackAtlasEntry> {
        let all_paths = self.resolve_all_in(PackDirectory::Atlases, source);

        if all_paths.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "atlas metadata not found",
            ));
        }

        // Build from lowest-priority layer to highest-priority layer.
        let mut merged: Option<PackAtlasEntry> = None;
        for path in all_paths.iter().rev() {
            let data = std::fs::read_to_string(path)?;
            let current: PackAtlasEntry = serde_json::from_str(&data)
                .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))?;

            if let Some(existing) = &mut merged {
                if !current.name.is_empty() {
                    existing.name = current.name.clone();
                }
                if !current.source.is_empty() {
                    existing.source = current.source.clone();
                }

                for sprite in current.sprites {
                    if !existing.sprites.contains(&sprite) {
                        existing.sprites.push(sprite);
                    }
                }
            } else {
                merged = Some(current);
            }
        }

        merged.ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid merged atlas")
        })
    }
}
