//! Arbre technologique primitif (V0.1) et coûts de construction, en Rust pur.
//!
//! Les **paliers** se débloquent en livrant des objets aux hubs. Les quantités sont
//! **cumulées** depuis le début de la partie (les livraisons du hub, déjà sauvegardées) :
//! la progression n'a donc pas d'état propre à sauvegarder. Un palier exige aussi que le
//! précédent soit atteint.
//!
//! En mode normal, chaque bâtiment, convoyeur ou module se **paie en plaques** prises dans
//! l'inventaire du joueur, et le démontage les **rembourse** intégralement.
//! En mode créatif, tout est débloqué et gratuit.

use super::hub::Deliveries;
use super::modules::ModuleKind;
use super::sim_world::BuildingKind;
use crate::inventory::{Inventory, ItemKind};

/// Ce qu'un palier rend constructible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unlock {
    Building(BuildingKind),
    Conveyor,
    Module(ModuleKind),
}

impl Unlock {
    pub fn name(self) -> &'static str {
        match self {
            Unlock::Building(kind) => kind.name(),
            Unlock::Conveyor => "Convoyeur",
            Unlock::Module(kind) => kind.name(),
        }
    }
}

/// Un palier : objets à livrer (cumulés) et ce qu'il débloque.
#[derive(Debug, Clone, Copy)]
pub struct Tier {
    pub name: &'static str,
    pub requires: &'static [(ItemKind, u64)],
    pub unlocks: &'static [Unlock],
}

/// Paliers, dans l'ordre. Le palier 0 est disponible dès le départ.
pub const TIERS: [Tier; 5] = [
    Tier {
        name: "Départ",
        requires: &[],
        unlocks: &[
            Unlock::Building(BuildingKind::Extractor),
            Unlock::Building(BuildingKind::Generator),
            Unlock::Building(BuildingKind::Pole),
            Unlock::Building(BuildingKind::Furnace),
            Unlock::Building(BuildingKind::Storage),
            Unlock::Building(BuildingKind::Hub),
        ],
    },
    Tier {
        name: "Logistique",
        requires: &[(ItemKind::IronPlate, 50)],
        unlocks: &[Unlock::Conveyor],
    },
    Tier {
        name: "Accélération",
        requires: &[(ItemKind::CopperPlate, 50)],
        unlocks: &[Unlock::Module(ModuleKind::Speed)],
    },
    Tier {
        name: "Rendement",
        requires: &[(ItemKind::IronPlate, 300), (ItemKind::CopperPlate, 200)],
        unlocks: &[Unlock::Module(ModuleKind::Efficiency)],
    },
    Tier {
        name: "Industrie",
        requires: &[(ItemKind::IronPlate, 1000)],
        unlocks: &[],
    },
];

/// Palier le plus élevé atteint (0 = départ).
pub fn tier_reached(deliveries: &Deliveries) -> usize {
    TIERS
        .iter()
        .skip(1)
        .take_while(|tier| {
            tier.requires
                .iter()
                .all(|&(kind, amount)| deliveries.total(kind) >= amount)
        })
        .count()
}

/// Palier qui débloque `unlock`.
pub fn tier_of(unlock: Unlock) -> usize {
    TIERS
        .iter()
        .position(|tier| tier.unlocks.contains(&unlock))
        .expect("tout ce qui se construit appartient à un palier")
}

/// `unlock` est-il disponible ?
pub fn is_unlocked(unlock: Unlock, deliveries: &Deliveries, creative: bool) -> bool {
    creative || tier_of(unlock) <= tier_reached(deliveries)
}

/// Prix d'une construction : objets et quantités.
pub type Cost = Vec<(ItemKind, u32)>;

pub fn building_cost(kind: BuildingKind) -> Cost {
    let iron = match kind {
        BuildingKind::Extractor => 10,
        BuildingKind::Generator => 8,
        BuildingKind::Furnace => 8,
        BuildingKind::Storage => 4,
        BuildingKind::Pole => 2,
        BuildingKind::Hub => 15,
    };
    vec![(ItemKind::IronPlate, iron)]
}

/// Mètres de convoyeur payés par plaque de fer.
pub const CONVEYOR_METERS_PER_PLATE: f32 = 2.0;

/// Une plaque de fer par tranche de 2 m commencée.
pub fn conveyor_cost(length_m: f32) -> Cost {
    let plates = (length_m / CONVEYOR_METERS_PER_PLATE).ceil().max(1.0) as u32;
    vec![(ItemKind::IronPlate, plates)]
}

pub fn module_cost(kind: ModuleKind) -> Cost {
    match kind {
        ModuleKind::Speed => vec![(ItemKind::IronPlate, 5), (ItemKind::CopperPlate, 10)],
        ModuleKind::Efficiency => vec![(ItemKind::IronPlate, 10), (ItemKind::CopperPlate, 10)],
    }
}

/// Ce qui manque dans `inventory` pour payer `cost` (vide si c'est payable).
pub fn missing(inventory: &Inventory, cost: &[(ItemKind, u32)]) -> Cost {
    cost.iter()
        .filter_map(|&(kind, amount)| {
            let have = inventory.count(kind);
            (have < amount).then(|| (kind, amount - have))
        })
        .collect()
}

/// Paie `cost` en entier, ou rien du tout. `false` s'il manque quelque chose.
pub fn pay(inventory: &mut Inventory, cost: &[(ItemKind, u32)]) -> bool {
    if !missing(inventory, cost).is_empty() {
        return false;
    }
    for &(kind, amount) in cost {
        inventory.remove(kind, amount);
    }
    true
}

/// Rend des objets au joueur. Retourne ce qui n'a pas tenu dans l'inventaire.
pub fn refund(inventory: &mut Inventory, items: &[(ItemKind, u32)]) -> Cost {
    items
        .iter()
        .filter_map(|&(kind, amount)| {
            let lost = amount - inventory.add(kind, amount);
            (lost > 0).then_some((kind, lost))
        })
        .collect()
}

/// « 10 plaques de fer + 5 plaques de cuivre ».
pub fn describe(cost: &[(ItemKind, u32)]) -> String {
    if cost.is_empty() {
        return "gratuit".into();
    }
    cost.iter()
        .map(|&(kind, amount)| kind.quantity(amount))
        .collect::<Vec<_>>()
        .join(" + ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::INVENTORY_SLOTS;
    use crate::save::STARTER_IRON_PLATES;

    fn delivered(items: &[(ItemKind, u64)]) -> Deliveries {
        let mut d = Deliveries::default();
        for &(kind, count) in items {
            d.record(kind, count, 0);
        }
        d
    }

    #[test]
    fn tiers_unlock_in_order_with_cumulative_deliveries() {
        assert_eq!(tier_reached(&Deliveries::default()), 0);
        assert_eq!(tier_reached(&delivered(&[(ItemKind::IronPlate, 49)])), 0);
        assert_eq!(tier_reached(&delivered(&[(ItemKind::IronPlate, 50)])), 1);
        // Le cuivre seul ne saute pas le palier « Logistique ».
        assert_eq!(tier_reached(&delivered(&[(ItemKind::CopperPlate, 500)])), 0);
        let d = delivered(&[(ItemKind::IronPlate, 300), (ItemKind::CopperPlate, 199)]);
        assert_eq!(tier_reached(&d), 2, "il manque 1 plaque de cuivre");
        let d = delivered(&[(ItemKind::IronPlate, 1000), (ItemKind::CopperPlate, 200)]);
        assert_eq!(tier_reached(&d), 4);
    }

    #[test]
    fn conveyors_need_tier_one_unless_creative() {
        let start = Deliveries::default();
        assert!(is_unlocked(
            Unlock::Building(BuildingKind::Hub),
            &start,
            false
        ));
        assert!(!is_unlocked(Unlock::Conveyor, &start, false));
        assert!(is_unlocked(Unlock::Conveyor, &start, true));
        assert!(is_unlocked(
            Unlock::Conveyor,
            &delivered(&[(ItemKind::IronPlate, 50)]),
            false
        ));
    }

    #[test]
    fn every_buildable_thing_belongs_to_a_tier() {
        for kind in BuildingKind::ALL {
            tier_of(Unlock::Building(kind));
        }
        tier_of(Unlock::Conveyor);
        for kind in ModuleKind::ALL {
            tier_of(Unlock::Module(kind));
        }
    }

    #[test]
    fn the_starter_kit_pays_for_a_first_factory() {
        let first_factory = [
            BuildingKind::Extractor,
            BuildingKind::Furnace,
            BuildingKind::Generator,
            BuildingKind::Pole,
            BuildingKind::Hub,
        ];
        let total: u32 = first_factory
            .iter()
            .flat_map(|&k| building_cost(k))
            .map(|(_, n)| n)
            .sum();
        assert!(total <= STARTER_IRON_PLATES, "{total} plaques nécessaires");
    }

    #[test]
    fn paying_is_all_or_nothing_and_refunds_overflow_is_reported() {
        let mut inventory = Inventory::new(INVENTORY_SLOTS);
        inventory.add(ItemKind::IronPlate, 12);
        let cost = module_cost(ModuleKind::Speed);
        assert_eq!(
            missing(&inventory, &cost),
            vec![(ItemKind::CopperPlate, 10)]
        );
        assert!(!pay(&mut inventory, &cost));
        assert_eq!(
            inventory.count(ItemKind::IronPlate),
            12,
            "rien n'est prélevé"
        );
        assert!(pay(&mut inventory, &building_cost(BuildingKind::Extractor)));
        assert_eq!(inventory.count(ItemKind::IronPlate), 2);
        assert!(refund(&mut inventory, &[(ItemKind::IronPlate, 10)]).is_empty());

        let mut tiny = Inventory::new(1);
        let lost = refund(
            &mut tiny,
            &[(ItemKind::IronPlate, 5), (ItemKind::CopperPlate, 3)],
        );
        assert_eq!(lost, vec![(ItemKind::CopperPlate, 3)]);
    }

    #[test]
    fn conveyor_cost_grows_with_length() {
        assert_eq!(conveyor_cost(0.5), vec![(ItemKind::IronPlate, 1)]);
        assert_eq!(conveyor_cost(2.0), vec![(ItemKind::IronPlate, 1)]);
        assert_eq!(conveyor_cost(2.1), vec![(ItemKind::IronPlate, 2)]);
        assert_eq!(conveyor_cost(40.0), vec![(ItemKind::IronPlate, 20)]);
        assert_eq!(
            describe(&module_cost(ModuleKind::Speed)),
            "5 plaques de fer + 10 plaques de cuivre"
        );
    }
}
