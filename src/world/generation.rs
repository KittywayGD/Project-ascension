//! Génération déterministe du relief : seed + configuration + version → même heightmap.
//!
//! Module en Rust pur (aucun ECS ; seulement les types mathématiques de glam),
//! testable sans fenêtre. Toute modification qui change le monde produit pour une
//! seed donnée doit incrémenter [`WORLDGEN_VERSION`] (les tests de référence le rappellent).

use bevy::math::Vec3;

use super::noise::{Perlin, SplitMix64};

/// Version de la génération du monde. À incrémenter dès que le contenu généré change.
///
/// - v1 : relief + rochers.
/// - v2 : ajout des gisements (relief et rochers inchangés).
pub const WORLDGEN_VERSION: WorldGenVersion = WorldGenVersion(2);

/// Au-delà de cette profondeur d'eau, on ne peut plus avancer à pied (mètres).
pub const MAX_WADING_DEPTH_M: f32 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldGenVersion(pub u32);

/// Paramètres de génération, regroupés pour être sauvegardés avec la seed (V0.1).
///
/// Repère : le monde est centré sur l'origine, x et z dans [-size/2, size/2],
/// le niveau de la mer est à y = 0.
#[derive(Debug, Clone, PartialEq)]
pub struct WorldGenConfig {
    /// Côté de la carte carrée, en mètres.
    pub size_m: u32,
    /// Écart entre deux échantillons de la heightmap, en mètres.
    pub cell_size_m: f32,
    /// Hauteur maximale théorique des reliefs au-dessus de la mer, en mètres.
    pub max_height_m: f64,
    /// Longueur d'onde du relief principal, en mètres.
    pub base_wavelength_m: f64,
    /// Nombre d'octaves de bruit (détails de plus en plus fins).
    pub octaves: u32,
    /// Facteur d'amplitude entre deux octaves.
    pub persistence: f64,
    /// Facteur de fréquence entre deux octaves.
    pub lacunarity: f64,
    /// Décalage ajouté au bruit : > 0 donne plus de terres émergées.
    pub land_bias: f64,
    /// Largeur de la bande côtière où le terrain plonge vers la mer, en mètres.
    pub coast_width_m: f64,
    /// Élévation normalisée imposée tout au bord de la carte (fond marin).
    pub border_elevation: f64,
}

impl Default for WorldGenConfig {
    fn default() -> Self {
        Self {
            size_m: 512,
            cell_size_m: 1.0,
            max_height_m: 150.0,
            base_wavelength_m: 256.0,
            octaves: 5,
            persistence: 0.5,
            lacunarity: 2.0,
            land_bias: 0.2,
            coast_width_m: 96.0,
            border_elevation: -0.6,
        }
    }
}

impl WorldGenConfig {
    /// Nombre de cellules par côté.
    pub fn cells(&self) -> usize {
        (self.size_m as f32 / self.cell_size_m).round() as usize
    }

    pub fn half_size(&self) -> f32 {
        self.size_m as f32 / 2.0
    }
}

/// Heightmap régulière de `(cells + 1)²` échantillons.
#[derive(Debug, Clone)]
pub struct HeightField {
    seed: WorldSeed,
    config: WorldGenConfig,
    /// Nombre d'échantillons par côté (`cells + 1`).
    samples: usize,
    /// Hauteurs en mètres, ligne par ligne selon z, puis x.
    heights: Vec<f32>,
}

impl HeightField {
    pub fn generate(seed: WorldSeed, config: &WorldGenConfig) -> Self {
        let cells = config.cells();
        let samples = cells + 1;
        let noise = Perlin::new(seed.0);

        // Un décalage par octave évite que les octaves s'alignent sur l'origine.
        let mut rng = SplitMix64::new(seed.0 ^ 0xA5A5_5A5A_DEAD_BEEF);
        let offsets: Vec<(f64, f64)> = (0..config.octaves)
            .map(|_| (rng.next_f64() * 4096.0, rng.next_f64() * 4096.0))
            .collect();

        let half = f64::from(config.half_size());
        let cell = f64::from(config.cell_size_m);
        let mut heights = Vec::with_capacity(samples * samples);
        for j in 0..samples {
            let z = j as f64 * cell - half;
            for i in 0..samples {
                let x = i as f64 * cell - half;
                heights.push(elevation_at(&noise, &offsets, config, x, z) as f32);
            }
        }

        Self {
            seed,
            config: config.clone(),
            samples,
            heights,
        }
    }

    pub fn seed(&self) -> WorldSeed {
        self.seed
    }

    pub fn config(&self) -> &WorldGenConfig {
        &self.config
    }

    /// Nombre d'échantillons par côté.
    pub fn samples(&self) -> usize {
        self.samples
    }

    /// Hauteur de l'échantillon (i, j), indices bornés à la grille.
    pub fn sample(&self, i: usize, j: usize) -> f32 {
        let last = self.samples - 1;
        self.heights[j.min(last) * self.samples + i.min(last)]
    }

    /// Position monde (x, z) de l'échantillon (i, j).
    pub fn sample_position(&self, i: usize, j: usize) -> (f32, f32) {
        let half = self.config.half_size();
        let cell = self.config.cell_size_m;
        (i as f32 * cell - half, j as f32 * cell - half)
    }

    /// Hauteur du sol en (x, z), exactement sur les triangles du maillage.
    ///
    /// Chaque cellule est coupée selon la diagonale (i, j) → (i+1, j+1), comme dans
    /// `terrain.rs` : marcher sur le terrain suit donc la surface affichée.
    /// Hors carte, on prend la hauteur du bord le plus proche.
    pub fn height_at(&self, x: f32, z: f32) -> f32 {
        let half = self.config.half_size();
        let cell = self.config.cell_size_m;
        let last = (self.samples - 1) as f32;
        let gx = ((x + half) / cell).clamp(0.0, last);
        let gz = ((z + half) / cell).clamp(0.0, last);
        // Sur le bord max, on se place dans la dernière cellule.
        let i = (gx.floor() as usize).min(self.samples - 2);
        let j = (gz.floor() as usize).min(self.samples - 2);
        let (fx, fz) = (gx - i as f32, gz - j as f32);

        let a = self.sample(i, j);
        let b = self.sample(i + 1, j);
        let c = self.sample(i, j + 1);
        let d = self.sample(i + 1, j + 1);
        if fx >= fz {
            // Triangle (a, b, d)
            a + fx * (b - a) + fz * (d - b)
        } else {
            // Triangle (a, c, d)
            a + fz * (c - a) + fx * (d - c)
        }
    }

    /// Point d'apparition : la première terre émergée et peu pentue en partant du centre,
    /// parcourue en spirale carrée (déterministe). Retourne `None` si la carte est sous l'eau.
    pub fn find_spawn(&self) -> Option<(f32, f32)> {
        const STEP_M: f32 = 4.0;
        const MIN_HEIGHT_M: f32 = 2.0;
        const MAX_SLOPE: f32 = 0.5; // dénivelé / distance horizontale

        let half = self.config.half_size();
        let rings = (half / STEP_M) as i32;
        for ring in 0..rings {
            for (dx, dz) in square_ring(ring) {
                let (x, z) = (dx as f32 * STEP_M, dz as f32 * STEP_M);
                let h = self.height_at(x, z);
                let slope = [(1.0, 0.0), (0.0, 1.0)]
                    .iter()
                    .map(|(ox, oz)| (self.height_at(x + ox, z + oz) - h).abs())
                    .fold(0.0, f32::max);
                if h >= MIN_HEIGHT_M && slope <= MAX_SLOPE {
                    return Some((x, z));
                }
            }
        }
        None
    }

    /// Distance du premier point de terrain touché par un rayon (`direction` normalisée).
    ///
    /// Marche par pas de 25 cm puis affine par dichotomie. Gameplay uniquement
    /// (visée, placement) : ne sert pas à la génération.
    pub fn raycast(&self, origin: Vec3, direction: Vec3, max_distance: f32) -> Option<f32> {
        const STEP_M: f32 = 0.25;
        let below = |t: f32| {
            let p = origin + direction * t;
            p.y <= self.height_at(p.x, p.z)
        };
        if below(0.0) {
            return Some(0.0);
        }
        let mut previous = 0.0;
        let mut t = STEP_M;
        while t <= max_distance + STEP_M {
            let t_clamped = t.min(max_distance);
            if below(t_clamped) {
                let (mut lo, mut hi) = (previous, t_clamped);
                for _ in 0..12 {
                    let mid = (lo + hi) / 2.0;
                    if below(mid) {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                return Some(hi);
            }
            if t_clamped >= max_distance {
                break;
            }
            previous = t_clamped;
            t += STEP_M;
        }
        None
    }

    /// Indique si l'on peut marcher en ligne droite entre deux points sans eau profonde.
    pub fn walkable_line(&self, from: (f32, f32), to: (f32, f32)) -> bool {
        let (dx, dz) = (to.0 - from.0, to.1 - from.1);
        let steps = (dx.abs().max(dz.abs()).ceil() as usize).max(1);
        (0..=steps).all(|k| {
            let t = k as f32 / steps as f32;
            self.height_at(from.0 + dx * t, from.1 + dz * t) >= -MAX_WADING_DEPTH_M
        })
    }

    /// Empreinte des hauteurs (FNV-1a sur les bits), pour les tests de déterminisme.
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for h in &self.heights {
            for byte in h.to_bits().to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        }
        hash
    }
}

/// Élévation en mètres au point (x, z).
fn elevation_at(
    noise: &Perlin,
    offsets: &[(f64, f64)],
    config: &WorldGenConfig,
    x: f64,
    z: f64,
) -> f64 {
    // Bruit fractal (fBm) normalisé dans [-1, 1].
    let mut frequency = 1.0 / config.base_wavelength_m;
    let mut amplitude = 1.0;
    let (mut sum, mut total) = (0.0, 0.0);
    for &(ox, oz) in offsets {
        sum += amplitude * noise.sample(x * frequency + ox, z * frequency + oz);
        total += amplitude;
        frequency *= config.lacunarity;
        amplitude *= config.persistence;
    }
    let mut e = sum / total + config.land_bias;

    // Bande côtière : le terrain plonge vers le fond marin près des bords.
    let half = f64::from(config.half_size());
    let edge_distance = half - x.abs().max(z.abs());
    let mask = smoothstep(edge_distance / config.coast_width_m);
    e = config.border_elevation + (e - config.border_elevation) * mask;

    // e·|e| : plaines douces, reliefs plus marqués en altitude, symétrique sous la mer.
    e * e.abs() * config.max_height_m
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cases du carré de « rayon » `ring` autour de l'origine, dans un ordre fixe.
pub(crate) fn square_ring(ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut cells = Vec::with_capacity(8 * ring as usize);
    for k in -ring..ring {
        cells.push((k, -ring)); // bas, vers la droite
        cells.push((ring, k)); // droite, vers le haut
        cells.push((-k, ring)); // haut, vers la gauche
        cells.push((-ring, -k)); // gauche, vers le bas
    }
    cells
}

#[cfg(test)]
mod tests {
    use super::*;

    fn small_config() -> WorldGenConfig {
        WorldGenConfig {
            size_m: 128,
            ..WorldGenConfig::default()
        }
    }

    #[test]
    fn same_seed_and_config_give_same_terrain() {
        let config = small_config();
        let a = HeightField::generate(WorldSeed(1234), &config);
        let b = HeightField::generate(WorldSeed(1234), &config);
        assert_eq!(a.checksum(), b.checksum());
    }

    #[test]
    fn different_seeds_give_different_terrain() {
        let config = small_config();
        let a = HeightField::generate(WorldSeed(1), &config);
        let b = HeightField::generate(WorldSeed(2), &config);
        assert_ne!(a.checksum(), b.checksum());
    }

    /// Seed de référence : si ce test casse, le relief a changé.
    /// Vérifier que c'est voulu, incrémenter `WORLDGEN_VERSION`, puis mettre à jour la valeur.
    #[test]
    fn reference_seed_checksum() {
        let field = HeightField::generate(WorldSeed(42), &WorldGenConfig::default());
        assert_eq!(field.checksum(), REFERENCE_CHECKSUM_SEED_42);
    }

    /// Relief inchangé depuis la v1.
    const REFERENCE_CHECKSUM_SEED_42: u64 = 15_990_239_262_009_807_748;

    #[test]
    fn raycast_finds_the_ground_below() {
        let field = HeightField::generate(WorldSeed(42), &WorldGenConfig::default());
        let (x, z) = (10.0, -5.0);
        let ground = field.height_at(x, z);
        let origin = Vec3::new(x, ground + 5.0, z);
        let t = field
            .raycast(origin, Vec3::NEG_Y, 20.0)
            .expect("sol touché");
        assert!((t - 5.0).abs() < 0.01, "{t}");
        assert_eq!(
            field.raycast(origin, Vec3::NEG_Y, 3.0),
            None,
            "hors de portée"
        );
        assert_eq!(field.raycast(origin, Vec3::Y, 20.0), None, "vers le ciel");
    }

    #[test]
    fn grid_has_expected_size() {
        let field = HeightField::generate(WorldSeed(5), &small_config());
        assert_eq!(field.samples(), 129);
        assert_eq!(field.sample_position(0, 0), (-64.0, -64.0));
        assert_eq!(field.sample_position(128, 128), (64.0, 64.0));
    }

    #[test]
    fn border_is_under_the_sea() {
        let field = HeightField::generate(WorldSeed(77), &small_config());
        let last = field.samples() - 1;
        for k in 0..field.samples() {
            for (i, j) in [(k, 0), (k, last), (0, k), (last, k)] {
                assert!(field.sample(i, j) < 0.0, "bord émergé en ({i}, {j})");
            }
        }
    }

    #[test]
    fn height_at_matches_samples_on_grid_points() {
        let field = HeightField::generate(WorldSeed(3), &small_config());
        for (i, j) in [(0, 0), (10, 20), (64, 64), (127, 3), (128, 128)] {
            let (x, z) = field.sample_position(i, j);
            assert_eq!(field.height_at(x, z), field.sample(i, j));
        }
    }

    #[test]
    fn height_at_follows_the_mesh_diagonal() {
        let field = HeightField::generate(WorldSeed(3), &small_config());
        let (x, z) = field.sample_position(30, 40);
        let a = field.sample(30, 40);
        let d = field.sample(31, 41);
        // Le milieu de la diagonale est la moyenne de ses deux extrémités.
        let mid = field.height_at(x + 0.5, z + 0.5);
        assert!((mid - (a + d) / 2.0).abs() < 1e-4);
    }

    #[test]
    fn spawn_is_on_dry_land_for_many_seeds() {
        let config = WorldGenConfig::default();
        for seed in 0..12 {
            let field = HeightField::generate(WorldSeed(seed), &config);
            let (x, z) = field.find_spawn().expect("aucun point d'apparition");
            let half = config.half_size();
            assert!(x.abs() <= half && z.abs() <= half);
            assert!(field.height_at(x, z) >= 2.0, "seed {seed}");
        }
    }

    #[test]
    fn square_rings_cover_each_cell_once() {
        for ring in 0..5 {
            let cells = square_ring(ring);
            let expected = if ring == 0 { 1 } else { 8 * ring as usize };
            assert_eq!(cells.len(), expected);
            let mut sorted = cells.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), expected);
            assert!(cells.iter().all(|(x, z)| x.abs().max(z.abs()) == ring));
        }
    }
}
