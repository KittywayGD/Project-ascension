//! Maillage du terrain par secteurs (chunks) à partir de la heightmap.
//!
//! Tous les chunks lisent la même heightmap globale (hauteurs ET normales) :
//! les sommets partagés aux frontières sont identiques, donc pas de fissures.

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use super::generation::HeightField;

/// Côté d'un chunk, en cellules de heightmap (64 m avec la résolution par défaut).
pub const CHUNK_CELLS: usize = 64;

/// Secteur de terrain, repéré par ses indices dans la grille de chunks.
/// Les indices dépendent uniquement de la carte, jamais de la caméra.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TerrainChunk {
    pub x: usize,
    pub z: usize,
}

/// Nombre de chunks par côté.
pub fn chunk_count(field: &HeightField) -> usize {
    (field.samples() - 1).div_ceil(CHUNK_CELLS)
}

/// Plage de cellules couverte par un chunk le long d'un axe : `[start, end)`.
fn cell_range(field: &HeightField, chunk: usize) -> (usize, usize) {
    let cells = field.samples() - 1;
    let start = chunk * CHUNK_CELLS;
    (start, (start + CHUNK_CELLS).min(cells))
}

/// Coin (x, z) du chunk dans le monde : sert de translation à son entité.
pub fn chunk_origin(field: &HeightField, chunk: TerrainChunk) -> Vec3 {
    let (i0, _) = cell_range(field, chunk.x);
    let (j0, _) = cell_range(field, chunk.z);
    let (x, z) = field.sample_position(i0, j0);
    Vec3::new(x, 0.0, z)
}

/// Construit le maillage d'un chunk, en coordonnées locales au chunk.
pub fn build_chunk_mesh(field: &HeightField, chunk: TerrainChunk) -> Mesh {
    let (i0, i1) = cell_range(field, chunk.x);
    let (j0, j1) = cell_range(field, chunk.z);
    let width = i1 - i0 + 1; // sommets par ligne
    let depth = j1 - j0 + 1;
    let origin = chunk_origin(field, chunk);
    let cell = field.config().cell_size_m;

    let vertex_count = width * depth;
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);

    for j in j0..=j1 {
        for i in i0..=i1 {
            let (x, z) = field.sample_position(i, j);
            let h = field.sample(i, j);
            let normal = sample_normal(field, i, j, cell);
            positions.push([x - origin.x, h, z - origin.z]);
            normals.push(normal.to_array());
            // UV en mètres / 16 : prêt pour une texture qui se répète tous les 16 m.
            uvs.push([x / 16.0, z / 16.0]);
            colors.push(ground_color(h, normal.y));
        }
    }

    let mut indices = Vec::with_capacity((width - 1) * (depth - 1) * 6);
    for lj in 0..depth - 1 {
        for li in 0..width - 1 {
            let a = (lj * width + li) as u32;
            let b = a + 1;
            let c = a + width as u32;
            let d = c + 1;
            // Diagonale a → d, identique à `HeightField::height_at`. Ordre antihoraire vu du dessus.
            indices.extend_from_slice(&[a, d, b, a, c, d]);
        }
    }

    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
}

/// Normale par différences centrées sur la heightmap globale.
fn sample_normal(field: &HeightField, i: usize, j: usize, cell: f32) -> Vec3 {
    let dhdx = (field.sample(i + 1, j) - field.sample(i.saturating_sub(1), j)) / (2.0 * cell);
    let dhdz = (field.sample(i, j + 1) - field.sample(i, j.saturating_sub(1))) / (2.0 * cell);
    Vec3::new(-dhdx, 1.0, -dhdz).normalize()
}

/// Couleur du sol selon l'altitude et la pente (`up` = composante verticale de la normale).
fn ground_color(height: f32, up: f32) -> [f32; 4] {
    let linear = |r, g, b| Color::srgb(r, g, b).to_linear().to_vec4();
    let seabed = linear(0.50, 0.46, 0.36);
    let sand = linear(0.80, 0.74, 0.54);
    let grass_low = linear(0.30, 0.47, 0.20);
    let grass_high = linear(0.45, 0.52, 0.30);
    let rock = linear(0.46, 0.44, 0.41);

    let grass = grass_low.lerp(grass_high, (height / 60.0).clamp(0.0, 1.0));
    let mut color = if height < 0.0 {
        seabed.lerp(sand, smoothstep((height + 4.0) / 4.0))
    } else {
        sand.lerp(grass, smoothstep((height - 1.0) / 2.0))
    };
    // Pentes raides (> ~30°) : roche.
    color = color.lerp(rock, smoothstep((0.88 - up) / 0.12));
    color.to_array()
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::{WorldGenConfig, WorldSeed};

    fn field(size_m: u32) -> HeightField {
        let config = WorldGenConfig {
            size_m,
            ..WorldGenConfig::default()
        };
        HeightField::generate(WorldSeed(11), &config)
    }

    #[test]
    fn chunks_cover_the_whole_map() {
        assert_eq!(chunk_count(&field(512)), 8);
        // Taille non multiple de 64 : un dernier chunk plus étroit.
        let odd = field(100);
        assert_eq!(chunk_count(&odd), 2);
        assert_eq!(cell_range(&odd, 1), (64, 100));
    }

    #[test]
    fn neighbouring_chunks_share_identical_border_vertices() {
        let field = field(128);
        let left = TerrainChunk { x: 0, z: 0 };
        let right = TerrainChunk { x: 1, z: 0 };
        let world_positions = |chunk: TerrainChunk| -> Vec<[f32; 3]> {
            let mesh = build_chunk_mesh(&field, chunk);
            let origin = chunk_origin(&field, chunk);
            let Some(bevy::mesh::VertexAttributeValues::Float32x3(positions)) =
                mesh.attribute(Mesh::ATTRIBUTE_POSITION)
            else {
                panic!("positions manquantes");
            };
            positions
                .iter()
                .map(|p| [p[0] + origin.x, p[1], p[2] + origin.z])
                .collect()
        };
        let (a, b) = (world_positions(left), world_positions(right));
        let width = CHUNK_CELLS + 1;
        for row in 0..width {
            // Dernière colonne du chunk gauche = première colonne du chunk droit.
            assert_eq!(a[row * width + CHUNK_CELLS], b[row * width]);
        }
    }

    #[test]
    fn steep_slopes_turn_to_rock_and_flat_land_stays_green() {
        let flat = ground_color(20.0, 1.0);
        let cliff = ground_color(20.0, 0.5);
        // Herbe : vert dominant. Roche : composantes presque égales.
        assert!(flat[1] > flat[0] && flat[1] > flat[2]);
        assert!((cliff[0] - cliff[1]).abs() < 0.05);
    }
}
