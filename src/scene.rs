//! Scène de démonstration standard (`--demo`), en Rust pur.
//!
//! Construit près du point d'apparition une petite usine complète et automatique :
//! extracteur (gisement de départ) → convoyeur → four → convoyeur → hub, alimentée par
//! deux générateurs via un poteau, avec des modules de vitesse et d'efficacité.
//!
//! La scène passe par **les mêmes règles de placement que le joueur** (`placement.rs`) :
//! si une disposition est refusée (rocher, pente…), la suivante est essayée.

use bevy::math::{Quat, Vec3};

use crate::building::placement::{PlacedBuildings, PlacementContext, preview, preview_conveyor};
use crate::simulation::modules::ModuleKind;
use crate::simulation::sim_world::{BuildingId, BuildingKind, SimWorld};
use crate::world::deposits::DepositField;
use crate::world::generation::HeightField;
use crate::world::rocks::RockField;

/// Bilan de la scène construite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DemoScene {
    pub buildings: usize,
    pub conveyors: usize,
    pub modules: usize,
}

/// Construit la scène dans `sim` et `placed`. En cas d'échec, rien n'est modifié.
pub fn build_demo(
    field: &HeightField,
    deposits: &DepositField,
    rocks: &RockField,
    sim: &mut SimWorld,
    placed: &mut PlacedBuildings,
) -> Result<DemoScene, String> {
    let starter = deposits
        .starter()
        .and_then(|id| deposits.get(id))
        .ok_or("pas de gisement de départ")?;
    let center = Vec3::new(starter.x, 0.0, starter.z);

    // 4 orientations × 2 côtés (générateurs à droite ou à gauche de la chaîne).
    for rotation in 0..4u8 {
        for side in [1.0, -1.0] {
            let mut trial_sim = sim.clone();
            let mut trial_placed = placed.clone();
            if let Some(scene) = try_layout(
                field,
                deposits,
                rocks,
                &mut trial_sim,
                &mut trial_placed,
                center,
                rotation,
                side,
            ) {
                *sim = trial_sim;
                *placed = trial_placed;
                return Ok(scene);
            }
        }
    }
    Err("aucune disposition valide autour du gisement de départ".into())
}

/// Essaie une disposition ; `None` dès qu'une étape est refusée.
#[allow(clippy::too_many_arguments)]
fn try_layout(
    field: &HeightField,
    deposits: &DepositField,
    rocks: &RockField,
    sim: &mut SimWorld,
    placed: &mut PlacedBuildings,
    center: Vec3,
    rotation: u8,
    side: f32,
) -> Option<DemoScene> {
    let turn = Quat::from_rotation_y(f32::from(rotation) * std::f32::consts::FRAC_PI_2);
    // La sortie d'un bâtiment à cette rotation regarde vers `forward`.
    let forward = turn * Vec3::NEG_Z;
    let right = turn * Vec3::X * side;

    let layout = [
        (BuildingKind::Extractor, center),
        (BuildingKind::Furnace, center + forward * 7.0),
        (BuildingKind::Hub, center + forward * 14.0),
        (BuildingKind::Pole, center + forward * 7.0 + right * 4.0),
        (
            BuildingKind::Generator,
            center + forward * 7.0 + right * 8.0,
        ),
        (
            BuildingKind::Generator,
            center + forward * 7.0 + right * 12.0,
        ),
    ];

    let mut ids = Vec::new();
    for (kind, position) in layout {
        let aim = Vec3::new(
            position.x,
            field.height_at(position.x, position.z),
            position.z,
        );
        let ctx = PlacementContext {
            field,
            deposits,
            rocks,
            sim,
            placed,
            // Le joueur n'est pas pris en compte : il apparaît à l'écart.
            player: (Vec3::splat(f32::MAX), 0.0),
        };
        let p = preview(kind, aim, rotation, &ctx);
        p.result.ok()?;
        let id = match kind {
            BuildingKind::Extractor => sim.add_extractor(deposits.get(p.deposit?)?),
            _ => sim.add_building(kind, p.building.x, p.building.z)?,
        };
        placed.insert(id, p.building);
        ids.push(id);
    }
    let (extractor, furnace, hub) = (ids[0], ids[1], ids[2]);

    let mut conveyors = 0;
    for (from, to) in [(extractor, furnace), (furnace, hub)] {
        connect(field, sim, placed, from, to)?;
        conveyors += 1;
    }

    // Vitesse + efficacité sur les deux machines : 45/min chacune, 39 + 65 kW pour 200 kW.
    let mut modules = 0;
    for host in [extractor, furnace] {
        for (socket, kind) in [(0, ModuleKind::Speed), (1, ModuleKind::Efficiency)] {
            sim.attach_module(host, socket, kind).ok()?;
            modules += 1;
        }
    }

    Some(DemoScene {
        buildings: ids.len(),
        conveyors,
        modules,
    })
}

fn connect(
    field: &HeightField,
    sim: &mut SimWorld,
    placed: &PlacedBuildings,
    from: BuildingId,
    to: BuildingId,
) -> Option<BuildingId> {
    let p = preview_conveyor(from, Some(to), Vec3::ZERO, sim, placed, field)?;
    p.result.ok()?;
    sim.add_conveyor(from, to, p.start.distance(p.end)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemKind;
    use crate::simulation::production::TICKS_PER_MINUTE;
    use crate::world::deposits::DepositConfig;
    use crate::world::generation::{WorldGenConfig, WorldSeed};
    use crate::world::rocks::RockConfig;

    fn world(seed: u64) -> (HeightField, RockField, DepositField) {
        let seed = WorldSeed(seed);
        let field = HeightField::generate(seed, &WorldGenConfig::default());
        let rocks = RockField::generate(seed, &field, &RockConfig::default());
        let deposits = DepositField::generate(seed, &field, &rocks, &DepositConfig::default());
        (field, rocks, deposits)
    }

    #[test]
    fn demo_scene_fits_on_most_maps() {
        let built = (0..16)
            .filter(|&seed| {
                let (field, rocks, deposits) = world(seed);
                let mut sim = SimWorld::default();
                let mut placed = PlacedBuildings::default();
                build_demo(&field, &deposits, &rocks, &mut sim, &mut placed).is_ok()
            })
            .count();
        assert!(built >= 14, "scène construite sur {built}/16 cartes");
    }

    /// Scène test standard : 5 minutes simulées sans fenêtre, tout doit arriver au hub.
    #[test]
    fn demo_scene_delivers_plates_unattended() {
        let (field, rocks, deposits) = world(42);
        let mut sim = SimWorld::default();
        let mut placed = PlacedBuildings::default();
        let scene = build_demo(&field, &deposits, &rocks, &mut sim, &mut placed).unwrap();
        assert_eq!(
            scene,
            DemoScene {
                buildings: 6,
                conveyors: 2,
                modules: 4
            }
        );
        for _ in 0..5 * TICKS_PER_MINUTE {
            sim.step();
        }
        // 45 plaques/min au régime établi, moins le temps de mise en route.
        let plates = sim.deliveries().total(ItemKind::IronPlate);
        assert!((190..=225).contains(&plates), "{plates} plaques livrées");
        assert_eq!(
            sim.deliveries().goals_reached(),
            1,
            "50 plaques de fer : atteint"
        );
    }

    #[test]
    fn a_failed_attempt_changes_nothing() {
        let (field, rocks, deposits) = world(42);
        let mut sim = SimWorld::default();
        let mut placed = PlacedBuildings::default();
        build_demo(&field, &deposits, &rocks, &mut sim, &mut placed).unwrap();
        // Une seconde scène ne trouve plus de place (gisement occupé) : rien ne bouge.
        let before = sim.buildings().count();
        assert!(build_demo(&field, &deposits, &rocks, &mut sim, &mut placed).is_err());
        assert_eq!(sim.buildings().count(), before);
    }
}
