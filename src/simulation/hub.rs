//! Hub de livraison et objectifs, en Rust pur.
//!
//! Le hub accepte tout objet qu'on lui apporte (convoyeur ou à la main) et le
//! **consomme** : l'objet quitte le jeu et est compté. Les livraisons, cumulées sur tous
//! les hubs, font avancer une courte série d'**objectifs** qui donne un but aux parties
//! et préfigure l'arbre technologique de la V0.1.

use std::collections::{BTreeMap, VecDeque};

use super::production::TICKS_PER_MINUTE;
use crate::inventory::ItemKind;

/// Un objectif : livrer une quantité d'un objet au total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Goal {
    pub kind: ItemKind,
    pub amount: u64,
}

/// Objectifs, dans l'ordre : chacun se débloque quand le précédent est atteint.
pub const GOALS: [Goal; 5] = [
    Goal {
        kind: ItemKind::IronPlate,
        amount: 50,
    },
    Goal {
        kind: ItemKind::CopperPlate,
        amount: 50,
    },
    Goal {
        kind: ItemKind::IronPlate,
        amount: 300,
    },
    Goal {
        kind: ItemKind::CopperPlate,
        amount: 200,
    },
    Goal {
        kind: ItemKind::IronPlate,
        amount: 1000,
    },
];

/// Livraisons reçues par les hubs.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deliveries {
    totals: BTreeMap<ItemKind, u64>,
    /// Ticks des livraisons de la dernière minute simulée (débit).
    recent: VecDeque<u64>,
}

impl Deliveries {
    /// Enregistre `count` objets livrés au tick `now`.
    pub fn record(&mut self, kind: ItemKind, count: u64, now: u64) {
        *self.totals.entry(kind).or_default() += count;
        for _ in 0..count {
            self.recent.push_back(now);
        }
        self.forget_before(now);
    }

    fn forget_before(&mut self, now: u64) {
        let window_start = now.saturating_sub(u64::from(TICKS_PER_MINUTE));
        while self.recent.front().is_some_and(|&t| t <= window_start) {
            self.recent.pop_front();
        }
    }

    pub fn total(&self, kind: ItemKind) -> u64 {
        self.totals.get(&kind).copied().unwrap_or(0)
    }

    pub fn grand_total(&self) -> u64 {
        self.totals.values().sum()
    }

    /// Objets livrés pendant la dernière minute simulée.
    pub fn per_minute(&self, now: u64) -> usize {
        let window_start = now.saturating_sub(u64::from(TICKS_PER_MINUTE));
        self.recent.iter().filter(|&&t| t > window_start).count()
    }

    /// Nombre d'objectifs atteints (ils se valident dans l'ordre).
    pub fn goals_reached(&self) -> usize {
        GOALS
            .iter()
            .take_while(|g| self.total(g.kind) >= g.amount)
            .count()
    }

    /// Objectif en cours (le premier non atteint) et son rang, s'il en reste un.
    pub fn current_goal(&self) -> Option<(usize, Goal)> {
        let index = self.goals_reached();
        GOALS.get(index).map(|g| (index, *g))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deliveries_count_per_kind_and_per_minute() {
        let mut d = Deliveries::default();
        d.record(ItemKind::IronPlate, 3, 10);
        d.record(ItemKind::CopperOre, 1, 20);
        assert_eq!(d.total(ItemKind::IronPlate), 3);
        assert_eq!(d.grand_total(), 4);
        assert_eq!(d.per_minute(30), 4);
        // Une minute plus tard, le débit retombe ; les totaux restent.
        assert_eq!(d.per_minute(20 + u64::from(TICKS_PER_MINUTE)), 0);
        assert_eq!(d.total(ItemKind::IronPlate), 3);
    }

    #[test]
    fn goals_are_reached_in_order() {
        let mut d = Deliveries::default();
        assert_eq!(d.current_goal(), Some((0, GOALS[0])));
        // Du cuivre livré en avance ne valide pas l'objectif du fer.
        d.record(ItemKind::CopperPlate, 60, 1);
        assert_eq!(d.goals_reached(), 0);
        d.record(ItemKind::IronPlate, 50, 2);
        assert_eq!(d.goals_reached(), 2, "fer 50 puis cuivre 50 (déjà livré)");
        assert_eq!(d.current_goal(), Some((2, GOALS[2])));
        d.record(ItemKind::IronPlate, 2000, 3);
        d.record(ItemKind::CopperPlate, 500, 4);
        assert_eq!(d.current_goal(), None, "tout est fait");
    }
}
