//! État complet de la simulation, en Rust pur : horloge, bâtiments, convoyeurs et
//! réseau électrique.
//!
//! Indépendant du rendu : une machine continue de produire que le joueur la regarde
//! ou non, et les entités Bevy ne font qu'afficher cet état.

use std::collections::BTreeMap;

use super::clock::SimClock;
use super::energy::{GENERATOR_POWER_W, NetworkStats, PowerGrid, PowerNode};
use super::hub::Deliveries;
use super::logistics::{CONVEYOR_MAX_LENGTH_M, CONVEYOR_MIN_LENGTH_M, Conveyor};
use super::modules::{MachineStats, Module, ModuleError, ModuleKind};
use super::production::{Extractor, Furnace};
use crate::inventory::{Inventory, ItemKind};
use crate::world::deposits::{Deposit, DepositId};

/// Identifiant stable d'un bâtiment (jamais réutilisé, à sauvegarder en V0.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BuildingId(pub u64);

/// Types de bâtiments constructibles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BuildingKind {
    Extractor,
    Generator,
    Furnace,
    Storage,
    Pole,
    Hub,
}

impl BuildingKind {
    /// Ordre d'affichage dans le menu de construction.
    pub const ALL: [BuildingKind; 6] = [
        BuildingKind::Extractor,
        BuildingKind::Generator,
        BuildingKind::Furnace,
        BuildingKind::Storage,
        BuildingKind::Pole,
        BuildingKind::Hub,
    ];

    pub fn name(self) -> &'static str {
        match self {
            BuildingKind::Extractor => "Extracteur",
            BuildingKind::Generator => "Générateur",
            BuildingKind::Furnace => "Four",
            BuildingKind::Storage => "Coffre",
            BuildingKind::Pole => "Poteau électrique",
            BuildingKind::Hub => "Hub de livraison",
        }
    }

    /// A-t-il une sortie à laquelle brancher un convoyeur ?
    pub fn has_output(self) -> bool {
        matches!(
            self,
            BuildingKind::Extractor | BuildingKind::Furnace | BuildingKind::Storage
        )
    }

    /// A-t-il une entrée qui accepte des objets d'un convoyeur ?
    pub fn has_input(self) -> bool {
        matches!(
            self,
            BuildingKind::Furnace | BuildingKind::Storage | BuildingKind::Hub
        )
    }

    /// Nombre d'emplacements de module (noyaux de production uniquement).
    pub fn module_slots(self) -> u8 {
        match self {
            BuildingKind::Extractor | BuildingKind::Furnace => 2,
            _ => 0,
        }
    }

    /// Fait-il partie du réseau électrique ? (le coffre n'en a pas besoin)
    pub fn is_power_node(self) -> bool {
        !matches!(self, BuildingKind::Storage | BuildingKind::Hub)
    }
}

/// Nombre de cases d'un coffre.
pub const STORAGE_SLOTS: usize = 18;

/// Générateur : fournit une puissance fixe tant qu'il est allumé (pas de carburant en V0).
#[derive(Debug, Clone)]
pub struct Generator {
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub enum Machine {
    Extractor(Extractor),
    Generator(Generator),
    Furnace(Furnace),
    Storage(Inventory),
    Pole,
    /// Hub de livraison : consomme et compte tout ce qu'on lui apporte.
    Hub,
}

impl Machine {
    pub fn kind(&self) -> BuildingKind {
        match self {
            Machine::Extractor(_) => BuildingKind::Extractor,
            Machine::Generator(_) => BuildingKind::Generator,
            Machine::Furnace(_) => BuildingKind::Furnace,
            Machine::Storage(_) => BuildingKind::Storage,
            Machine::Pole => BuildingKind::Pole,
            Machine::Hub => BuildingKind::Hub,
        }
    }

    /// Retire un objet de la sortie (pour un convoyeur).
    fn take_output(&mut self) -> Option<ItemKind> {
        match self {
            Machine::Extractor(e) => {
                let kind = e.output().kind();
                (e.output_mut().take(1) == 1).then_some(kind)
            }
            Machine::Furnace(f) => {
                let kind = f.output().kind()?;
                (f.output_mut().take(1) == 1).then_some(kind)
            }
            Machine::Storage(inventory) => {
                // Dernière case non vide : le coffre se vide « par la fin ».
                let index = (0..inventory.slots().len())
                    .rev()
                    .find(|&i| inventory.slot(i).is_some())?;
                let kind = inventory.slot(index)?.kind;
                (inventory.take_at(index, 1) == 1).then_some(kind)
            }
            _ => None,
        }
    }

    /// Fait entrer un objet (venu d'un convoyeur) ; `false` s'il est refusé.
    fn accept_input(&mut self, kind: ItemKind) -> bool {
        match self {
            Machine::Furnace(f) => f.insert(kind, 1) == 1,
            Machine::Storage(inventory) => inventory.add(kind, 1) == 1,
            Machine::Hub => true,
            _ => false,
        }
    }

    /// (puissance produite, puissance demandée) ce tick, en watts.
    fn power_balance(&self) -> (u64, u64) {
        match self {
            Machine::Generator(g) if g.enabled => (GENERATOR_POWER_W, 0),
            Machine::Extractor(e) if e.wants_power() => (0, e.power_demand_w()),
            Machine::Furnace(f) if f.wants_power() => (0, f.power_demand_w()),
            _ => (0, 0),
        }
    }

    /// Allumé / éteint (les poteaux sont toujours actifs).
    pub fn enabled(&self) -> bool {
        match self {
            Machine::Extractor(e) => e.enabled,
            Machine::Generator(g) => g.enabled,
            Machine::Furnace(f) => f.enabled,
            Machine::Storage(_) | Machine::Pole | Machine::Hub => true,
        }
    }

    pub fn set_enabled(&mut self, on: bool) {
        match self {
            Machine::Extractor(e) => e.enabled = on,
            Machine::Generator(g) => g.enabled = on,
            Machine::Furnace(f) => f.enabled = on,
            Machine::Storage(_) | Machine::Pole | Machine::Hub => {}
        }
    }
}

#[derive(Debug, Clone)]
pub struct Building {
    /// Centre horizontal (x, z) : sert aux connexions électriques.
    pub x: f32,
    pub z: f32,
    pub machine: Machine,
}

/// Raison du refus d'un convoyeur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConveyorError {
    Missing,
    SameBuilding,
    NoOutput,
    NoInput,
    OutputTaken,
    InputTaken,
    TooShort,
    TooLong,
}

impl ConveyorError {
    pub fn message(self) -> &'static str {
        match self {
            ConveyorError::Missing => "bâtiment introuvable",
            ConveyorError::SameBuilding => "choisis un autre bâtiment comme destination",
            ConveyorError::NoOutput => "ce bâtiment n'a pas de sortie",
            ConveyorError::NoInput => "ce bâtiment n'a pas d'entrée",
            ConveyorError::OutputTaken => "cette sortie a déjà un convoyeur",
            ConveyorError::InputTaken => "cette entrée a déjà un convoyeur",
            ConveyorError::TooShort => "les deux bâtiments sont trop proches",
            ConveyorError::TooLong => "trop long (40 m au maximum)",
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct SimWorld {
    clock: SimClock,
    next_building: u64,
    /// `BTreeMap` : ordre d'itération déterministe (par identifiant).
    buildings: BTreeMap<BuildingId, Building>,
    /// Convoyeurs, dans le même espace d'identifiants que les bâtiments.
    conveyors: BTreeMap<BuildingId, Conveyor>,
    /// Modules fixés sur les machines, même espace d'identifiants.
    modules: BTreeMap<BuildingId, Module>,
    /// Livraisons cumulées de tous les hubs (objectifs).
    deliveries: Deliveries,
    grid: PowerGrid,
    /// Incrémenté à chaque recalcul des réseaux (pour redessiner les câbles).
    grid_version: u64,
}

impl SimWorld {
    pub fn clock(&self) -> &SimClock {
        &self.clock
    }

    /// Avance toute la simulation d'un tick : bilan électrique, machines, puis convoyeurs.
    pub fn step(&mut self) {
        self.clock.advance();
        let now = self.clock.tick();
        self.grid
            .update_balance(self.buildings.iter().map(|(id, b)| {
                let (produced, demanded) = b.machine.power_balance();
                (*id, produced, demanded)
            }));
        for (id, building) in &mut self.buildings {
            let power = self.grid.satisfaction_of(*id);
            match &mut building.machine {
                Machine::Extractor(e) => e.step(now, power),
                Machine::Furnace(f) => f.step(power),
                Machine::Generator(_) | Machine::Storage(_) | Machine::Pole | Machine::Hub => {}
            }
        }
        for conveyor in self.conveyors.values_mut() {
            conveyor.advance();
            if let Some(kind) = conveyor.arrived()
                && let Some(dest) = self.buildings.get_mut(&conveyor.to)
                && dest.machine.accept_input(kind)
            {
                conveyor.deliver();
                if dest.machine.kind() == BuildingKind::Hub {
                    self.deliveries.record(kind, 1, now);
                }
            }
            if conveyor.can_load()
                && let Some(source) = self.buildings.get_mut(&conveyor.from)
                && let Some(kind) = source.machine.take_output()
            {
                conveyor.load(kind);
            }
        }
    }

    fn add(&mut self, x: f32, z: f32, machine: Machine) -> BuildingId {
        let id = BuildingId(self.next_building);
        self.next_building += 1;
        self.buildings.insert(id, Building { x, z, machine });
        self.rebuild_grid();
        id
    }

    /// Pose un extracteur au centre du gisement.
    pub fn add_extractor(&mut self, deposit: &Deposit) -> BuildingId {
        let machine = Machine::Extractor(Extractor::new(
            deposit.id,
            deposit.kind,
            deposit.purity,
            self.clock.tick(),
        ));
        self.add(deposit.x, deposit.z, machine)
    }

    pub fn add_building(&mut self, kind: BuildingKind, x: f32, z: f32) -> Option<BuildingId> {
        let machine = match kind {
            BuildingKind::Extractor => return None, // il faut un gisement
            BuildingKind::Generator => Machine::Generator(Generator { enabled: true }),
            BuildingKind::Furnace => Machine::Furnace(Furnace::default()),
            BuildingKind::Storage => Machine::Storage(Inventory::new(STORAGE_SLOTS)),
            BuildingKind::Pole => Machine::Pole,
            BuildingKind::Hub => Machine::Hub,
        };
        Some(self.add(x, z, machine))
    }

    /// Retire un bâtiment, avec les convoyeurs qui y étaient branchés et ses modules.
    pub fn remove(&mut self, id: BuildingId) -> Option<Building> {
        let removed = self.buildings.remove(&id);
        if removed.is_some() {
            self.conveyors.retain(|_, c| c.from != id && c.to != id);
            self.modules.retain(|_, m| m.host != id);
            self.rebuild_grid();
        }
        removed
    }

    /// Vérifie qu'un module de ce type peut se fixer sur l'emplacement `socket` de `host`.
    pub fn check_module(
        &self,
        host: BuildingId,
        socket: u8,
        kind: ModuleKind,
    ) -> Result<(), ModuleError> {
        let building = self.buildings.get(&host).ok_or(ModuleError::Missing)?;
        let host_kind = building.machine.kind();
        if !kind.fits(host_kind) {
            Err(ModuleError::Incompatible)
        } else if socket >= host_kind.module_slots() {
            Err(ModuleError::NoSuchSocket)
        } else if self.module_at(host, socket).is_some() {
            Err(ModuleError::SocketTaken)
        } else {
            Ok(())
        }
    }

    /// Fixe un module et recalcule aussitôt les statistiques de la machine.
    pub fn attach_module(
        &mut self,
        host: BuildingId,
        socket: u8,
        kind: ModuleKind,
    ) -> Result<BuildingId, ModuleError> {
        self.check_module(host, socket, kind)?;
        let id = BuildingId(self.next_building);
        self.next_building += 1;
        self.modules.insert(id, Module { kind, host, socket });
        self.refresh_stats(host);
        Ok(id)
    }

    /// Retire un module ; le contenu de la machine n'est pas touché.
    pub fn detach_module(&mut self, id: BuildingId) -> Option<Module> {
        let module = self.modules.remove(&id)?;
        self.refresh_stats(module.host);
        Some(module)
    }

    pub fn module(&self, id: BuildingId) -> Option<&Module> {
        self.modules.get(&id)
    }

    pub fn modules(&self) -> impl Iterator<Item = (BuildingId, &Module)> {
        self.modules.iter().map(|(id, m)| (*id, m))
    }

    /// Modules d'une machine, par emplacement croissant.
    pub fn modules_of(&self, host: BuildingId) -> Vec<(BuildingId, Module)> {
        let mut list: Vec<_> = self
            .modules
            .iter()
            .filter(|(_, m)| m.host == host)
            .map(|(id, m)| (*id, *m))
            .collect();
        list.sort_by_key(|(_, m)| m.socket);
        list
    }

    fn module_at(&self, host: BuildingId, socket: u8) -> Option<BuildingId> {
        self.modules
            .iter()
            .find(|(_, m)| m.host == host && m.socket == socket)
            .map(|(id, _)| *id)
    }

    /// Premier emplacement libre de la machine, s'il en reste.
    #[cfg(test)]
    pub fn free_socket(&self, host: BuildingId) -> Option<u8> {
        let slots = self.buildings.get(&host)?.machine.kind().module_slots();
        (0..slots).find(|&s| self.module_at(host, s).is_none())
    }

    /// Recalcule les statistiques d'une machine d'après ses modules.
    fn refresh_stats(&mut self, host: BuildingId) {
        let stats =
            MachineStats::from_modules(self.modules_of(host).into_iter().map(|(_, m)| m.kind));
        match self.buildings.get_mut(&host).map(|b| &mut b.machine) {
            Some(Machine::Extractor(e)) => e.stats = stats,
            Some(Machine::Furnace(f)) => f.stats = stats,
            _ => {}
        }
    }

    /// Statistiques actuelles d'une machine (de base si elle n'en a pas).
    pub fn stats_of(&self, host: BuildingId) -> MachineStats {
        match self.buildings.get(&host).map(|b| &b.machine) {
            Some(Machine::Extractor(e)) => e.stats,
            Some(Machine::Furnace(f)) => f.stats,
            _ => MachineStats::BASE,
        }
    }

    /// Vérifie qu'un convoyeur peut relier la sortie de `from` à l'entrée de `to`.
    pub fn check_conveyor(
        &self,
        from: BuildingId,
        to: BuildingId,
        length_m: f32,
    ) -> Result<(), ConveyorError> {
        let (Some(source), Some(dest)) = (self.buildings.get(&from), self.buildings.get(&to))
        else {
            return Err(ConveyorError::Missing);
        };
        if from == to {
            Err(ConveyorError::SameBuilding)
        } else if !source.machine.kind().has_output() {
            Err(ConveyorError::NoOutput)
        } else if !dest.machine.kind().has_input() {
            Err(ConveyorError::NoInput)
        } else if self.conveyors.values().any(|c| c.from == from) {
            Err(ConveyorError::OutputTaken)
        } else if self.conveyors.values().any(|c| c.to == to) {
            Err(ConveyorError::InputTaken)
        } else if length_m < CONVEYOR_MIN_LENGTH_M {
            Err(ConveyorError::TooShort)
        } else if length_m > CONVEYOR_MAX_LENGTH_M {
            Err(ConveyorError::TooLong)
        } else {
            Ok(())
        }
    }

    /// La sortie de ce bâtiment est-elle libre pour un nouveau convoyeur ?
    pub fn output_free(&self, id: BuildingId) -> Result<(), ConveyorError> {
        let building = self.buildings.get(&id).ok_or(ConveyorError::Missing)?;
        if !building.machine.kind().has_output() {
            Err(ConveyorError::NoOutput)
        } else if self.conveyors.values().any(|c| c.from == id) {
            Err(ConveyorError::OutputTaken)
        } else {
            Ok(())
        }
    }

    pub fn add_conveyor(
        &mut self,
        from: BuildingId,
        to: BuildingId,
        length_m: f32,
    ) -> Result<BuildingId, ConveyorError> {
        self.check_conveyor(from, to, length_m)?;
        let id = BuildingId(self.next_building);
        self.next_building += 1;
        self.conveyors.insert(id, Conveyor::new(from, to, length_m));
        Ok(id)
    }

    /// Retire un convoyeur ; les objets qu'il transportait sont perdus (mode créatif).
    pub fn remove_conveyor(&mut self, id: BuildingId) -> Option<Conveyor> {
        self.conveyors.remove(&id)
    }

    pub fn conveyor(&self, id: BuildingId) -> Option<&Conveyor> {
        self.conveyors.get(&id)
    }

    pub fn conveyors(&self) -> impl Iterator<Item = (BuildingId, &Conveyor)> {
        self.conveyors.iter().map(|(id, c)| (*id, c))
    }

    /// Recalcule connexions et réseaux : seulement à la pose ou au retrait d'un bâtiment.
    fn rebuild_grid(&mut self) {
        let nodes: Vec<PowerNode> = self
            .buildings
            .iter()
            .filter(|(_, b)| b.machine.kind().is_power_node())
            .map(|(id, b)| PowerNode {
                id: *id,
                x: b.x,
                z: b.z,
                is_pole: b.machine.kind() == BuildingKind::Pole,
            })
            .collect();
        self.grid = PowerGrid::build(&nodes);
        self.grid_version += 1;
    }

    pub fn building(&self, id: BuildingId) -> Option<&Building> {
        self.buildings.get(&id)
    }

    pub fn building_mut(&mut self, id: BuildingId) -> Option<&mut Building> {
        self.buildings.get_mut(&id)
    }

    pub fn buildings(&self) -> impl Iterator<Item = (BuildingId, &Building)> {
        self.buildings.iter().map(|(id, b)| (*id, b))
    }

    #[cfg(test)]
    pub fn extractor(&self, id: BuildingId) -> Option<&Extractor> {
        match &self.buildings.get(&id)?.machine {
            Machine::Extractor(e) => Some(e),
            _ => None,
        }
    }

    pub fn furnace(&self, id: BuildingId) -> Option<&Furnace> {
        match &self.buildings.get(&id)?.machine {
            Machine::Furnace(f) => Some(f),
            _ => None,
        }
    }

    #[cfg(test)]
    pub fn furnace_mut(&mut self, id: BuildingId) -> Option<&mut Furnace> {
        match &mut self.buildings.get_mut(&id)?.machine {
            Machine::Furnace(f) => Some(f),
            _ => None,
        }
    }

    pub fn extractors(&self) -> impl Iterator<Item = (BuildingId, &Extractor)> {
        self.buildings
            .iter()
            .filter_map(|(id, b)| match &b.machine {
                Machine::Extractor(e) => Some((*id, e)),
                _ => None,
            })
    }

    /// Extracteur déjà posé sur ce gisement, s'il y en a un.
    pub fn extractor_on(&self, deposit: DepositId) -> Option<BuildingId> {
        self.extractors()
            .find(|(_, e)| e.deposit == deposit)
            .map(|(id, _)| id)
    }

    /// Livraisons cumulées de tous les hubs.
    pub fn deliveries(&self) -> &Deliveries {
        &self.deliveries
    }

    /// Livre à la main `count` objets à un hub ; retourne `false` si ce n'est pas un hub.
    /// L'appelant retire d'abord les objets de l'inventaire du joueur (conservation).
    pub fn deliver_by_hand(&mut self, hub: BuildingId, kind: ItemKind, count: u64) -> bool {
        let is_hub = self
            .buildings
            .get(&hub)
            .is_some_and(|b| b.machine.kind() == BuildingKind::Hub);
        if is_hub {
            self.deliveries.record(kind, count, self.clock.tick());
        }
        is_hub
    }

    pub fn grid(&self) -> &PowerGrid {
        &self.grid
    }

    pub fn grid_version(&self) -> u64 {
        self.grid_version
    }

    /// Bilan du réseau auquel appartient ce bâtiment.
    pub fn network_stats(&self, id: BuildingId) -> Option<&NetworkStats> {
        self.grid.network_of(id).and_then(|n| self.grid.network(n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::ItemKind;
    use crate::simulation::production::{MachineState, TICKS_PER_MINUTE};
    use crate::world::deposits::{Purity, ResourceKind};

    fn deposit(id: u32, x: f32, purity: Purity) -> Deposit {
        Deposit {
            id: DepositId(id),
            kind: ResourceKind::Iron,
            purity,
            x,
            z: 0.0,
            radius: 3.0,
        }
    }

    fn run(sim: &mut SimWorld, ticks: u32) {
        for _ in 0..ticks {
            sim.step();
        }
    }

    /// Générateur en 0, poteau en 5, extracteur en 10 : tout est relié.
    fn powered_extractor() -> (SimWorld, BuildingId, BuildingId, BuildingId) {
        let mut sim = SimWorld::default();
        let generator = sim.add_building(BuildingKind::Generator, 0.0, 0.0).unwrap();
        let pole = sim.add_building(BuildingKind::Pole, 5.0, 0.0).unwrap();
        let extractor = sim.add_extractor(&deposit(0, 10.0, Purity::Normal));
        (sim, generator, pole, extractor)
    }

    #[test]
    fn ids_are_never_reused() {
        let mut sim = SimWorld::default();
        let a = sim.add_extractor(&deposit(0, 0.0, Purity::Normal));
        sim.remove(a);
        let b = sim.add_extractor(&deposit(0, 0.0, Purity::Normal));
        assert_ne!(a, b);
        assert_eq!(sim.extractor_on(DepositId(0)), Some(b));
        assert_eq!(sim.extractor_on(DepositId(1)), None);
    }

    #[test]
    fn isolated_machine_has_no_power() {
        let mut sim = SimWorld::default();
        let e = sim.add_extractor(&deposit(0, 0.0, Purity::Normal));
        run(&mut sim, TICKS_PER_MINUTE);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::NoPower);
        assert_eq!(sim.extractor(e).unwrap().produced_total(), 0);
    }

    #[test]
    fn generator_pole_machine_produces_at_full_rate() {
        let (mut sim, _, _, e) = powered_extractor();
        run(&mut sim, TICKS_PER_MINUTE);
        let stats = sim.network_stats(e).unwrap();
        assert_eq!(
            (stats.produced_w, stats.demanded_w, stats.satisfaction),
            (100_000, 30_000, 1000)
        );
        assert_eq!(sim.extractor(e).unwrap().produced_total(), 30);
    }

    #[test]
    fn cutting_the_cable_stops_the_machine_and_reconnecting_resumes_it() {
        let (mut sim, _, pole, e) = powered_extractor();
        run(&mut sim, 600); // 15 unités
        sim.remove(pole);
        run(&mut sim, 600);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::NoPower);
        assert_eq!(sim.extractor(e).unwrap().produced_total(), 15);

        sim.add_building(BuildingKind::Pole, 5.0, 0.0);
        run(&mut sim, 600);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::Active);
        assert_eq!(sim.extractor(e).unwrap().produced_total(), 30);
    }

    #[test]
    fn switching_the_generator_off_stops_the_network() {
        let (mut sim, generator, _, e) = powered_extractor();
        sim.building_mut(generator)
            .unwrap()
            .machine
            .set_enabled(false);
        run(&mut sim, 100);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::NoPower);
        sim.building_mut(generator)
            .unwrap()
            .machine
            .set_enabled(true);
        run(&mut sim, 100);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::Active);
    }

    #[test]
    fn overload_slows_every_machine_proportionally() {
        // 1 générateur (100 kW) pour 2 extracteurs (60 kW) + 1 four (50 kW) = 110 kW.
        let mut sim = SimWorld::default();
        sim.add_building(BuildingKind::Generator, 0.0, 0.0);
        sim.add_building(BuildingKind::Pole, 4.0, 0.0);
        let a = sim.add_extractor(&deposit(0, 8.0, Purity::Normal));
        let b = sim.add_extractor(&deposit(1, 4.0, Purity::Normal));
        let f = sim.add_building(BuildingKind::Furnace, 0.0, 5.0).unwrap();
        sim.furnace_mut(f).unwrap().insert(ItemKind::IronOre, 50);
        run(&mut sim, TICKS_PER_MINUTE);

        let stats = sim.network_stats(a).unwrap();
        assert_eq!((stats.demanded_w, stats.satisfaction), (110_000, 909));
        // 30/min × 0,909 ≈ 27 unités chacun.
        for id in [a, b] {
            assert_eq!(sim.extractor(id).unwrap().produced_total(), 27);
        }
    }

    #[test]
    fn furnace_smelts_when_powered() {
        let mut sim = SimWorld::default();
        sim.add_building(BuildingKind::Generator, 0.0, 0.0);
        sim.add_building(BuildingKind::Pole, 5.0, 0.0);
        let f = sim.add_building(BuildingKind::Furnace, 10.0, 0.0).unwrap();
        sim.furnace_mut(f).unwrap().insert(ItemKind::IronOre, 10);
        run(&mut sim, 20 * 20); // 20 s → 10 plaques
        let furnace = sim.furnace(f).unwrap();
        assert_eq!(furnace.output().count(), 10);
        assert_eq!(furnace.output().kind(), Some(ItemKind::IronPlate));
    }

    fn storage_items(sim: &SimWorld, id: BuildingId, kind: ItemKind) -> u32 {
        match &sim.building(id).unwrap().machine {
            Machine::Storage(inventory) => inventory.count(kind),
            _ => panic!("pas un coffre"),
        }
    }

    /// Chaîne complète, sans fenêtre : gisement → extracteur → four → coffre.
    #[test]
    fn extractor_to_furnace_to_storage_chain_runs_unattended() {
        let (mut sim, _, _, extractor) = powered_extractor();
        // Le four (au sud du poteau) est relié au réseau ; le coffre n'en a pas besoin.
        let furnace = sim.add_building(BuildingKind::Furnace, 5.0, 6.0).unwrap();
        let storage = sim.add_building(BuildingKind::Storage, 5.0, 14.0).unwrap();
        sim.add_conveyor(extractor, furnace, 8.0).unwrap();
        sim.add_conveyor(furnace, storage, 8.0).unwrap();

        run(&mut sim, 5 * TICKS_PER_MINUTE);
        // 30 minerais/min pendant 5 min, moins ce qui est encore en route.
        let plates = storage_items(&sim, storage, ItemKind::IronPlate);
        assert!((140..=150).contains(&plates), "{plates} plaques");

        // Conservation : tout minerai extrait est quelque part (four, tapis, coffre…).
        let produced = sim.extractor(extractor).unwrap().produced_total();
        let in_extractor = u64::from(sim.extractor(extractor).unwrap().output().count());
        let f = sim.furnace(furnace).unwrap();
        let on_belts: u64 = sim.conveyors().map(|(_, c)| c.len() as u64).sum();
        let total = in_extractor
            + on_belts
            + u64::from(f.input().count())
            + u64::from(f.output().count())
            + u64::from(plates);
        assert_eq!(total, produced);
    }

    #[test]
    fn conveyors_need_free_ports_and_a_reasonable_length() {
        let (mut sim, generator, pole, extractor) = powered_extractor();
        let furnace = sim.add_building(BuildingKind::Furnace, 5.0, 6.0).unwrap();
        let storage = sim.add_building(BuildingKind::Storage, 5.0, 14.0).unwrap();
        assert_eq!(
            sim.check_conveyor(generator, furnace, 5.0),
            Err(ConveyorError::NoOutput)
        );
        assert_eq!(
            sim.check_conveyor(extractor, pole, 5.0),
            Err(ConveyorError::NoInput)
        );
        assert_eq!(
            sim.check_conveyor(furnace, furnace, 5.0),
            Err(ConveyorError::SameBuilding)
        );
        assert_eq!(
            sim.check_conveyor(extractor, furnace, 60.0),
            Err(ConveyorError::TooLong)
        );
        sim.add_conveyor(extractor, furnace, 8.0).unwrap();
        assert_eq!(
            sim.check_conveyor(extractor, storage, 8.0),
            Err(ConveyorError::OutputTaken)
        );
        assert_eq!(
            sim.check_conveyor(storage, furnace, 8.0),
            Err(ConveyorError::InputTaken)
        );
    }

    #[test]
    fn removing_a_building_removes_its_conveyors() {
        let (mut sim, _, _, extractor) = powered_extractor();
        let storage = sim.add_building(BuildingKind::Storage, 5.0, 14.0).unwrap();
        let belt = sim.add_conveyor(extractor, storage, 8.0).unwrap();
        sim.remove(storage);
        assert!(sim.conveyor(belt).is_none());
        assert!(sim.output_free(extractor).is_ok());
    }

    #[test]
    fn storage_is_not_part_of_the_power_grid() {
        let (mut sim, ..) = powered_extractor();
        let storage = sim.add_building(BuildingKind::Storage, 5.0, 2.0).unwrap();
        assert!(sim.network_stats(storage).is_none());
    }

    #[test]
    fn modules_change_rate_and_demand_immediately() {
        let (mut sim, _, _, e) = powered_extractor();
        let speed = sim.attach_module(e, 0, ModuleKind::Speed).unwrap();
        run(&mut sim, TICKS_PER_MINUTE);
        assert_eq!(sim.extractor(e).unwrap().produced_total(), 45);
        assert_eq!(sim.network_stats(e).unwrap().demanded_w, 48_000);

        let efficiency = sim.attach_module(e, 1, ModuleKind::Efficiency).unwrap();
        assert_eq!(sim.stats_of(e).power_percent, 130);
        sim.detach_module(speed);
        sim.detach_module(efficiency);
        assert_eq!(sim.stats_of(e), MachineStats::BASE);
    }

    #[test]
    fn module_rules_sockets_and_compatibility() {
        let (mut sim, generator, _, e) = powered_extractor();
        assert_eq!(
            sim.attach_module(generator, 0, ModuleKind::Speed),
            Err(ModuleError::Incompatible)
        );
        sim.attach_module(e, 0, ModuleKind::Speed).unwrap();
        assert_eq!(
            sim.attach_module(e, 0, ModuleKind::Efficiency),
            Err(ModuleError::SocketTaken)
        );
        assert_eq!(sim.free_socket(e), Some(1));
        sim.attach_module(e, 1, ModuleKind::Speed).unwrap();
        assert_eq!(sim.free_socket(e), None);
        assert_eq!(
            sim.attach_module(e, 2, ModuleKind::Speed),
            Err(ModuleError::NoSuchSocket)
        );
    }

    #[test]
    fn detaching_keeps_the_machine_contents() {
        let mut sim = SimWorld::default();
        let f = sim.add_building(BuildingKind::Furnace, 0.0, 0.0).unwrap();
        sim.furnace_mut(f).unwrap().insert(ItemKind::IronOre, 12);
        let m = sim.attach_module(f, 0, ModuleKind::Speed).unwrap();
        sim.detach_module(m);
        assert_eq!(sim.furnace(f).unwrap().input().count(), 12);
        assert!(sim.module(m).is_none());
    }

    #[test]
    fn removing_a_machine_removes_its_modules() {
        let (mut sim, _, _, e) = powered_extractor();
        let m = sim.attach_module(e, 0, ModuleKind::Efficiency).unwrap();
        sim.remove(e);
        assert!(sim.module(m).is_none(), "pas de module fantôme");
        assert_eq!(sim.modules().count(), 0);
    }

    #[test]
    fn hub_counts_what_conveyors_bring() {
        let (mut sim, _, _, extractor) = powered_extractor();
        let hub = sim.add_building(BuildingKind::Hub, 5.0, 14.0).unwrap();
        sim.add_conveyor(extractor, hub, 8.0).unwrap();
        assert!(
            sim.network_stats(hub).is_none(),
            "le hub n'a pas besoin d'énergie"
        );
        run(&mut sim, 2 * TICKS_PER_MINUTE);
        let delivered = sim.deliveries().total(ItemKind::IronOre);
        // 60 minerais extraits en 2 min, moins ceux encore sur le tapis (≤ 6).
        assert!((54..=60).contains(&delivered), "{delivered}");
        assert_eq!(sim.deliveries().per_minute(sim.clock().tick()), 30);
    }

    #[test]
    fn hand_delivery_only_works_on_a_hub() {
        let (mut sim, generator, ..) = powered_extractor();
        let hub = sim.add_building(BuildingKind::Hub, 30.0, 0.0).unwrap();
        assert!(!sim.deliver_by_hand(generator, ItemKind::IronPlate, 10));
        assert!(sim.deliver_by_hand(hub, ItemKind::IronPlate, 10));
        assert_eq!(sim.deliveries().total(ItemKind::IronPlate), 10);
    }

    /// Test de charge (M7) : 250 unités complètes = 1 000 bâtiments + 250 convoyeurs.
    /// Lancer avec `cargo test --features dev stress -- --ignored --nocapture`.
    #[test]
    #[ignore = "mesure de performance, à lancer à la main"]
    fn stress_thousand_buildings() {
        use std::time::Instant;
        let mut sim = SimWorld::default();
        let setup = Instant::now();
        for unit in 0..250u32 {
            let (x, z) = ((unit % 16) as f32 * 25.0, (unit / 16) as f32 * 25.0);
            sim.add_building(BuildingKind::Generator, x, z);
            sim.add_building(BuildingKind::Pole, x + 5.0, z);
            let e = sim.add_extractor(&Deposit {
                id: DepositId(unit),
                kind: ResourceKind::Iron,
                purity: Purity::Normal,
                x: x + 10.0,
                z,
                radius: 3.0,
            });
            let f = sim
                .add_building(BuildingKind::Furnace, x + 5.0, z + 6.0)
                .unwrap();
            sim.add_conveyor(e, f, 7.0).unwrap();
        }
        let setup = setup.elapsed();
        let ticks = TICKS_PER_MINUTE;
        let start = Instant::now();
        run(&mut sim, ticks);
        let per_tick = start.elapsed().as_secs_f64() * 1e6 / f64::from(ticks);
        println!(
            "{} bâtiments, {} convoyeurs : pose {:.0} ms, {per_tick:.0} µs par tick ({:.2} % d'un cœur à 20 Hz)",
            sim.buildings().count(),
            sim.conveyors().count(),
            setup.as_secs_f64() * 1000.0,
            per_tick * 20.0 / 1e6 * 100.0
        );
        assert_eq!(sim.buildings().count(), 1000);
        assert!(per_tick < 5_000.0, "trop lent : {per_tick:.0} µs par tick");
    }

    #[test]
    fn switched_off_machines_do_not_consume() {
        let (mut sim, _, _, e) = powered_extractor();
        sim.building_mut(e).unwrap().machine.set_enabled(false);
        run(&mut sim, 10);
        assert_eq!(sim.network_stats(e).unwrap().demanded_w, 0);
        assert_eq!(sim.extractor(e).unwrap().state(), MachineState::Off);
    }

    /// La simulation ne dépend que du nombre de ticks, pas de leur regroupement par frame.
    #[test]
    fn result_depends_only_on_the_number_of_ticks() {
        let (mut slow, _, _, a) = powered_extractor();
        let (mut fast, _, _, b) = powered_extractor();
        run(&mut slow, 600);
        for _frame in 0..6 {
            run(&mut fast, 100);
        }
        assert_eq!(slow.clock(), fast.clock());
        let (ea, eb) = (slow.extractor(a).unwrap(), fast.extractor(b).unwrap());
        assert_eq!(ea.produced_total(), eb.produced_total());
        assert_eq!(ea.output(), eb.output());
    }
}
