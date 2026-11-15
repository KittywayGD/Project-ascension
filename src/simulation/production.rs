//! Machines de production (extracteur, four), en Rust pur et en entiers.
//!
//! Règle (voir `docs/simulation.md`) : chaque tick, une machine reçoit une **alimentation**
//! `p` en millièmes (1000 = pleine puissance, voir `energy.rs`) et avance de `p × v`, où
//! `v` est sa vitesse en pourcentage (100 sans module, voir `modules.rs`). Les
//! accumulateurs sont entiers : aucune perte, aucune dérive.
//!
//! - Extracteur au débit `r` unités/minute : l'accumulateur reçoit `r × p × v` par tick et
//!   une unité sort à chaque `TICKS_PER_MINUTE × 1000 × 100`. À pleine puissance et sans
//!   module, après `n` ticks : exactement `⌊n × r / 1200⌋` unités.
//! - Four : une recette de `d` ticks reçoit `p × v` par tick et se termine à `d × 1000 × 100`.

use std::collections::VecDeque;

use super::clock::TICK_RATE_HZ;
use super::modules::MachineStats;
use crate::inventory::{Inventory, ItemBuffer, ItemKind, MachineSlot};
use crate::world::deposits::{DepositId, Purity, ResourceKind};

pub const TICKS_PER_MINUTE: u32 = TICK_RATE_HZ * 60;

/// Alimentation complète, en millièmes.
pub const FULL_POWER: u32 = 1000;

/// Débit d'un extracteur sur un gisement de pureté normale, en unités/minute.
pub const EXTRACTOR_BASE_RATE_PER_MIN: u32 = 30;
/// Capacité de l'inventaire de sortie d'un extracteur.
pub const EXTRACTOR_OUTPUT_CAPACITY: u32 = 50;
/// Puissance demandée par un extracteur en marche, en watts.
pub const EXTRACTOR_POWER_W: u64 = 30_000;

/// Capacité de l'entrée et de la sortie d'un four.
pub const FURNACE_SLOT_CAPACITY: u32 = 50;
/// Puissance demandée par un four en marche, en watts.
pub const FURNACE_POWER_W: u64 = 50_000;

/// Débit théorique d'un extracteur selon la pureté du gisement.
pub fn extractor_rate_per_min(purity: Purity) -> u32 {
    EXTRACTOR_BASE_RATE_PER_MIN * purity.rate_percent() / 100
}

/// Une recette : 1 objet d'entrée → 1 objet de sortie, en `duration_ticks` à pleine puissance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Recipe {
    pub input: ItemKind,
    pub output: ItemKind,
    pub duration_ticks: u32,
}

impl Recipe {
    /// Énergie consommée par fabrication à pleine puissance, en joules.
    pub fn energy_j(&self, power_w: u64) -> u64 {
        power_w * u64::from(self.duration_ticks) / u64::from(TICK_RATE_HZ)
    }
}

/// Recettes du four : 2 s par plaque à pleine puissance (100 kJ à 50 kW).
pub const FURNACE_RECIPES: [Recipe; 2] = [
    Recipe {
        input: ItemKind::IronOre,
        output: ItemKind::IronPlate,
        duration_ticks: 2 * TICK_RATE_HZ,
    },
    Recipe {
        input: ItemKind::CopperOre,
        output: ItemKind::CopperPlate,
        duration_ticks: 2 * TICK_RATE_HZ,
    },
];

pub fn furnace_recipe(input: ItemKind) -> Option<&'static Recipe> {
    FURNACE_RECIPES.iter().find(|r| r.input == input)
}

/// État d'une machine, commun à l'extracteur et au four.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MachineState {
    Active,
    /// Rien à transformer (four sans minerai).
    WaitingInput,
    /// Sortie pleine : la production est suspendue, la progression conservée.
    OutputFull,
    /// Prête à travailler mais sans courant (non reliée, ou réseau sans production).
    NoPower,
    /// Éteinte par le joueur : ne consomme rien.
    Off,
}

impl MachineState {
    pub fn name(self) -> &'static str {
        match self {
            MachineState::Active => "actif",
            MachineState::WaitingInput => "en attente de minerai",
            MachineState::OutputFull => "bloqué (sortie pleine)",
            MachineState::NoPower => "sans énergie",
            MachineState::Off => "éteint",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Extractor {
    pub deposit: DepositId,
    pub enabled: bool,
    /// Vitesse et consommation, recalculées à chaque ajout ou retrait de module.
    pub stats: MachineStats,
    rate_per_min: u32,
    /// Accumulateur en « unités × ticks / minute × millièmes ».
    progress: u64,
    output: ItemBuffer,
    state: MachineState,
    produced_total: u64,
    placed_at_tick: u64,
    /// Ticks des productions de la dernière minute simulée (débit mesuré).
    recent: VecDeque<u64>,
}

impl Extractor {
    pub fn new(deposit: DepositId, kind: ResourceKind, purity: Purity, now: u64) -> Self {
        Self {
            deposit,
            enabled: true,
            stats: MachineStats::BASE,
            rate_per_min: extractor_rate_per_min(purity),
            progress: 0,
            output: ItemBuffer::new(ItemKind::ore_of(kind), EXTRACTOR_OUTPUT_CAPACITY),
            state: MachineState::NoPower,
            produced_total: 0,
            placed_at_tick: now,
            recent: VecDeque::new(),
        }
    }

    /// Débit nominal (pureté seule, sans module).
    pub fn rate_per_min(&self) -> u32 {
        self.rate_per_min
    }

    /// Débit théorique avec les modules, à pleine puissance.
    pub fn effective_rate_per_min(&self) -> f32 {
        self.rate_per_min as f32 * self.stats.speed_percent as f32 / 100.0
    }

    /// Puissance demandée quand il travaille, modules compris.
    pub fn power_demand_w(&self) -> u64 {
        self.stats.scale_power(EXTRACTOR_POWER_W)
    }

    pub fn output(&self) -> &ItemBuffer {
        &self.output
    }

    pub fn output_mut(&mut self) -> &mut ItemBuffer {
        &mut self.output
    }

    pub fn state(&self) -> MachineState {
        self.state
    }

    pub fn produced_total(&self) -> u64 {
        self.produced_total
    }

    fn threshold() -> u64 {
        u64::from(TICKS_PER_MINUTE) * u64::from(FULL_POWER) * 100
    }

    /// Avancement vers la prochaine unité, entre 0 et 1.
    pub fn progress_fraction(&self) -> f32 {
        self.progress as f32 / Self::threshold() as f32
    }

    /// L'extracteur demande du courant s'il est allumé et a de la place en sortie.
    pub fn wants_power(&self) -> bool {
        self.enabled && !self.output.is_full()
    }

    /// Avance d'un tick ; `now` est le numéro du tick, `power` l'alimentation en millièmes.
    pub fn step(&mut self, now: u64, power: u32) {
        let window_start = now.saturating_sub(u64::from(TICKS_PER_MINUTE));
        while self.recent.front().is_some_and(|&t| t <= window_start) {
            self.recent.pop_front();
        }

        self.state = if !self.enabled {
            MachineState::Off
        } else if self.output.is_full() {
            MachineState::OutputFull
        } else if power == 0 {
            MachineState::NoPower
        } else {
            MachineState::Active
        };
        if self.state != MachineState::Active {
            return;
        }

        self.progress += u64::from(self.rate_per_min)
            * u64::from(power.min(FULL_POWER))
            * u64::from(self.stats.speed_percent);
        while self.progress >= Self::threshold() {
            if self.output.add(1) == 0 {
                // Plein en cours de tick : on garde la progression pour plus tard.
                self.state = MachineState::OutputFull;
                break;
            }
            self.progress -= Self::threshold();
            self.produced_total += 1;
            self.recent.push_back(now);
        }
    }

    /// Débit mesuré sur la dernière minute simulée (extrapolé si l'extracteur est plus récent).
    pub fn measured_rate_per_min(&self, now: u64) -> f32 {
        let age = now.saturating_sub(self.placed_at_tick);
        if age == 0 {
            return 0.0;
        }
        let window = age.min(u64::from(TICKS_PER_MINUTE));
        self.recent.len() as f32 * TICKS_PER_MINUTE as f32 / window as f32
    }
}

/// Four : fond un minerai en plaque. Le type de minerai déposé choisit la recette.
#[derive(Debug, Clone)]
pub struct Furnace {
    pub enabled: bool,
    /// Vitesse et consommation, recalculées à chaque ajout ou retrait de module.
    pub stats: MachineStats,
    input: MachineSlot,
    output: MachineSlot,
    /// Avancement de la fabrication en cours, en « ticks × millièmes × pourcents ».
    progress: u64,
    state: MachineState,
    crafted_total: u64,
}

impl Default for Furnace {
    fn default() -> Self {
        Self {
            enabled: true,
            stats: MachineStats::BASE,
            input: MachineSlot::new(FURNACE_SLOT_CAPACITY),
            output: MachineSlot::new(FURNACE_SLOT_CAPACITY),
            progress: 0,
            state: MachineState::WaitingInput,
            crafted_total: 0,
        }
    }
}

impl Furnace {
    pub fn input(&self) -> &MachineSlot {
        &self.input
    }

    /// Accès direct à l'entrée (reprendre du minerai à la main).
    pub fn input_mut(&mut self) -> &mut MachineSlot {
        &mut self.input
    }

    pub fn output(&self) -> &MachineSlot {
        &self.output
    }

    pub fn output_mut(&mut self) -> &mut MachineSlot {
        &mut self.output
    }

    pub fn state(&self) -> MachineState {
        self.state
    }

    pub fn crafted_total(&self) -> u64 {
        self.crafted_total
    }

    /// Recette en cours, déterminée par le minerai présent en entrée.
    pub fn recipe(&self) -> Option<&'static Recipe> {
        self.input.kind().and_then(furnace_recipe)
    }

    /// Seuil de fin d'une recette pour l'accumulateur.
    fn threshold(recipe: &Recipe) -> u64 {
        u64::from(recipe.duration_ticks) * u64::from(FULL_POWER) * 100
    }

    /// Avancement de la fabrication en cours, entre 0 et 1.
    pub fn progress_fraction(&self) -> f32 {
        self.recipe()
            .map_or(0.0, |r| self.progress as f32 / Self::threshold(r) as f32)
    }

    /// Puissance demandée quand il travaille, modules compris.
    pub fn power_demand_w(&self) -> u64 {
        self.stats.scale_power(FURNACE_POWER_W)
    }

    /// Durée d'une fabrication à pleine puissance, modules compris, en secondes.
    pub fn craft_seconds(&self, recipe: &Recipe) -> f32 {
        recipe.duration_ticks as f32 / TICK_RATE_HZ as f32 * 100.0 / self.stats.speed_percent as f32
    }

    /// Le four accepte ce minerai s'il a une recette, que l'entrée est vide ou du même
    /// type, et que la sortie peut recevoir le produit.
    pub fn accepts(&self, kind: ItemKind) -> bool {
        furnace_recipe(kind).is_some_and(|recipe| {
            self.input.space_for(kind) > 0 && self.output.space_for(recipe.output) > 0
        })
    }

    /// Place à donner au minerai `kind` (0 s'il n'est pas accepté).
    pub fn input_space_for(&self, kind: ItemKind) -> u32 {
        if self.accepts(kind) {
            self.input.space_for(kind)
        } else {
            0
        }
    }

    /// Ajoute du minerai en entrée ; retourne le nombre accepté.
    pub fn insert(&mut self, kind: ItemKind, amount: u32) -> u32 {
        if self.accepts(kind) {
            self.input.insert(kind, amount)
        } else {
            0
        }
    }

    /// Charge le plus de minerai possible depuis l'inventaire : le type déjà présent en
    /// entrée, sinon le premier minerai accepté. Retourne (type, quantité) déplacés.
    pub fn load_from(&mut self, inventory: &mut Inventory) -> Option<(ItemKind, u32)> {
        let kind = self.input.kind().or_else(|| {
            FURNACE_RECIPES
                .iter()
                .map(|r| r.input)
                .find(|&k| inventory.count(k) > 0 && self.accepts(k))
        })?;
        let wanted = inventory.count(kind).min(self.input_space_for(kind));
        let removed = inventory.remove(kind, wanted);
        let added = self.insert(kind, removed);
        debug_assert_eq!(removed, added, "conservation des objets");
        (added > 0).then_some((kind, added))
    }

    /// Le four demande du courant s'il est allumé et peut fabriquer.
    pub fn wants_power(&self) -> bool {
        self.enabled
            && self
                .recipe()
                .is_some_and(|r| self.output.space_for(r.output) > 0)
    }

    /// Avance d'un tick avec l'alimentation `power` (millièmes).
    pub fn step(&mut self, power: u32) {
        let Some(recipe) = self.recipe() else {
            self.progress = 0;
            self.state = if self.enabled {
                MachineState::WaitingInput
            } else {
                MachineState::Off
            };
            return;
        };
        self.state = if !self.enabled {
            MachineState::Off
        } else if self.output.space_for(recipe.output) == 0 {
            MachineState::OutputFull
        } else if power == 0 {
            MachineState::NoPower
        } else {
            MachineState::Active
        };
        if self.state != MachineState::Active {
            return;
        }

        self.progress += u64::from(power.min(FULL_POWER)) * u64::from(self.stats.speed_percent);
        if self.progress >= Self::threshold(recipe) {
            // Une seule consommation par fabrication, au moment où elle se termine.
            let consumed = self.input.take(1);
            let produced = self.output.insert(recipe.output, consumed);
            debug_assert_eq!((consumed, produced), (1, 1), "conservation au four");
            self.progress -= Self::threshold(recipe);
            self.crafted_total += u64::from(produced);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn extractor(purity: Purity) -> Extractor {
        Extractor::new(DepositId(0), ResourceKind::Iron, purity, 0)
    }

    fn run(e: &mut Extractor, from: u64, ticks: u64) {
        run_at(e, from, ticks, FULL_POWER);
    }

    fn run_at(e: &mut Extractor, from: u64, ticks: u64, power: u32) {
        for t in from + 1..=from + ticks {
            e.step(t, power);
        }
    }

    #[test]
    fn rates_follow_purity() {
        assert_eq!(extractor_rate_per_min(Purity::Impure), 15);
        assert_eq!(extractor_rate_per_min(Purity::Normal), 30);
        assert_eq!(extractor_rate_per_min(Purity::Pure), 60);
    }

    #[test]
    fn produces_exactly_the_rate_each_simulated_minute() {
        for purity in [Purity::Impure, Purity::Normal] {
            let mut e = extractor(purity);
            run(&mut e, 0, u64::from(TICKS_PER_MINUTE));
            assert_eq!(
                e.produced_total(),
                u64::from(e.rate_per_min()),
                "{purity:?}"
            );
        }
    }

    #[test]
    fn half_power_means_half_speed() {
        let mut e = extractor(Purity::Normal);
        run_at(&mut e, 0, u64::from(TICKS_PER_MINUTE), 500);
        assert_eq!(e.produced_total(), 15);
        assert_eq!(e.state(), MachineState::Active);
    }

    #[test]
    fn no_power_or_switched_off_produces_nothing_and_keeps_progress() {
        let mut e = extractor(Purity::Normal);
        run(&mut e, 0, 30); // un peu de progression
        let progress = e.progress;
        run_at(&mut e, 30, 600, 0);
        assert_eq!(e.state(), MachineState::NoPower);
        e.enabled = false;
        run(&mut e, 630, 600);
        assert_eq!(e.state(), MachineState::Off);
        assert!(!e.wants_power());
        assert_eq!((e.produced_total(), e.progress), (0, progress));
    }

    #[test]
    fn speed_modules_raise_the_rate_and_the_demand() {
        let mut e = extractor(Purity::Normal);
        e.stats = MachineStats {
            speed_percent: 150,
            power_percent: 160,
        };
        run(&mut e, 0, u64::from(TICKS_PER_MINUTE));
        assert_eq!(e.produced_total(), 45);
        assert_eq!(e.power_demand_w(), 48_000);
        assert_eq!(e.effective_rate_per_min(), 45.0);
    }

    #[test]
    fn furnace_speed_shortens_the_recipe() {
        let mut f = furnace_with(ItemKind::IronOre, 10);
        f.stats.speed_percent = 200; // 1 s par plaque
        run_furnace(&mut f, 20, FULL_POWER);
        assert_eq!(f.output().count(), 1);
        assert_eq!(f.craft_seconds(&FURNACE_RECIPES[0]), 1.0);
    }

    #[test]
    fn no_loss_with_uneven_rates() {
        let mut e = extractor(Purity::Normal);
        e.rate_per_min = 7;
        for minute in 0..10u64 {
            run(
                &mut e,
                minute * u64::from(TICKS_PER_MINUTE),
                u64::from(TICKS_PER_MINUTE),
            );
            e.output_mut().take_all();
        }
        assert_eq!(e.produced_total(), 70);
        // floor(n × r / 1200) à tout instant
        let mut f = extractor(Purity::Normal);
        f.rate_per_min = 7;
        run(&mut f, 0, 1000);
        assert_eq!(f.produced_total(), 1000 * 7 / 1200);
    }

    #[test]
    fn stops_when_full_and_resumes_without_losing_progress() {
        let mut e = extractor(Purity::Pure); // 60/min, sortie de 50
        run(&mut e, 0, u64::from(TICKS_PER_MINUTE));
        assert_eq!(e.output().count(), EXTRACTOR_OUTPUT_CAPACITY);
        assert_eq!(e.state(), MachineState::OutputFull);
        assert!(!e.wants_power(), "plein : ne demande plus de courant");
        let blocked = e.produced_total();

        run(&mut e, 1200, 1200);
        assert_eq!(e.produced_total(), blocked);

        // On vide : la production reprend au même rythme (30 en 30 s à 60/min).
        assert_eq!(e.output_mut().take_all(), 50);
        run(&mut e, 2400, 600);
        assert_eq!(e.state(), MachineState::Active);
        assert_eq!(e.produced_total(), blocked + 30);
    }

    #[test]
    fn deposit_never_runs_out() {
        let mut e = extractor(Purity::Normal);
        let minute = u64::from(TICKS_PER_MINUTE);
        for m in 0..600 {
            run(&mut e, m * minute, minute);
            e.output_mut().take_all();
        }
        assert_eq!(e.produced_total(), 600 * 30);
    }

    #[test]
    fn measured_rate_matches_theory_once_the_window_is_full() {
        let mut e = extractor(Purity::Normal);
        let minute = u64::from(TICKS_PER_MINUTE);
        for m in 0..3 {
            run(&mut e, m * minute, minute);
            e.output_mut().take_all();
        }
        assert!((e.measured_rate_per_min(3 * minute) - 30.0).abs() < 1.01);
    }

    fn furnace_with(kind: ItemKind, count: u32) -> Furnace {
        let mut f = Furnace::default();
        assert_eq!(f.insert(kind, count), count);
        f
    }

    fn run_furnace(f: &mut Furnace, ticks: u32, power: u32) {
        for _ in 0..ticks {
            f.step(power);
        }
    }

    #[test]
    fn furnace_smelts_one_plate_every_two_seconds() {
        let mut f = furnace_with(ItemKind::IronOre, 10);
        run_furnace(&mut f, 39, FULL_POWER);
        assert_eq!(f.output().count(), 0, "pas encore");
        run_furnace(&mut f, 1, FULL_POWER);
        assert_eq!(
            f.output().stack().map(|s| (s.kind, s.count)),
            Some((ItemKind::IronPlate, 1))
        );
        assert_eq!(f.input().count(), 9, "un seul minerai consommé");
        run_furnace(&mut f, 9 * 40 + 100, FULL_POWER);
        assert_eq!((f.input().count(), f.output().count()), (0, 10));
        assert_eq!(f.state(), MachineState::WaitingInput);
        assert!(!f.wants_power());
    }

    #[test]
    fn furnace_recipe_costs_100_kilojoules() {
        assert_eq!(FURNACE_RECIPES[0].energy_j(FURNACE_POWER_W), 100_000);
    }

    #[test]
    fn furnace_slows_down_and_stops_with_power() {
        let mut f = furnace_with(ItemKind::CopperOre, 5);
        run_furnace(&mut f, 80, 500); // demi-puissance : 4 s pour une plaque
        assert_eq!(f.output().count(), 1);
        run_furnace(&mut f, 200, 0);
        assert_eq!(f.state(), MachineState::NoPower);
        assert_eq!(f.output().count(), 1);
    }

    #[test]
    fn furnace_rejects_items_without_recipe_or_incompatible_output() {
        let mut f = Furnace::default();
        assert_eq!(
            f.insert(ItemKind::IronPlate, 5),
            0,
            "pas de recette pour une plaque"
        );
        assert_eq!(f.insert(ItemKind::IronOre, 1), 1);
        run_furnace(&mut f, 40, FULL_POWER);
        // La sortie contient des plaques de fer : le cuivre est refusé tant qu'elle n'est pas vidée.
        assert!(!f.accepts(ItemKind::CopperOre));
        f.output_mut().take(1);
        assert!(f.accepts(ItemKind::CopperOre));
    }

    #[test]
    fn loading_from_the_inventory_conserves_items() {
        let mut inventory = Inventory::new(3);
        inventory.add(ItemKind::CopperOre, 80);
        inventory.add(ItemKind::IronOre, 10);
        let mut f = Furnace::default();
        // Le fer passe en premier (ordre des recettes), dans la limite de la place.
        assert_eq!(f.load_from(&mut inventory), Some((ItemKind::IronOre, 10)));
        assert_eq!(inventory.count(ItemKind::IronOre), 0);
        // L'entrée contient du fer : le cuivre n'est pas mélangé.
        assert_eq!(f.load_from(&mut inventory), None);
        assert_eq!(inventory.count(ItemKind::CopperOre), 80);
        assert_eq!(f.input().count(), 10);
    }

    #[test]
    fn furnace_blocks_when_output_is_full() {
        let mut f = furnace_with(ItemKind::IronOre, 50);
        run_furnace(&mut f, 50 * 40 + 10, FULL_POWER);
        assert_eq!(f.output().count(), 50);
        assert_eq!(f.input().count(), 0);
        // Remplir de nouveau l'entrée : la sortie est pleine, le four se bloque.
        assert_eq!(f.insert(ItemKind::IronOre, 3), 0, "sortie pleine : refusé");
    }
}
