//! Gisements de ressources : **réserve infinie, débit limité**.
//!
//! Un gisement n'a pas de stock : il ne s'épuise jamais. Ce qui limite la production,
//! c'est le débit de l'extracteur posé dessus, modulé par la pureté du gisement.
//!
//! Module de données (aucun ECS). Le placement fait partie de la génération du monde :
//! mêmes règles de déterminisme que `generation.rs`, figé par `reference_seed_deposits`.

use super::generation::{HeightField, WorldSeed, square_ring};
use super::noise::SplitMix64;
use super::rocks::RockField;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ResourceKind {
    Iron,
    Copper,
}

impl ResourceKind {
    /// Ressources qui doivent exister sur toute carte (contrôle après génération).
    pub const MANDATORY: [ResourceKind; 1] = [ResourceKind::Iron];

    pub fn name(self) -> &'static str {
        match self {
            ResourceKind::Iron => "fer",
            ResourceKind::Copper => "cuivre",
        }
    }
}

/// Pureté d'un gisement : multiplie le débit de l'extracteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Purity {
    Impure,
    Normal,
    Pure,
}

impl Purity {
    /// Multiplicateur de débit, en pourcentage (entier : pas d'arrondi flottant).
    pub fn rate_percent(self) -> u32 {
        match self {
            Purity::Impure => 50,
            Purity::Normal => 100,
            Purity::Pure => 200,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Purity::Impure => "impur",
            Purity::Normal => "normal",
            Purity::Pure => "pur",
        }
    }
}

/// Identifiant stable d'un gisement : son rang dans l'ordre de génération.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DepositId(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub struct Deposit {
    pub id: DepositId,
    pub kind: ResourceKind,
    pub purity: Purity,
    /// Centre du gisement : c'est là que l'extracteur s'aligne.
    pub x: f32,
    pub z: f32,
    /// Rayon de la zone visible et visable, en mètres.
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DepositConfig {
    /// Nombre de gisements visés par type, dans l'ordre de `ResourceKind::ALL`.
    pub targets: [(ResourceKind, u32); 2],
    /// Tirages essayés par type avant d'abandonner.
    pub attempts_per_kind: u32,
    pub radius_m: f32,
    /// Distance minimale aux bords de la carte.
    pub margin_m: f32,
    pub min_ground_m: f32,
    pub max_ground_m: f32,
    /// Demi-côté de l'emprise d'un extracteur, qui doit tenir sur un sol assez plat.
    pub footprint_half_m: f32,
    /// Dénivelé maximal dans l'emprise.
    pub flat_tolerance_m: f32,
    pub min_distance_same_kind_m: f32,
    pub min_distance_any_m: f32,
    /// Distance libre autour des rochers (en plus de leur rayon).
    pub rock_clearance_m: f32,
    pub spawn_clearance_m: f32,
    /// Un gisement de fer doit exister dans ce rayon autour du point d'apparition…
    pub starter_radius_m: f32,
    /// … mais pas plus près que ça.
    pub starter_min_m: f32,
}

impl Default for DepositConfig {
    fn default() -> Self {
        Self {
            targets: [(ResourceKind::Iron, 8), (ResourceKind::Copper, 5)],
            attempts_per_kind: 400,
            radius_m: 3.0,
            margin_m: 24.0,
            min_ground_m: 1.5,
            max_ground_m: 45.0,
            footprint_half_m: 1.5,
            flat_tolerance_m: 0.6,
            min_distance_same_kind_m: 70.0,
            min_distance_any_m: 30.0,
            rock_clearance_m: 3.0,
            spawn_clearance_m: 12.0,
            starter_radius_m: 64.0,
            starter_min_m: 14.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DepositField {
    deposits: Vec<Deposit>,
    /// Gisement de fer garanti près du point d'apparition.
    starter: Option<DepositId>,
}

impl DepositField {
    pub fn generate(
        seed: WorldSeed,
        field: &HeightField,
        rocks: &RockField,
        config: &DepositConfig,
    ) -> Self {
        let spawn = field.find_spawn().unwrap_or((0.0, 0.0));
        let half = field.config().half_size();
        let span = 2.0 * (half - config.margin_m);
        let mut rng = SplitMix64::new(seed.0 ^ 0xDE90_5175_0BE5_1234);
        let mut sites: Vec<(ResourceKind, Purity, f32, f32)> = Vec::new();

        for (kind, target) in config.targets {
            let mut placed = 0;
            for _ in 0..config.attempts_per_kind {
                // Toujours 3 tirages par essai.
                let (u, v, p) = (rng.next_f64() as f32, rng.next_f64() as f32, rng.next_f64());
                if placed >= target {
                    continue;
                }
                let x = u * span - span / 2.0;
                let z = v * span - span / 2.0;
                let far_from_spawn =
                    dist_sq((x, z), spawn) >= config.spawn_clearance_m * config.spawn_clearance_m;
                if far_from_spawn && site_ok(field, rocks, &sites, kind, x, z, config, true) {
                    let purity = match p {
                        p if p < 0.3 => Purity::Impure,
                        p if p < 0.85 => Purity::Normal,
                        _ => Purity::Pure,
                    };
                    sites.push((kind, purity, x, z));
                    placed += 1;
                }
            }
        }

        let mut deposits: Vec<Deposit> = sites
            .into_iter()
            .enumerate()
            .map(|(i, (kind, purity, x, z))| Deposit {
                id: DepositId(i as u32),
                kind,
                purity,
                x,
                z,
                radius: config.radius_m,
            })
            .collect();

        // Garantie de démarrage : un fer accessible à pied près du spawn, sinon on en crée un.
        let starter = nearest_reachable_iron(field, &deposits, spawn, config).or_else(|| {
            let (x, z) = find_starter_site(field, rocks, &deposits, spawn, config)?;
            let id = DepositId(deposits.len() as u32);
            deposits.push(Deposit {
                id,
                kind: ResourceKind::Iron,
                purity: Purity::Normal,
                x,
                z,
                radius: config.radius_m,
            });
            Some(id)
        });

        Self { deposits, starter }
    }

    pub fn all(&self) -> &[Deposit] {
        &self.deposits
    }

    pub fn get(&self, id: DepositId) -> Option<&Deposit> {
        self.deposits.get(id.0 as usize)
    }

    pub fn starter(&self) -> Option<DepositId> {
        self.starter
    }

    /// Gisement dont la zone contient le point (x, z).
    pub fn find_at(&self, x: f32, z: f32) -> Option<&Deposit> {
        self.deposits
            .iter()
            .find(|d| dist_sq((x, z), (d.x, d.z)) <= d.radius * d.radius)
    }

    /// Contrôle après génération : ressources obligatoires et gisement de démarrage.
    pub fn check_quotas(&self) -> Result<(), String> {
        for kind in ResourceKind::MANDATORY {
            if !self.deposits.iter().any(|d| d.kind == kind) {
                return Err(format!("aucun gisement de {}", kind.name()));
            }
        }
        if self.starter.is_none() {
            return Err("aucun gisement de fer accessible près du point d'apparition".into());
        }
        Ok(())
    }

    pub fn checksum(&self) -> u64 {
        let mut hash: u64 = 0xCBF2_9CE4_8422_2325;
        let mut feed = |bytes: &[u8]| {
            for &byte in bytes {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01B3);
            }
        };
        for d in &self.deposits {
            feed(&[d.kind as u8, d.purity as u8]);
            feed(&d.x.to_bits().to_le_bytes());
            feed(&d.z.to_bits().to_le_bytes());
        }
        feed(&self.starter.map_or(u32::MAX, |id| id.0).to_le_bytes());
        hash
    }
}

fn dist_sq(a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dz) = (a.0 - b.0, a.1 - b.1);
    dx * dx + dz * dz
}

/// Le site convient-il à un gisement (et donc à un extracteur) ?
fn site_ok(
    field: &HeightField,
    rocks: &RockField,
    existing: &[(ResourceKind, Purity, f32, f32)],
    kind: ResourceKind,
    x: f32,
    z: f32,
    config: &DepositConfig,
    check_same_kind: bool,
) -> bool {
    let ground = field.height_at(x, z);
    if !(config.min_ground_m..=config.max_ground_m).contains(&ground) {
        return false;
    }
    // Emprise de l'extracteur assez plate : 9 points sur un carré.
    let h = config.footprint_half_m;
    let samples = [-h, 0.0, h].into_iter().flat_map(|ox| {
        [-h, 0.0, h]
            .into_iter()
            .map(move |oz| field.height_at(x + ox, z + oz))
    });
    let (lo, hi) = samples.fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(v), hi.max(v)));
    if hi - lo > config.flat_tolerance_m {
        return false;
    }
    let spaced = existing.iter().all(|&(other, _, ox, oz)| {
        let min = if check_same_kind && other == kind {
            config.min_distance_same_kind_m
        } else {
            config.min_distance_any_m
        };
        dist_sq((x, z), (ox, oz)) >= min * min
    });
    let clear_of_rocks = rocks.all().iter().all(|rock| {
        let min = rock.radius + config.radius_m + config.rock_clearance_m;
        dist_sq((x, z), (rock.x, rock.z)) >= min * min
    });
    spaced && clear_of_rocks
}

fn nearest_reachable_iron(
    field: &HeightField,
    deposits: &[Deposit],
    spawn: (f32, f32),
    config: &DepositConfig,
) -> Option<DepositId> {
    let max_sq = config.starter_radius_m * config.starter_radius_m;
    deposits
        .iter()
        .filter(|d| d.kind == ResourceKind::Iron)
        .map(|d| (d, dist_sq((d.x, d.z), spawn)))
        .filter(|&(d, dist)| dist <= max_sq && field.walkable_line(spawn, (d.x, d.z)))
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(d, _)| d.id)
}

/// Cherche en spirale autour du spawn un site de fer valide et accessible à pied.
fn find_starter_site(
    field: &HeightField,
    rocks: &RockField,
    deposits: &[Deposit],
    spawn: (f32, f32),
    config: &DepositConfig,
) -> Option<(f32, f32)> {
    const STEP_M: f32 = 4.0;
    let existing: Vec<_> = deposits
        .iter()
        .map(|d| (d.kind, d.purity, d.x, d.z))
        .collect();
    let first_ring = (config.starter_min_m / STEP_M).ceil() as i32;
    let last_ring = (config.starter_radius_m / STEP_M) as i32;
    (first_ring..=last_ring)
        .flat_map(square_ring)
        .map(|(dx, dz)| (spawn.0 + dx as f32 * STEP_M, spawn.1 + dz as f32 * STEP_M))
        .find(|&(x, z)| {
            // Distance réelle (le carré déborde du cercle dans les coins).
            dist_sq((x, z), spawn) <= config.starter_radius_m * config.starter_radius_m
                && site_ok(
                    field,
                    rocks,
                    &existing,
                    ResourceKind::Iron,
                    x,
                    z,
                    config,
                    false,
                )
                && field.walkable_line(spawn, (x, z))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::generation::WorldGenConfig;
    use crate::world::rocks::RockConfig;

    fn world(seed: u64) -> (HeightField, RockField, DepositField) {
        let field = HeightField::generate(WorldSeed(seed), &WorldGenConfig::default());
        let rocks = RockField::generate(WorldSeed(seed), &field, &RockConfig::default());
        let deposits =
            DepositField::generate(WorldSeed(seed), &field, &rocks, &DepositConfig::default());
        (field, rocks, deposits)
    }

    #[test]
    fn same_seed_gives_same_deposits() {
        let (_, _, a) = world(42);
        let (_, _, b) = world(42);
        assert_eq!(a.all(), b.all());
        assert_eq!(a.starter(), b.starter());
    }

    /// Seed de référence : si ce test casse, le placement des gisements a changé.
    /// Vérifier que c'est voulu, incrémenter `WORLDGEN_VERSION`, puis mettre à jour.
    #[test]
    fn reference_seed_deposits() {
        let (_, _, deposits) = world(42);
        assert_eq!(
            (deposits.all().len(), deposits.checksum()),
            REFERENCE_DEPOSITS_SEED_42
        );
    }

    /// Placement introduit en v2.
    const REFERENCE_DEPOSITS_SEED_42: (usize, u64) = (13, 9_833_645_354_855_604_123);

    #[test]
    fn quotas_and_starter_hold_for_many_seeds() {
        for seed in 0..24 {
            let (field, _, deposits) = world(seed);
            deposits
                .check_quotas()
                .unwrap_or_else(|e| panic!("seed {seed} : {e}"));
            let starter = deposits.get(deposits.starter().unwrap()).unwrap();
            assert_eq!(starter.kind, ResourceKind::Iron);
            let spawn = field.find_spawn().unwrap();
            assert!(
                dist_sq((starter.x, starter.z), spawn) <= 64.0 * 64.0,
                "seed {seed}"
            );
            assert!(
                field.walkable_line(spawn, (starter.x, starter.z)),
                "seed {seed}"
            );
        }
    }

    #[test]
    fn deposits_respect_terrain_spacing_and_rocks() {
        let config = DepositConfig::default();
        for seed in [1, 2, 3, 42] {
            let (field, rocks, deposits) = world(seed);
            let half = field.config().half_size();
            for d in deposits.all() {
                let ground = field.height_at(d.x, d.z);
                assert!(ground >= config.min_ground_m, "gisement dans l'eau");
                assert!(d.x.abs() <= half && d.z.abs() <= half);
                for rock in rocks.all() {
                    let min = rock.radius + config.radius_m;
                    assert!(dist_sq((d.x, d.z), (rock.x, rock.z)) >= min * min);
                }
                for other in deposits.all().iter().filter(|o| o.id != d.id) {
                    let dist = dist_sq((d.x, d.z), (other.x, other.z)).sqrt();
                    assert!(dist >= config.min_distance_any_m, "{dist} m");
                }
                assert_eq!(deposits.get(d.id), Some(d));
            }
        }
    }

    #[test]
    fn find_at_uses_the_deposit_radius() {
        let (_, _, deposits) = world(42);
        let d = &deposits.all()[0];
        assert_eq!(deposits.find_at(d.x + 2.9, d.z).map(|f| f.id), Some(d.id));
        assert_eq!(deposits.find_at(d.x + 3.1, d.z).map(|f| f.id), None);
    }

    #[test]
    fn purity_scales_the_rate() {
        assert_eq!(Purity::Impure.rate_percent(), 50);
        assert_eq!(Purity::Normal.rate_percent(), 100);
        assert_eq!(Purity::Pure.rate_percent(), 200);
    }
}
