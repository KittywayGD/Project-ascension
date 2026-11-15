//! Règles de placement des bâtiments, en Rust pur.
//!
//! Règle retenue pour le terrain irrégulier : le bâtiment repose sur une **fondation
//! automatique** qui va du point le plus bas de son emprise jusqu'à un plateau posé
//! juste au-dessus du point le plus haut. Hors gisement, la position est calée sur une
//! grille horizontale de 1 m ; un extracteur, lui, s'aligne sur le centre du gisement.

use std::collections::BTreeMap;

use bevy::math::{Quat, Vec3};

use crate::collision::{Obstacle, ray_aabb};
use crate::simulation::energy::POLE_RANGE_M;
use crate::simulation::sim_world::{BuildingId, BuildingKind, ConveyorError, SimWorld};
use crate::world::deposits::{DepositField, DepositId};
use crate::world::generation::HeightField;
use crate::world::rocks::RockField;

/// Pas de la grille de placement hors gisement.
pub const GRID_M: f32 = 1.0;
/// Hauteur maximale d'une fondation (au-delà, le terrain est jugé trop pentu).
pub const MAX_FOUNDATION_M: f32 = 2.5;

/// Encombrement d'un type de bâtiment.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dims {
    /// Demi-côté de l'emprise carrée.
    pub half: f32,
    /// Hauteur au-dessus du plateau (sommet le plus haut).
    pub height: f32,
    /// Hauteur d'accroche des câbles au-dessus du plateau.
    pub cable_height: f32,
}

pub fn dims(kind: BuildingKind) -> Dims {
    match kind {
        BuildingKind::Extractor => Dims {
            half: 1.5,
            height: 2.3,
            cable_height: 1.45,
        },
        BuildingKind::Generator => Dims {
            half: 1.5,
            height: 2.4,
            cable_height: 2.25,
        },
        BuildingKind::Furnace => Dims {
            half: 1.5,
            height: 3.2,
            cable_height: 2.05,
        },
        BuildingKind::Storage => Dims {
            half: 1.0,
            height: 1.4,
            cable_height: 1.2,
        },
        BuildingKind::Pole => Dims {
            half: 0.3,
            height: 4.2,
            cable_height: 4.0,
        },
    }
}

/// Position locale de la sortie (face où un convoyeur se branche), à rotation 0.
pub fn output_port(kind: BuildingKind) -> Option<Vec3> {
    match kind {
        BuildingKind::Extractor => Some(Vec3::new(0.0, 0.6, -1.5)),
        BuildingKind::Furnace => Some(Vec3::new(1.5, 0.7, 0.0)),
        BuildingKind::Storage => Some(Vec3::new(0.0, 0.5, -1.0)),
        BuildingKind::Generator | BuildingKind::Pole => None,
    }
}

/// Position locale de l'entrée, à rotation 0.
pub fn input_port(kind: BuildingKind) -> Option<Vec3> {
    match kind {
        BuildingKind::Furnace => Some(Vec3::new(0.0, 0.7, 1.5)),
        BuildingKind::Storage => Some(Vec3::new(0.0, 0.5, 1.0)),
        _ => None,
    }
}

/// Un bâtiment posé dans le monde : tout ce qu'il faut pour l'afficher et le sauvegarder.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacedBuilding {
    pub kind: BuildingKind,
    pub x: f32,
    pub z: f32,
    /// Hauteur du plateau de la fondation, sur lequel repose la machine.
    pub base_y: f32,
    /// Bas de la fondation (sous le point le plus bas du terrain).
    pub foundation_bottom: f32,
    /// Orientation en quarts de tour (0 à 3) ; la face avant regarde vers -Z à 0.
    pub rotation: u8,
}

impl PlacedBuilding {
    pub fn dims(&self) -> Dims {
        dims(self.kind)
    }

    pub fn obstacle(&self) -> Obstacle {
        let Dims { half, height, .. } = self.dims();
        Obstacle::Rect {
            min_x: self.x - half,
            max_x: self.x + half,
            min_z: self.z - half,
            max_z: self.z + half,
            top: self.base_y + height,
        }
    }

    pub fn bounds(&self) -> (Vec3, Vec3) {
        let Dims { half, height, .. } = self.dims();
        (
            Vec3::new(self.x - half, self.foundation_bottom, self.z - half),
            Vec3::new(self.x + half, self.base_y + height, self.z + half),
        )
    }

    /// Position dans le monde d'un point exprimé dans le repère du bâtiment.
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        let rotation =
            Quat::from_rotation_y(f32::from(self.rotation) * std::f32::consts::FRAC_PI_2);
        Vec3::new(self.x, self.base_y, self.z) + rotation * local
    }

    pub fn output_world(&self) -> Option<Vec3> {
        output_port(self.kind).map(|p| self.to_world(p))
    }

    pub fn input_world(&self) -> Option<Vec3> {
        input_port(self.kind).map(|p| self.to_world(p))
    }

    /// Point d'accroche des câbles électriques.
    pub fn cable_anchor(&self) -> Vec3 {
        Vec3::new(self.x, self.base_y + self.dims().cable_height, self.z)
    }

    /// Les emprises se chevauchent-elles (se toucher est permis) ?
    fn overlaps(&self, other: &PlacedBuilding) -> bool {
        let (a, b) = (self.dims().half, other.dims().half);
        (self.x - other.x).abs() < a + b && (self.z - other.z).abs() < a + b
    }
}

/// Tous les bâtiments posés, indexés par leur identifiant de simulation.
#[derive(Debug, Default)]
pub struct PlacedBuildings {
    buildings: BTreeMap<BuildingId, PlacedBuilding>,
}

impl PlacedBuildings {
    pub fn insert(&mut self, id: BuildingId, building: PlacedBuilding) {
        self.buildings.insert(id, building);
    }

    pub fn remove(&mut self, id: BuildingId) -> Option<PlacedBuilding> {
        self.buildings.remove(&id)
    }

    pub fn get(&self, id: BuildingId) -> Option<&PlacedBuilding> {
        self.buildings.get(&id)
    }

    pub fn iter(&self) -> impl Iterator<Item = (BuildingId, &PlacedBuilding)> {
        self.buildings.iter().map(|(id, b)| (*id, b))
    }

    pub fn obstacles(&self) -> impl Iterator<Item = Obstacle> + '_ {
        self.buildings.values().map(PlacedBuilding::obstacle)
    }

    /// Extrémités d'un convoyeur : de la sortie de `from` à l'entrée de `to`.
    pub fn conveyor_ends(&self, from: BuildingId, to: BuildingId) -> Option<(Vec3, Vec3)> {
        Some((
            self.get(from)?.output_world()?,
            self.get(to)?.input_world()?,
        ))
    }

    /// Premier convoyeur touché par un rayon (à moins de 40 cm de son axe).
    pub fn raycast_conveyors(
        &self,
        sim: &SimWorld,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(BuildingId, f32)> {
        sim.conveyors()
            .filter_map(|(id, c)| {
                let (a, b) = self.conveyor_ends(c.from, c.to)?;
                let (t, gap) = ray_segment(origin, direction, a, b);
                (gap <= 0.4 && t <= max_distance).then_some((id, t))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }

    /// Premier bâtiment touché par un rayon (`direction` normalisée).
    pub fn raycast(
        &self,
        origin: Vec3,
        direction: Vec3,
        max_distance: f32,
    ) -> Option<(BuildingId, f32)> {
        self.buildings
            .iter()
            .filter_map(|(id, b)| {
                let (min, max) = b.bounds();
                ray_aabb(origin, direction, min, max).map(|t| (*id, t))
            })
            .filter(|&(_, t)| t <= max_distance)
            .min_by(|a, b| a.1.total_cmp(&b.1))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementError {
    NoDeposit,
    OnDeposit,
    Underwater,
    TooSteep,
    DepositOccupied,
    BlockedByRock,
    BlockedByBuilding,
    BlockedByPlayer,
}

impl PlacementError {
    pub fn message(self) -> &'static str {
        match self {
            PlacementError::NoDeposit => "un extracteur doit être posé sur un gisement",
            PlacementError::OnDeposit => "laisse les gisements libres pour les extracteurs",
            PlacementError::Underwater => "impossible de construire dans l'eau",
            PlacementError::TooSteep => "terrain trop pentu (fondation de plus de 2,5 m)",
            PlacementError::DepositOccupied => "ce gisement a déjà un extracteur",
            PlacementError::BlockedByRock => "un rocher gêne : détruis-le d'abord",
            PlacementError::BlockedByBuilding => "un autre bâtiment occupe la place",
            PlacementError::BlockedByPlayer => "tu es sur l'emplacement : recule",
        }
    }
}

/// Où et comment le bâtiment serait posé. Toujours calculé, même si la pose est refusée,
/// pour afficher le fantôme au bon endroit.
#[derive(Debug, Clone, PartialEq)]
pub struct PlacementPreview {
    pub building: PlacedBuilding,
    /// Gisement visé, si le point visé est dans sa zone.
    pub deposit: Option<DepositId>,
    /// Nombre de bâtiments auxquels il serait relié électriquement.
    pub connections: usize,
    pub result: Result<(), PlacementError>,
}

/// Distance le long du rayon et écart minimal entre un rayon et un segment [a, b].
fn ray_segment(origin: Vec3, direction: Vec3, a: Vec3, b: Vec3) -> (f32, f32) {
    let segment = b - a;
    let w = origin - a;
    let (aa, ab, bb) = (
        direction.dot(direction),
        direction.dot(segment),
        segment.dot(segment),
    );
    let (aw, bw) = (direction.dot(w), segment.dot(w));
    let denominator = aa * bb - ab * ab;
    // Paramètre sur le segment (0..1) du point le plus proche, puis sur le rayon (≥ 0).
    let s = if denominator.abs() < 1e-6 {
        0.0
    } else {
        ((aa * bw - ab * aw) / denominator).clamp(0.0, 1.0)
    };
    let t = ((ab * s - aw) / aa).max(0.0);
    let gap = (origin + direction * t).distance(a + segment * s);
    (t, gap)
}

/// Problème empêchant la pose d'un convoyeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConveyorIssue {
    /// Pas encore de destination visée.
    NoTarget,
    Rule(ConveyorError),
    /// Le tapis droit passerait sous le terrain.
    ThroughTerrain,
}

impl ConveyorIssue {
    pub fn message(self) -> &'static str {
        match self {
            ConveyorIssue::NoTarget => "vise l'entrée d'un four ou d'un coffre",
            ConveyorIssue::Rule(error) => error.message(),
            ConveyorIssue::ThroughTerrain => "le tapis traverserait le relief",
        }
    }
}

/// Aperçu d'un convoyeur en cours de pose.
#[derive(Debug, Clone, PartialEq)]
pub struct ConveyorPreview {
    pub from: BuildingId,
    pub to: Option<BuildingId>,
    pub start: Vec3,
    pub end: Vec3,
    pub result: Result<(), ConveyorIssue>,
}

/// Évalue un convoyeur partant de la sortie de `from`, vers le bâtiment visé `to`
/// (ou, à défaut, vers le point visé `aim`, pour l'aperçu).
pub fn preview_conveyor(
    from: BuildingId,
    to: Option<BuildingId>,
    aim: Vec3,
    sim: &SimWorld,
    placed: &PlacedBuildings,
    field: &HeightField,
) -> Option<ConveyorPreview> {
    let start = placed.get(from)?.output_world()?;
    let target_input = to
        .and_then(|id| placed.get(id))
        .and_then(PlacedBuilding::input_world);
    let end = target_input.unwrap_or(aim);
    let result = match to {
        None => Err(ConveyorIssue::NoTarget),
        Some(to) => {
            let length = start.distance(end);
            sim.check_conveyor(from, to, length)
                .map_err(ConveyorIssue::Rule)
                .and_then(|()| {
                    if passes_above_terrain(start, end, field) {
                        Ok(())
                    } else {
                        Err(ConveyorIssue::ThroughTerrain)
                    }
                })
        }
    };
    Some(ConveyorPreview {
        from,
        to,
        start,
        end,
        result,
    })
}

/// Le segment reste-t-il au-dessus du sol (à 5 cm près), échantillonné tous les 50 cm ?
fn passes_above_terrain(start: Vec3, end: Vec3, field: &HeightField) -> bool {
    let steps = (start.distance(end) / 0.5).ceil().max(1.0) as usize;
    (0..=steps).all(|k| {
        let p = start.lerp(end, k as f32 / steps as f32);
        p.y >= field.height_at(p.x, p.z) - 0.05
    })
}

/// Plateau et bas de la fondation pour une emprise carrée centrée en (x, z).
/// Retourne aussi le dénivelé de l'emprise.
pub fn foundation(field: &HeightField, x: f32, z: f32, half: f32) -> (f32, f32, f32) {
    let samples = [-half, 0.0, half].into_iter().flat_map(|ox| {
        [-half, 0.0, half]
            .into_iter()
            .map(move |oz| field.height_at(x + ox, z + oz))
    });
    let (lo, hi) = samples.fold((f32::MAX, f32::MIN), |(lo, hi), h| (lo.min(h), hi.max(h)));
    (hi + 0.05, lo - 0.3, hi - lo)
}

/// Tout ce dont la validation a besoin pour juger d'une pose.
pub struct PlacementContext<'a> {
    pub field: &'a HeightField,
    pub deposits: &'a DepositField,
    pub rocks: &'a RockField,
    pub sim: &'a SimWorld,
    pub placed: &'a PlacedBuildings,
    /// Position et rayon du joueur.
    pub player: (Vec3, f32),
}

/// Évalue la pose d'un bâtiment de type `kind` visé au point `aim` du terrain.
pub fn preview(
    kind: BuildingKind,
    aim: Vec3,
    rotation: u8,
    ctx: &PlacementContext,
) -> PlacementPreview {
    let deposit = ctx.deposits.find_at(aim.x, aim.z);
    let (x, z) = match (kind, deposit) {
        (BuildingKind::Extractor, Some(d)) => (d.x, d.z),
        _ => (
            (aim.x / GRID_M).round() * GRID_M,
            (aim.z / GRID_M).round() * GRID_M,
        ),
    };
    let half = dims(kind).half;
    let (base_y, foundation_bottom, drop) = foundation(ctx.field, x, z, half);
    let building = PlacedBuilding {
        kind,
        x,
        z,
        base_y,
        foundation_bottom,
        rotation: rotation % 4,
    };

    let footprint = building.obstacle();
    let (player_pos, player_radius) = ctx.player;
    let touches_deposit = ctx
        .deposits
        .all()
        .iter()
        .any(|d| footprint.overlaps_circle(d.x, d.z, d.radius));
    let result = if ctx.field.height_at(x, z) < 0.0 {
        Err(PlacementError::Underwater)
    } else if kind == BuildingKind::Extractor && deposit.is_none() {
        Err(PlacementError::NoDeposit)
    } else if kind != BuildingKind::Extractor && touches_deposit {
        Err(PlacementError::OnDeposit)
    } else if drop > MAX_FOUNDATION_M {
        Err(PlacementError::TooSteep)
    } else if deposit
        .is_some_and(|d| kind == BuildingKind::Extractor && ctx.sim.extractor_on(d.id).is_some())
    {
        Err(PlacementError::DepositOccupied)
    } else if ctx.placed.iter().any(|(_, other)| building.overlaps(other)) {
        Err(PlacementError::BlockedByBuilding)
    } else if ctx
        .rocks
        .live()
        .any(|r| footprint.overlaps_circle(r.x, r.z, r.radius))
    {
        Err(PlacementError::BlockedByRock)
    } else if footprint.overlaps_circle(player_pos.x, player_pos.z, player_radius) {
        Err(PlacementError::BlockedByPlayer)
    } else {
        Ok(())
    };

    PlacementPreview {
        connections: connections(kind, x, z, ctx.sim),
        deposit: deposit.map(|d| d.id),
        building,
        result,
    }
}

/// Nombre de bâtiments existants qui seraient reliés à un nouveau bâtiment posé en (x, z) :
/// tout bâtiment à portée pour un poteau, les poteaux à portée pour une machine.
pub fn connections(kind: BuildingKind, x: f32, z: f32, sim: &SimWorld) -> usize {
    let range_sq = POLE_RANGE_M * POLE_RANGE_M;
    if !kind.is_power_node() {
        return 0;
    }
    sim.buildings()
        .filter(|(_, b)| b.machine.kind().is_power_node())
        .filter(|(_, b)| kind == BuildingKind::Pole || b.machine.kind() == BuildingKind::Pole)
        .filter(|(_, b)| {
            let (dx, dz) = (b.x - x, b.z - z);
            dx * dx + dz * dz <= range_sq
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::deposits::DepositConfig;
    use crate::world::generation::{WorldGenConfig, WorldSeed};
    use crate::world::rocks::RockConfig;

    struct World {
        field: HeightField,
        rocks: RockField,
        deposits: DepositField,
        sim: SimWorld,
        placed: PlacedBuildings,
    }

    fn world() -> World {
        let seed = WorldSeed(42);
        let field = HeightField::generate(seed, &WorldGenConfig::default());
        let rocks = RockField::generate(seed, &field, &RockConfig::default());
        let deposits = DepositField::generate(seed, &field, &rocks, &DepositConfig::default());
        World {
            field,
            rocks,
            deposits,
            sim: SimWorld::default(),
            placed: PlacedBuildings::default(),
        }
    }

    const FAR: (Vec3, f32) = (Vec3::new(10_000.0, 0.0, 10_000.0), 0.3);

    fn try_place(
        w: &World,
        kind: BuildingKind,
        aim: Vec3,
        player: (Vec3, f32),
    ) -> PlacementPreview {
        let ctx = PlacementContext {
            field: &w.field,
            deposits: &w.deposits,
            rocks: &w.rocks,
            sim: &w.sim,
            placed: &w.placed,
            player,
        };
        preview(kind, aim, 0, &ctx)
    }

    fn starter_aim(w: &World) -> Vec3 {
        let d = w.deposits.get(w.deposits.starter().unwrap()).unwrap();
        // Visé un peu à côté du centre : l'extracteur doit quand même s'y aligner.
        Vec3::new(d.x + 1.2, w.field.height_at(d.x + 1.2, d.z), d.z)
    }

    fn spawn_aim(w: &World) -> Vec3 {
        let (x, z) = w.field.find_spawn().unwrap();
        Vec3::new(x + 0.4, w.field.height_at(x, z), z - 0.3)
    }

    #[test]
    fn extractor_snaps_to_the_deposit_center() {
        let w = world();
        let d = w.deposits.get(w.deposits.starter().unwrap()).unwrap();
        let p = try_place(&w, BuildingKind::Extractor, starter_aim(&w), FAR);
        assert_eq!(p.result, Ok(()));
        assert_eq!((p.building.x, p.building.z), (d.x, d.z));
        assert_eq!(p.deposit, Some(d.id));
    }

    #[test]
    fn foundation_reaches_from_below_the_lowest_to_above_the_highest_point() {
        let w = world();
        let p = try_place(&w, BuildingKind::Extractor, starter_aim(&w), FAR);
        let b = &p.building;
        for (ox, oz) in [(-1.5, -1.5), (1.5, 1.5), (0.0, 0.0), (-1.5, 1.5)] {
            let h = w.field.height_at(b.x + ox, b.z + oz);
            assert!(b.foundation_bottom < h && h < b.base_y);
        }
    }

    #[test]
    fn extractor_is_refused_off_deposit_and_others_on_deposits() {
        let w = world();
        let off = try_place(&w, BuildingKind::Extractor, spawn_aim(&w), FAR);
        assert_eq!(off.result, Err(PlacementError::NoDeposit));
        assert_eq!(off.building.x, off.building.x.round(), "calé sur la grille");

        let generator = try_place(&w, BuildingKind::Generator, spawn_aim(&w), FAR);
        assert_eq!(generator.result, Ok(()));
        let on_deposit = try_place(&w, BuildingKind::Furnace, starter_aim(&w), FAR);
        assert_eq!(on_deposit.result, Err(PlacementError::OnDeposit));
    }

    #[test]
    fn refused_on_an_occupied_deposit_on_the_player_or_on_a_building() {
        let mut w = world();
        let aim = starter_aim(&w);
        let d = w
            .deposits
            .get(w.deposits.starter().unwrap())
            .unwrap()
            .clone();
        let on_player = try_place(
            &w,
            BuildingKind::Extractor,
            aim,
            (Vec3::new(d.x, 0.0, d.z), 0.3),
        );
        assert_eq!(on_player.result, Err(PlacementError::BlockedByPlayer));

        w.sim.add_extractor(&d);
        assert_eq!(
            try_place(&w, BuildingKind::Extractor, aim, FAR).result,
            Err(PlacementError::DepositOccupied)
        );

        let generator = try_place(&w, BuildingKind::Generator, spawn_aim(&w), FAR);
        w.placed.insert(BuildingId(99), generator.building.clone());
        let overlap = try_place(&w, BuildingKind::Pole, spawn_aim(&w) + Vec3::X, FAR);
        assert_eq!(overlap.result, Err(PlacementError::BlockedByBuilding));
    }

    #[test]
    fn poles_report_their_future_connections() {
        let mut w = world();
        let aim = spawn_aim(&w);
        w.sim.add_building(BuildingKind::Generator, aim.x, aim.z);
        w.sim
            .add_building(BuildingKind::Generator, aim.x + 50.0, aim.z);
        // Un poteau à 6 m du premier générateur s'y relie ; l'autre est trop loin.
        let pole = try_place(&w, BuildingKind::Pole, aim + Vec3::new(6.0, 0.0, 0.0), FAR);
        assert_eq!(pole.connections, 1);
        // Une machine ne se relie qu'aux poteaux : aucun pour l'instant.
        assert_eq!(
            connections(BuildingKind::Furnace, aim.x + 6.0, aim.z, &w.sim),
            0
        );
        // Le coffre ne fait pas partie du réseau : il n'est pas compté.
        w.sim
            .add_building(BuildingKind::Storage, aim.x + 6.0, aim.z + 3.0);
        let pole = try_place(&w, BuildingKind::Pole, aim + Vec3::new(6.0, 0.0, 0.0), FAR);
        assert_eq!(pole.connections, 1);
    }

    #[test]
    fn ports_turn_with_the_building() {
        let mut furnace = PlacedBuilding {
            kind: BuildingKind::Furnace,
            x: 10.0,
            z: 0.0,
            base_y: 2.0,
            foundation_bottom: 1.0,
            rotation: 0,
        };
        assert_eq!(furnace.output_world(), Some(Vec3::new(11.5, 2.7, 0.0)));
        furnace.rotation = 2; // demi-tour : la sortie passe à l'ouest
        let out = furnace.output_world().unwrap();
        assert!(out.abs_diff_eq(Vec3::new(8.5, 2.7, 0.0), 1e-5), "{out}");
        assert_eq!(
            BuildingKind::Generator.has_output(),
            output_port(BuildingKind::Generator).is_some()
        );
    }

    #[test]
    fn ray_segment_distance() {
        let (t, gap) = ray_segment(
            Vec3::new(0.0, 1.0, -5.0),
            Vec3::Z,
            Vec3::new(-3.0, 0.8, 0.0),
            Vec3::new(3.0, 0.8, 0.0),
        );
        assert!(
            (t - 5.0).abs() < 1e-4 && (gap - 0.2).abs() < 1e-4,
            "{t} {gap}"
        );
    }

    #[test]
    fn placed_buildings_block_and_can_be_aimed_at() {
        let w = world();
        let p = try_place(&w, BuildingKind::Extractor, starter_aim(&w), FAR);
        let mut placed = PlacedBuildings::default();
        placed.insert(BuildingId(7), p.building.clone());

        let b = &p.building;
        let origin = Vec3::new(b.x - 6.0, b.base_y + 1.0, b.z);
        let (id, distance) = placed.raycast(origin, Vec3::X, 10.0).expect("visé");
        assert_eq!(id, BuildingId(7));
        assert!((distance - 4.5).abs() < 1e-3, "{distance}");
        assert_eq!(placed.obstacles().count(), 1);

        assert!(placed.remove(BuildingId(7)).is_some());
        assert_eq!(placed.raycast(origin, Vec3::X, 10.0), None);
    }
}
