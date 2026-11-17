//! Convoyeurs point à point, en Rust pur.
//!
//! Un convoyeur relie la **sortie** d'un bâtiment à l'**entrée** d'un autre. Les objets y
//! avancent à vitesse fixe, espacés d'au moins `CONVEYOR_SPACING_TICKS` (débit maximal
//! de 60 objets/min). Chaque objet a un « âge » entier en ticks ; il arrive au bout quand
//! son âge atteint la durée du trajet.
//!
//! **Conservation** : un objet quitte la machine source au moment exact où il est posé
//! sur le tapis, et n'en descend qu'au moment exact où la destination l'accepte. Si la
//! destination est pleine, le tapis se remplit puis s'arrête de prendre : rien n'est perdu.

use std::collections::VecDeque;

use super::clock::TICK_RATE_HZ;
use super::sim_world::BuildingId;
use crate::inventory::ItemKind;
use serde::{Deserialize, Serialize};

/// Vitesse de déplacement des objets, en m/s.
pub const CONVEYOR_SPEED_MPS: f32 = 1.5;
/// Écart minimal entre deux objets, en ticks (20 ticks = 1 objet/s = 60/min).
pub const CONVEYOR_SPACING_TICKS: u32 = TICK_RATE_HZ;
/// Longueur maximale d'un convoyeur, en mètres.
pub const CONVEYOR_MAX_LENGTH_M: f32 = 40.0;
/// Longueur minimale (les deux ports ne peuvent pas se toucher).
pub const CONVEYOR_MIN_LENGTH_M: f32 = 0.5;

/// Débit maximal d'un convoyeur, en objets par minute.
pub const fn conveyor_max_rate_per_min() -> u32 {
    60 * TICK_RATE_HZ / CONVEYOR_SPACING_TICKS
}

/// Durée du trajet pour une longueur donnée, en ticks (au moins 1).
pub fn travel_ticks(length_m: f32) -> u32 {
    ((length_m / CONVEYOR_SPEED_MPS * TICK_RATE_HZ as f32).ceil() as u32).max(1)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
struct BeltItem {
    kind: ItemKind,
    age: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conveyor {
    pub from: BuildingId,
    pub to: BuildingId,
    travel_ticks: u32,
    /// De l'objet le plus avancé (devant) au dernier posé (derrière).
    items: VecDeque<BeltItem>,
    delivered_total: u64,
}

impl Conveyor {
    pub fn new(from: BuildingId, to: BuildingId, length_m: f32) -> Self {
        Self {
            from,
            to,
            travel_ticks: travel_ticks(length_m),
            items: VecDeque::new(),
            delivered_total: 0,
        }
    }

    pub fn travel_ticks(&self) -> u32 {
        self.travel_ticks
    }

    /// Nombre maximal d'objets sur le tapis.
    pub fn capacity(&self) -> usize {
        (self.travel_ticks / CONVEYOR_SPACING_TICKS) as usize + 1
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn delivered_total(&self) -> u64 {
        self.delivered_total
    }

    /// Objets transportés et leur position le long du tapis (0 = départ, 1 = arrivée).
    pub fn items(&self) -> impl Iterator<Item = (ItemKind, f32)> + '_ {
        self.items
            .iter()
            .map(|i| (i.kind, i.age as f32 / self.travel_ticks as f32))
    }

    /// Fait avancer les objets d'un tick. Un objet bloqué derrière un autre attend,
    /// en gardant l'écart minimal (le tapis s'accumule).
    pub fn advance(&mut self) {
        for (index, item) in self.items.iter_mut().enumerate() {
            let limit = self
                .travel_ticks
                .saturating_sub(index as u32 * CONVEYOR_SPACING_TICKS);
            item.age = (item.age + 1).min(limit);
        }
    }

    /// Type de l'objet arrivé au bout, prêt à être livré.
    pub fn arrived(&self) -> Option<ItemKind> {
        self.items
            .front()
            .filter(|i| i.age >= self.travel_ticks)
            .map(|i| i.kind)
    }

    /// Retire l'objet arrivé (à appeler seulement si la destination l'a accepté).
    pub fn deliver(&mut self) {
        if self.arrived().is_some() {
            self.items.pop_front();
            self.delivered_total += 1;
        }
    }

    /// Peut-on poser un nouvel objet au départ (place et écart minimal) ?
    pub fn can_load(&self) -> bool {
        self.items.len() < self.capacity()
            && self
                .items
                .back()
                .is_none_or(|last| last.age >= CONVEYOR_SPACING_TICKS)
    }

    pub fn load(&mut self, kind: ItemKind) {
        debug_assert!(self.can_load());
        self.items.push_back(BeltItem { kind, age: 0 });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn belt(length_m: f32) -> Conveyor {
        Conveyor::new(BuildingId(0), BuildingId(1), length_m)
    }

    /// Un tick complet avec une source infinie et une destination qui accepte ou non.
    fn tick(c: &mut Conveyor, dest_accepts: bool) -> bool {
        c.advance();
        let mut delivered = false;
        if c.arrived().is_some() && dest_accepts {
            c.deliver();
            delivered = true;
        }
        if c.can_load() {
            c.load(ItemKind::IronOre);
        }
        delivered
    }

    #[test]
    fn travel_time_follows_length_and_speed() {
        assert_eq!(travel_ticks(1.5), 20); // 1 s
        assert_eq!(travel_ticks(15.0), 200); // 10 s
        assert_eq!(travel_ticks(0.01), 1);
        assert_eq!(conveyor_max_rate_per_min(), 60);
    }

    #[test]
    fn first_item_arrives_after_the_travel_time_then_one_per_second() {
        let mut c = belt(15.0); // 200 ticks
        let deliveries: Vec<u32> = (1..=600).filter(|_| tick(&mut c, true)).collect();
        assert_eq!(
            deliveries.first(),
            Some(&201),
            "posé au tick 1, livré 200 ticks plus tard"
        );
        // Ensuite, un objet toutes les 20 ticks : 60/min.
        assert!(deliveries.windows(2).all(|w| w[1] - w[0] == 20));
        assert_eq!(deliveries.len(), (600 - 201) / 20 + 1);
    }

    #[test]
    fn blocked_destination_fills_the_belt_without_losing_anything() {
        let mut c = belt(6.0); // 80 ticks → 5 places
        for _ in 0..1000 {
            tick(&mut c, false);
        }
        assert_eq!(c.len(), c.capacity());
        assert_eq!(c.capacity(), 5);
        assert!(!c.can_load(), "plein : on ne prend plus rien à la source");
        assert_eq!(c.delivered_total(), 0);

        // La destination se libère : les objets repartent dans l'ordre, sans doublon.
        let delivered = (0..1000).filter(|_| tick(&mut c, true)).count();
        assert!(delivered > 5);
        assert_eq!(u64::try_from(delivered).unwrap(), c.delivered_total());
    }

    #[test]
    fn items_keep_their_spacing_when_queued() {
        let mut c = belt(6.0);
        for _ in 0..1000 {
            tick(&mut c, false);
        }
        let positions: Vec<f32> = c.items().map(|(_, p)| p).collect();
        assert_eq!(positions[0], 1.0, "le premier attend au bout");
        assert!(positions.windows(2).all(|w| w[0] > w[1]));
    }
}
