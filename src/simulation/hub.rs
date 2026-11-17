//! Hub de livraison, en Rust pur.
//!
//! Le hub accepte tout objet qu'on lui apporte (convoyeur ou à la main) et le
//! **consomme** : l'objet quitte le jeu et est compté. Les livraisons, cumulées sur tous
//! les hubs, débloquent les paliers de l'arbre technologique (`tech.rs`).

use std::collections::{BTreeMap, VecDeque};

use super::production::TICKS_PER_MINUTE;
use crate::inventory::ItemKind;
use serde::{Deserialize, Serialize};

/// Livraisons reçues par les hubs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
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
}
