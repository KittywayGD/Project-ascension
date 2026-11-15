//! Modules de machine, en Rust pur.
//!
//! Un **module** est un objet du monde fixé sur un **emplacement** (socket) d'une machine
//! de production (le **noyau** : extracteur ou four). Chaque module modifie les
//! statistiques du noyau ; la règle est générique (pas d'exception par machine) :
//!
//! - les bonus s'**additionnent** : 2 modules de vitesse = +100 % de vitesse ;
//! - la vitesse ne descend jamais sous 100 %, la consommation jamais sous 20 %.
//!
//! Règles de rattachement : un module par emplacement, uniquement sur un noyau
//! compatible, jamais sur un autre module (aucun cycle possible).

use super::sim_world::{BuildingId, BuildingKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ModuleKind {
    Speed,
    Efficiency,
}

impl ModuleKind {
    #[cfg(test)]
    pub const ALL: [ModuleKind; 2] = [ModuleKind::Speed, ModuleKind::Efficiency];

    pub fn name(self) -> &'static str {
        match self {
            ModuleKind::Speed => "Module de vitesse",
            ModuleKind::Efficiency => "Module d'efficacité",
        }
    }

    /// Nom court, pour les listes.
    pub fn short_name(self) -> &'static str {
        match self {
            ModuleKind::Speed => "vitesse",
            ModuleKind::Efficiency => "efficacité",
        }
    }

    /// Bonus de vitesse, en points de pourcentage.
    pub fn speed_bonus(self) -> i32 {
        match self {
            ModuleKind::Speed => 50,
            ModuleKind::Efficiency => 0,
        }
    }

    /// Bonus (ou malus) de consommation, en points de pourcentage.
    pub fn power_bonus(self) -> i32 {
        match self {
            ModuleKind::Speed => 60,
            ModuleKind::Efficiency => -30,
        }
    }

    /// Sur quels noyaux ce module peut-il se fixer ?
    pub fn fits(self, host: BuildingKind) -> bool {
        host.module_slots() > 0
    }
}

/// Un module posé : son type, sa machine hôte et son emplacement sur celle-ci.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Module {
    pub kind: ModuleKind,
    pub host: BuildingId,
    pub socket: u8,
}

/// Statistiques d'un noyau après application de ses modules, en pourcentages entiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineStats {
    /// Vitesse de travail (100 = nominale).
    pub speed_percent: u32,
    /// Puissance demandée (100 = nominale).
    pub power_percent: u32,
}

impl Default for MachineStats {
    fn default() -> Self {
        Self::BASE
    }
}

impl MachineStats {
    pub const BASE: Self = Self {
        speed_percent: 100,
        power_percent: 100,
    };
    /// Consommation minimale, quels que soient les modules d'efficacité.
    pub const MIN_POWER_PERCENT: u32 = 20;

    pub fn from_modules(modules: impl IntoIterator<Item = ModuleKind>) -> Self {
        let (speed, power) = modules.into_iter().fold((100, 100), |(s, p), m| {
            (s + m.speed_bonus(), p + m.power_bonus())
        });
        Self {
            speed_percent: speed.max(100) as u32,
            power_percent: power.max(Self::MIN_POWER_PERCENT as i32) as u32,
        }
    }

    /// Applique le pourcentage de consommation à une puissance nominale.
    pub fn scale_power(&self, base_w: u64) -> u64 {
        base_w * u64::from(self.power_percent) / 100
    }
}

/// Raison du refus d'un module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleError {
    Missing,
    Incompatible,
    NoSuchSocket,
    SocketTaken,
}

impl ModuleError {
    pub fn message(self) -> &'static str {
        match self {
            ModuleError::Missing => "vise une machine",
            ModuleError::Incompatible => "se fixe uniquement sur un extracteur ou un four",
            ModuleError::NoSuchSocket => "plus d'emplacement libre (2 au maximum)",
            ModuleError::SocketTaken => "cet emplacement est déjà occupé",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_module_means_base_stats() {
        assert_eq!(MachineStats::from_modules([]), MachineStats::BASE);
    }

    #[test]
    fn bonuses_add_up() {
        let stats = MachineStats::from_modules([ModuleKind::Speed, ModuleKind::Speed]);
        assert_eq!((stats.speed_percent, stats.power_percent), (200, 220));
        let mixed = MachineStats::from_modules([ModuleKind::Speed, ModuleKind::Efficiency]);
        assert_eq!((mixed.speed_percent, mixed.power_percent), (150, 130));
    }

    #[test]
    fn power_never_drops_below_the_minimum() {
        let stats = MachineStats::from_modules([ModuleKind::Efficiency; 5]);
        assert_eq!(stats.power_percent, MachineStats::MIN_POWER_PERCENT);
        assert_eq!(stats.speed_percent, 100);
        assert_eq!(stats.scale_power(30_000), 6_000);
    }

    #[test]
    fn modules_fit_production_machines_only() {
        for kind in ModuleKind::ALL {
            assert!(kind.fits(BuildingKind::Extractor));
            assert!(kind.fits(BuildingKind::Furnace));
            assert!(!kind.fits(BuildingKind::Generator));
            assert!(!kind.fits(BuildingKind::Pole));
            assert!(!kind.fits(BuildingKind::Storage));
        }
    }
}
