//! Rochers destructibles : des objets distincts du terrain, qui ne le modifient jamais.
//!
//! Module de données (aucun ECS) : placement déterministe, obstacles,
//! visée et destruction. L'état persistant est l'ensemble des rochers détruits,
//! repérés par un identifiant stable (V0.1 : sauvegarde).
//!
//! Le placement suit les mêmes règles de déterminisme que `generation.rs`
//! (pas de `sqrt`, `sin`… dans le calcul des positions) : il fait partie de
//! `WORLDGEN_VERSION`. Les collisions et la visée, elles, sont du gameplay et
//! peuvent utiliser `sqrt` librement.

use std::collections::BTreeSet;

use bevy::math::Vec3;

use super::generation::{HeightField, WorldSeed};
use super::noise::SplitMix64;
use crate::collision::Obstacle;

/// Identifiant stable d'un rocher : son rang dans l'ordre de génération.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RockId(pub u32);

/// Un rocher : ellipsoïde de révolution posé (et un peu enfoncé) sur le terrain.
#[derive(Debug, Clone, PartialEq)]
pub struct Rock {
    pub id: RockId,
    /// Centre horizontal.
    pub x: f32,
    pub z: f32,
    /// Bas de l'ellipsoïde (sous la surface du sol).
    pub base: f32,
    /// Rayon horizontal, en mètres.
    pub radius: f32,
    /// Hauteur totale, en mètres.
    pub height: f32,
    /// Orientation autour de Y, pour varier l'aspect, en radians.
    pub yaw: f32,
}

impl Rock {
    pub fn center(&self) -> Vec3 {
        Vec3::new(self.x, self.base + self.height / 2.0, self.z)
    }

    pub fn top(&self) -> f32 {
        self.base + self.height
    }

    /// Demi-axes de l'ellipsoïde.
    pub fn half_extents(&self) -> Vec3 {
        Vec3::new(self.radius, self.height / 2.0, self.radius)
    }
}

/// Règles de placement des rochers.
#[derive(Debug, Clone, PartialEq)]
pub struct RockConfig {
    /// Côté des cases de la grille de placement (un rocher au plus par case), en mètres.
    pub cell_m: f32,
    /// Probabilité qu'une case reçoive un rocher.
    pub chance: f64,
    pub min_radius_m: f32,
    pub max_radius_m: f32,
    /// Hauteur minimale du sol au-dessus de la mer.
    pub min_ground_m: f32,
    /// Pente maximale (dénivelé par mètre).
    pub max_slope: f32,
    /// Rayon autour du point d'apparition laissé libre.
    pub spawn_clearance_m: f32,
}

impl Default for RockConfig {
    fn default() -> Self {
        Self {
            cell_m: 16.0,
            chance: 0.3,
            min_radius_m: 0.5,
            max_radius_m: 2.0,
            min_ground_m: 1.0,
            max_slope: 0.6,
            spawn_clearance_m: 8.0,
        }
    }
}

/// Tous les rochers du monde et ceux qui ont été détruits.
#[derive(Debug, Clone)]
pub struct RockField {
    rocks: Vec<Rock>,
    destroyed: BTreeSet<RockId>,
}

impl RockField {
    pub fn generate(seed: WorldSeed, field: &HeightField, config: &RockConfig) -> Self {
        let half = field.config().half_size();
        let cells = (field.config().size_m as f32 / config.cell_m) as u32;
        let spawn = field.find_spawn();
        let mut rng = SplitMix64::new(seed.0 ^ 0x0C0C_4B1D_5EED_F00D);
        let mut rocks = Vec::new();

        for cz in 0..cells {
            for cx in 0..cells {
                // Toujours 6 tirages par case : changer une règle de rejet ne décale
                // pas les rochers des autres cases.
                let roll = rng.next_f64();
                let (u, v) = (rng.next_f64() as f32, rng.next_f64() as f32);
                let size = rng.next_f64() as f32;
                let aspect = rng.next_f64() as f32;
                let yaw = rng.next_f64() as f32 * std::f32::consts::TAU;
                if roll >= config.chance {
                    continue;
                }

                let x = (cx as f32 + u) * config.cell_m - half;
                let z = (cz as f32 + v) * config.cell_m - half;
                let ground = field.height_at(x, z);
                let slope = [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)]
                    .iter()
                    .map(|(ox, oz)| (field.height_at(x + ox, z + oz) - ground).abs())
                    .fold(0.0, f32::max);
                let near_spawn = spawn.is_some_and(|(sx, sz)| {
                    let (dx, dz) = (x - sx, z - sz);
                    dx * dx + dz * dz < config.spawn_clearance_m * config.spawn_clearance_m
                });
                if ground < config.min_ground_m || slope > config.max_slope || near_spawn {
                    continue;
                }

                // Beaucoup de petits rochers, quelques gros.
                let radius =
                    config.min_radius_m + (config.max_radius_m - config.min_radius_m) * size * size;
                let height = radius * (0.8 + 0.6 * aspect);
                rocks.push(Rock {
                    id: RockId(rocks.len() as u32),
                    x,
                    z,
                    // Enfoncé d'un quart de sa hauteur pour ne pas flotter sur les pentes.
                    base: ground - height * 0.25,
                    radius,
                    height,
                    yaw,
                });
            }
        }

        Self {
            rocks,
            destroyed: BTreeSet::new(),
        }
    }

    /// Tous les rochers générés, détruits compris.
    pub fn all(&self) -> &[Rock] {
        &self.rocks
    }

    /// Rochers encore présents.
    pub fn live(&self) -> impl Iterator<Item = &Rock> {
        self.rocks
            .iter()
            .filter(|rock| !self.destroyed.contains(&rock.id))
    }

    pub fn live_count(&self) -> usize {
        self.rocks.len() - self.destroyed.len()
    }

    pub fn get(&self, id: RockId) -> Option<&Rock> {
        self.rocks.get(id.0 as usize)
    }

    /// Identifiants des rochers détruits : c'est l'état à sauvegarder.
    pub fn destroyed(&self) -> &BTreeSet<RockId> {
        &self.destroyed
    }

    /// Détruit un rocher. Retourne `false` s'il n'existe pas ou était déjà détruit.
    pub fn destroy(&mut self, id: RockId) -> bool {
        self.get(id).is_some() && self.destroyed.insert(id)
    }

    /// Rochers présents, vus comme obstacles cylindriques pour les collisions.
    pub fn obstacles(&self) -> impl Iterator<Item = Obstacle> + '_ {
        self.live().map(|rock| Obstacle::Circle {
            x: rock.x,
            z: rock.z,
            radius: rock.radius,
            top: rock.top(),
        })
    }

    /// Premier rocher touché par un rayon, à moins de `max_distance` mètres.
    /// `direction` doit être normalisée. Retourne l'identifiant et la distance.
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(RockId, f32)> {
        self.live()
            .filter_map(|rock| {
                ray_ellipsoid(origin, direction, rock.center(), rock.half_extents())
                    .map(|t| (rock.id, t))
            })
            .filter(|&(_, t)| t <= max_distance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Empreinte des rochers générés, pour les tests de déterminisme.
    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        for rock in &self.rocks {
            for value in [
                rock.x,
                rock.z,
                rock.base,
                rock.radius,
                rock.height,
                rock.yaw,
            ] {
                for byte in value.to_bits().to_le_bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
                }
            }
        }
        hash
    }
}

/// Distance d'intersection d'un rayon avec un ellipsoïde aligné sur les axes.
/// On se ramène à une sphère unité en divisant par les demi-axes.
fn ray_ellipsoid(origin: Vec3, direction: Vec3, center: Vec3, half_extents: Vec3) -> Option<f32> {
    let o = (origin - center) / half_extents;
    let d = direction / half_extents;
    let a = d.dot(d);
    let b = 2.0 * o.dot(d);
    let c = o.dot(o) - 1.0;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let root = discriminant.sqrt();
    let near = (-b - root) / (2.0 * a);
    let far = (-b + root) / (2.0 * a);
    // Si l'origine est à l'intérieur, `near` est négatif : on prend la sortie.
    [near, far].into_iter().find(|t| *t >= 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::WorldGenConfig;

    fn world(seed: u64) -> (HeightField, RockField) {
        let field = HeightField::generate(WorldSeed(seed), &WorldGenConfig::default());
        let rocks = RockField::generate(WorldSeed(seed), &field, &RockConfig::default());
        (field, rocks)
    }

    /// Un seul rocher de 1 m de rayon et 2 m de haut, posé au sol en (0, 0).
    fn single_rock() -> RockField {
        RockField {
            rocks: vec![Rock {
                id: RockId(0),
                x: 0.0,
                z: 0.0,
                base: 0.0,
                radius: 1.0,
                height: 2.0,
                yaw: 0.0,
            }],
            destroyed: BTreeSet::new(),
        }
    }

    #[test]
    fn same_seed_gives_same_rocks() {
        let (_, a) = world(42);
        let (_, b) = world(42);
        assert_eq!(a.all(), b.all());
        assert!(a.all().len() > 50, "{} rochers seulement", a.all().len());
    }

    /// Seed de référence : si ce test casse, le placement des rochers a changé.
    /// Vérifier que c'est voulu, incrémenter `WORLDGEN_VERSION`, puis mettre à jour.
    #[test]
    fn reference_seed_rocks() {
        let (_, rocks) = world(42);
        assert_eq!(
            (rocks.all().len(), rocks.checksum()),
            REFERENCE_ROCKS_SEED_42
        );
    }

    /// Placement inchangé depuis la v1.
    const REFERENCE_ROCKS_SEED_42: (usize, u64) = (87, 11_393_642_858_685_267_379);

    #[test]
    fn rocks_are_on_dry_gentle_ground_and_away_from_spawn() {
        for seed in [1, 2, 3, 42] {
            let (field, rocks) = world(seed);
            let (sx, sz) = field.find_spawn().unwrap();
            for rock in rocks.all() {
                assert!(field.height_at(rock.x, rock.z) >= 1.0);
                let (dx, dz) = (rock.x - sx, rock.z - sz);
                assert!(
                    dx * dx + dz * dz >= 64.0,
                    "rocher {:?} trop près du spawn",
                    rock.id
                );
                assert_eq!(rocks.get(rock.id), Some(rock));
            }
        }
    }

    #[test]
    fn destroying_is_idempotent_and_leaves_the_terrain_untouched() {
        let (field, mut rocks) = world(42);
        let terrain_before = field.checksum();
        let total = rocks.live_count();

        assert!(rocks.destroy(RockId(3)));
        assert!(!rocks.destroy(RockId(3)), "déjà détruit");
        assert!(!rocks.destroy(RockId(999_999)), "inexistant");
        assert_eq!(rocks.live_count(), total - 1);
        assert!(rocks.live().all(|rock| rock.id != RockId(3)));
        assert_eq!(
            rocks.destroyed().iter().copied().collect::<Vec<_>>(),
            [RockId(3)]
        );
        // Le terrain est une donnée séparée : il ne bouge pas.
        assert_eq!(field.checksum(), terrain_before);
    }

    #[test]
    fn destroyed_rocks_are_no_longer_obstacles() {
        let mut rocks = single_rock();
        assert_eq!(rocks.obstacles().count(), 1);
        rocks.destroy(RockId(0));
        assert_eq!(rocks.obstacles().count(), 0);
        assert_eq!(
            rocks.raycast(Vec3::new(-5.0, 1.0, 0.0), Vec3::X, 10.0),
            None
        );
    }

    #[test]
    fn raycast_hits_the_rock_surface_within_reach() {
        let rocks = single_rock();
        let origin = Vec3::new(-5.0, 1.0, 0.0);
        let (id, distance) = rocks.raycast(origin, Vec3::X, 10.0).expect("touché");
        assert_eq!(id, RockId(0));
        assert!((distance - 4.0).abs() < 1e-4, "{distance}");
        assert_eq!(rocks.raycast(origin, Vec3::X, 3.0), None, "trop loin");
        assert_eq!(
            rocks.raycast(origin, Vec3::NEG_X, 10.0),
            None,
            "mauvaise direction"
        );
    }
}
