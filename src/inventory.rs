//! Objets, piles et inventaires, en Rust pur.
//!
//! Invariant central : **conservation des objets**. Un transfert retire exactement ce
//! qu'il ajoute, jamais plus que la place disponible ; rien n'est dupliqué ni perdu.

use crate::world::deposits::ResourceKind;

/// Type d'objet transportable : minerais (extraits des gisements) et plaques (fondues au four).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ItemKind {
    IronOre,
    CopperOre,
    IronPlate,
    CopperPlate,
}

impl ItemKind {
    /// Tous les objets, dans un ordre fixe (affichage des totaux).
    pub const ALL: [ItemKind; 4] = [
        ItemKind::IronOre,
        ItemKind::CopperOre,
        ItemKind::IronPlate,
        ItemKind::CopperPlate,
    ];

    /// Minerai extrait d'un gisement de cette ressource.
    pub fn ore_of(resource: ResourceKind) -> Self {
        match resource {
            ResourceKind::Iron => ItemKind::IronOre,
            ResourceKind::Copper => ItemKind::CopperOre,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ItemKind::IronOre => "minerai de fer",
            ItemKind::CopperOre => "minerai de cuivre",
            ItemKind::IronPlate => "plaque de fer",
            ItemKind::CopperPlate => "plaque de cuivre",
        }
    }

    /// Quantité accordée : « 1 minerai de fer », « 3 minerais de fer ».
    pub fn quantity(self, count: u32) -> String {
        let (singular, plural) = match self {
            ItemKind::IronOre => ("minerai de fer", "minerais de fer"),
            ItemKind::CopperOre => ("minerai de cuivre", "minerais de cuivre"),
            ItemKind::IronPlate => ("plaque de fer", "plaques de fer"),
            ItemKind::CopperPlate => ("plaque de cuivre", "plaques de cuivre"),
        };
        format!("{count} {}", if count > 1 { plural } else { singular })
    }

    /// Accorde un participe passé avec ce type d'objet : `agree(2, "récupéré")` donne
    /// « récupérés » pour du minerai et « récupérées » pour des plaques.
    pub fn agree(self, count: u32, participle: &str) -> String {
        let feminine = matches!(self, ItemKind::IronPlate | ItemKind::CopperPlate);
        format!(
            "{participle}{}{}",
            if feminine { "e" } else { "" },
            if count > 1 { "s" } else { "" }
        )
    }

    /// Abréviation affichée dans les cases d'inventaire.
    pub fn symbol(self) -> &'static str {
        match self {
            ItemKind::IronOre => "Fe",
            ItemKind::CopperOre => "Cu",
            ItemKind::IronPlate => "Pl.Fe",
            ItemKind::CopperPlate => "Pl.Cu",
        }
    }

    /// Nombre maximal d'objets par case.
    pub fn stack_size(self) -> u32 {
        100
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemStack {
    pub kind: ItemKind,
    pub count: u32,
}

/// Inventaire à cases (celui du joueur) : chaque case contient au plus une pile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inventory {
    slots: Vec<Option<ItemStack>>,
}

impl Inventory {
    pub fn new(slot_count: usize) -> Self {
        Self {
            slots: vec![None; slot_count],
        }
    }

    pub fn slots(&self) -> &[Option<ItemStack>] {
        &self.slots
    }

    pub fn count(&self, kind: ItemKind) -> u32 {
        self.slots
            .iter()
            .flatten()
            .filter(|s| s.kind == kind)
            .map(|s| s.count)
            .sum()
    }

    /// Nombre d'objets de ce type que l'inventaire peut encore accueillir.
    pub fn space_for(&self, kind: ItemKind) -> u32 {
        self.slots
            .iter()
            .map(|slot| match slot {
                None => kind.stack_size(),
                Some(s) if s.kind == kind => kind.stack_size() - s.count,
                Some(_) => 0,
            })
            .sum()
    }

    /// Ajoute jusqu'à `amount` objets : complète d'abord les piles existantes, puis
    /// remplit les cases vides dans l'ordre. Retourne le nombre réellement ajouté.
    pub fn add(&mut self, kind: ItemKind, amount: u32) -> u32 {
        let mut left = amount;
        for stack in self.slots.iter_mut().flatten() {
            if left == 0 {
                break;
            }
            if stack.kind == kind {
                let moved = left.min(kind.stack_size() - stack.count);
                stack.count += moved;
                left -= moved;
            }
        }
        for slot in self.slots.iter_mut().filter(|s| s.is_none()) {
            if left == 0 {
                break;
            }
            let moved = left.min(kind.stack_size());
            *slot = Some(ItemStack { kind, count: moved });
            left -= moved;
        }
        amount - left
    }

    pub fn slot(&self, index: usize) -> Option<ItemStack> {
        self.slots.get(index).copied().flatten()
    }

    /// Place disponible dans la case `index` pour ce type (0 si elle contient un autre type).
    pub fn space_at(&self, index: usize, kind: ItemKind) -> u32 {
        match self.slots.get(index) {
            Some(None) => kind.stack_size(),
            Some(Some(s)) if s.kind == kind => kind.stack_size() - s.count,
            _ => 0,
        }
    }

    /// Ajoute dans la case `index` uniquement ; retourne le nombre ajouté.
    pub fn add_at(&mut self, index: usize, kind: ItemKind, amount: u32) -> u32 {
        let added = amount.min(self.space_at(index, kind));
        if added > 0 {
            let count = self.slot(index).map_or(0, |s| s.count) + added;
            self.slots[index] = Some(ItemStack { kind, count });
        }
        added
    }

    /// Retire jusqu'à `amount` objets de la case `index` ; retourne le nombre retiré.
    pub fn take_at(&mut self, index: usize, amount: u32) -> u32 {
        let Some(Some(stack)) = self.slots.get_mut(index) else {
            return 0;
        };
        let taken = amount.min(stack.count);
        stack.count -= taken;
        if stack.count == 0 {
            self.slots[index] = None;
        }
        taken
    }

    /// Glisser-déposer d'une case vers une autre du même inventaire :
    /// déplace si la cible est vide, complète si même type, échange sinon.
    pub fn move_within(&mut self, from: usize, to: usize) {
        if from == to || from >= self.slots.len() || to >= self.slots.len() {
            return;
        }
        let (low, high) = (from.min(to), from.max(to));
        let (head, tail) = self.slots.split_at_mut(high);
        let (a, b) = (&mut head[low], &mut tail[0]);
        if from < to {
            merge_or_swap(a, b);
        } else {
            merge_or_swap(b, a);
        }
    }

    /// Retire jusqu'à `amount` objets de ce type (des dernières cases vers les premières).
    /// Retourne le nombre réellement retiré.
    pub fn remove(&mut self, kind: ItemKind, amount: u32) -> u32 {
        let mut left = amount;
        for slot in self.slots.iter_mut().rev() {
            if left == 0 {
                break;
            }
            if let Some(stack) = slot
                && stack.kind == kind
            {
                let moved = left.min(stack.count);
                stack.count -= moved;
                left -= moved;
                if stack.count == 0 {
                    *slot = None;
                }
            }
        }
        amount - left
    }
}

/// Glisser-déposer d'une case d'un inventaire vers une case d'un autre.
pub fn move_between(from: &mut Inventory, from_index: usize, to: &mut Inventory, to_index: usize) {
    if let (Some(a), Some(b)) = (from.slots.get_mut(from_index), to.slots.get_mut(to_index)) {
        merge_or_swap(a, b);
    }
}

/// Règle commune du glisser-déposer entre deux cases :
/// cible vide → déplacement ; même type → on complète la cible ; sinon → échange.
fn merge_or_swap(src: &mut Option<ItemStack>, dst: &mut Option<ItemStack>) {
    match (src.as_mut(), dst.as_mut()) {
        (None, _) => {}
        (Some(_), None) => *dst = src.take(),
        (Some(s), Some(d)) if s.kind == d.kind => {
            let moved = s.count.min(d.kind.stack_size() - d.count);
            d.count += moved;
            s.count -= moved;
            if s.count == 0 {
                *src = None;
            }
        }
        (Some(_), Some(_)) => std::mem::swap(src, dst),
    }
}

/// Emplacement de machine borné, qui contient un seul type d'objet à la fois.
/// Vide, il accepte n'importe quel type ; il oublie son type dès qu'il est vidé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachineSlot {
    stack: Option<ItemStack>,
    capacity: u32,
}

impl MachineSlot {
    pub fn new(capacity: u32) -> Self {
        Self {
            stack: None,
            capacity,
        }
    }

    pub fn stack(&self) -> Option<ItemStack> {
        self.stack
    }

    pub fn kind(&self) -> Option<ItemKind> {
        self.stack.map(|s| s.kind)
    }

    pub fn count(&self) -> u32 {
        self.stack.map_or(0, |s| s.count)
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Place disponible pour ce type (0 si l'emplacement contient un autre type).
    pub fn space_for(&self, kind: ItemKind) -> u32 {
        match self.stack {
            None => self.capacity,
            Some(s) if s.kind == kind => self.capacity - s.count,
            Some(_) => 0,
        }
    }

    /// Ajoute jusqu'à `amount` objets ; retourne le nombre réellement ajouté.
    pub fn insert(&mut self, kind: ItemKind, amount: u32) -> u32 {
        let added = amount.min(self.space_for(kind));
        if added > 0 {
            let count = self.count() + added;
            self.stack = Some(ItemStack { kind, count });
        }
        added
    }

    /// Retire jusqu'à `amount` objets ; retourne le nombre réellement retiré.
    pub fn take(&mut self, amount: u32) -> u32 {
        let Some(stack) = &mut self.stack else {
            return 0;
        };
        let taken = amount.min(stack.count);
        stack.count -= taken;
        if stack.count == 0 {
            self.stack = None;
        }
        taken
    }
}

/// Inventaire borné d'un seul type d'objet (entrée ou sortie de machine).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemBuffer {
    kind: ItemKind,
    count: u32,
    capacity: u32,
}

impl ItemBuffer {
    pub fn new(kind: ItemKind, capacity: u32) -> Self {
        Self {
            kind,
            count: 0,
            capacity,
        }
    }

    pub fn kind(&self) -> ItemKind {
        self.kind
    }

    pub fn count(&self) -> u32 {
        self.count
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    pub fn is_full(&self) -> bool {
        self.count >= self.capacity
    }

    /// Ajoute jusqu'à `amount` objets ; retourne le nombre réellement ajouté.
    pub fn add(&mut self, amount: u32) -> u32 {
        let added = amount.min(self.capacity - self.count);
        self.count += added;
        added
    }

    /// Retire jusqu'à `amount` objets ; retourne le nombre réellement retiré.
    pub fn take(&mut self, amount: u32) -> u32 {
        let taken = amount.min(self.count);
        self.count -= taken;
        taken
    }

    /// Retire tout ; retourne le nombre retiré.
    pub fn take_all(&mut self) -> u32 {
        std::mem::take(&mut self.count)
    }
}

/// Déplace le plus d'objets possible d'un tampon vers un inventaire.
/// Retourne le nombre déplacé ; ce qui ne rentre pas reste dans le tampon.
pub fn transfer(from: &mut ItemBuffer, to: &mut Inventory) -> u32 {
    let wanted = from.count().min(to.space_for(from.kind()));
    let taken = from.take(wanted);
    let added = to.add(from.kind(), taken);
    debug_assert_eq!(taken, added, "conservation des objets");
    added
}

/// Déplace le plus d'objets possible d'un emplacement de machine vers un inventaire.
pub fn take_from_slot(from: &mut MachineSlot, to: &mut Inventory) -> u32 {
    let Some(kind) = from.kind() else {
        return 0;
    };
    let wanted = from.count().min(to.space_for(kind));
    let taken = from.take(wanted);
    let added = to.add(kind, taken);
    debug_assert_eq!(taken, added, "conservation des objets");
    added
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_fills_existing_stacks_before_empty_slots() {
        let mut inv = Inventory::new(3);
        assert_eq!(inv.add(ItemKind::IronOre, 30), 30);
        assert_eq!(inv.add(ItemKind::CopperOre, 10), 10);
        assert_eq!(inv.add(ItemKind::IronOre, 80), 80);
        // 30 + 70 dans la 1re case, 10 dans la 3e (la 2e contient le cuivre).
        assert_eq!(
            inv.slots(),
            [
                Some(ItemStack {
                    kind: ItemKind::IronOre,
                    count: 100
                }),
                Some(ItemStack {
                    kind: ItemKind::CopperOre,
                    count: 10
                }),
                Some(ItemStack {
                    kind: ItemKind::IronOre,
                    count: 10
                }),
            ]
        );
        assert_eq!(inv.count(ItemKind::IronOre), 110);
    }

    #[test]
    fn add_stops_when_full() {
        let mut inv = Inventory::new(2);
        assert_eq!(inv.space_for(ItemKind::IronOre), 200);
        assert_eq!(inv.add(ItemKind::IronOre, 250), 200);
        assert_eq!(inv.space_for(ItemKind::IronOre), 0);
        assert_eq!(inv.add(ItemKind::CopperOre, 1), 0);
    }

    #[test]
    fn transfer_conserves_items() {
        let mut buffer = ItemBuffer::new(ItemKind::IronOre, 50);
        buffer.add(50);
        let mut inv = Inventory::new(1);
        inv.add(ItemKind::IronOre, 80); // place pour 20 seulement

        let before = buffer.count() + inv.count(ItemKind::IronOre);
        assert_eq!(transfer(&mut buffer, &mut inv), 20);
        assert_eq!(buffer.count(), 30, "le reste attend dans la machine");
        assert_eq!(buffer.count() + inv.count(ItemKind::IronOre), before);
    }

    #[test]
    fn transfer_into_a_full_inventory_moves_nothing() {
        let mut buffer = ItemBuffer::new(ItemKind::CopperOre, 50);
        buffer.add(12);
        let mut inv = Inventory::new(1);
        inv.add(ItemKind::IronOre, 5);
        assert_eq!(transfer(&mut buffer, &mut inv), 0);
        assert_eq!(buffer.count(), 12);
    }

    #[test]
    fn remove_takes_from_the_last_slots_and_frees_them() {
        let mut inv = Inventory::new(3);
        inv.add(ItemKind::IronOre, 150);
        assert_eq!(inv.remove(ItemKind::IronOre, 60), 60);
        assert_eq!(inv.count(ItemKind::IronOre), 90);
        assert_eq!(inv.slots()[1], None, "la 2e case (50) a été vidée");
        assert_eq!(inv.remove(ItemKind::IronOre, 500), 90);
        assert_eq!(inv.remove(ItemKind::CopperOre, 1), 0);
    }

    #[test]
    fn machine_slot_holds_one_kind_and_forgets_it_when_empty() {
        let mut slot = MachineSlot::new(10);
        assert_eq!(slot.insert(ItemKind::IronOre, 4), 4);
        assert_eq!(slot.insert(ItemKind::CopperOre, 4), 0, "autre type refusé");
        assert_eq!(slot.insert(ItemKind::IronOre, 20), 6);
        assert_eq!(slot.take(10), 10);
        assert_eq!(slot.kind(), None);
        assert_eq!(slot.insert(ItemKind::CopperOre, 3), 3);
    }

    #[test]
    fn slot_transfers_conserve_items() {
        let mut inv = Inventory::new(1);
        inv.add(ItemKind::IronOre, 70);
        let mut slot = MachineSlot::new(50);
        let removed = inv.remove(ItemKind::IronOre, slot.space_for(ItemKind::IronOre));
        assert_eq!(slot.insert(ItemKind::IronOre, removed), 50);
        assert_eq!(inv.count(ItemKind::IronOre) + slot.count(), 70);

        // L'unique case est remplie de cuivre : plus de place pour le fer.
        inv.remove(ItemKind::IronOre, 20);
        inv.add(ItemKind::CopperOre, 100);
        assert_eq!(take_from_slot(&mut slot, &mut inv), 0);
        assert_eq!(slot.count(), 50, "rien n'a quitté la machine");

        inv.remove(ItemKind::CopperOre, 100);
        assert_eq!(take_from_slot(&mut slot, &mut inv), 50);
        assert_eq!(inv.count(ItemKind::IronOre), 50);
        assert_eq!(slot.kind(), None);
    }

    #[test]
    fn quantities_agree_in_number() {
        assert_eq!(ItemKind::IronOre.quantity(0), "0 minerai de fer");
        assert_eq!(ItemKind::IronOre.quantity(1), "1 minerai de fer");
        assert_eq!(ItemKind::CopperOre.quantity(12), "12 minerais de cuivre");
    }

    fn stack(kind: ItemKind, count: u32) -> Option<ItemStack> {
        Some(ItemStack { kind, count })
    }

    #[test]
    fn drag_moves_merges_or_swaps() {
        let mut inv = Inventory::new(4);
        inv.add_at(0, ItemKind::IronOre, 70);
        inv.add_at(1, ItemKind::IronOre, 50);
        inv.add_at(2, ItemKind::CopperOre, 5);

        inv.move_within(0, 3); // vers une case vide : déplacement
        assert_eq!(
            (inv.slot(0), inv.slot(3)),
            (None, stack(ItemKind::IronOre, 70))
        );
        inv.move_within(1, 3); // même type : on complète jusqu'à 100
        assert_eq!(
            (inv.slot(1), inv.slot(3)),
            (stack(ItemKind::IronOre, 20), stack(ItemKind::IronOre, 100))
        );
        inv.move_within(2, 1); // types différents : échange
        assert_eq!(
            (inv.slot(1), inv.slot(2)),
            (stack(ItemKind::CopperOre, 5), stack(ItemKind::IronOre, 20))
        );
        assert_eq!(inv.count(ItemKind::IronOre), 120, "rien de perdu");
    }

    #[test]
    fn drag_between_two_inventories_conserves_items() {
        let mut player = Inventory::new(2);
        let mut chest = Inventory::new(2);
        player.add_at(0, ItemKind::IronPlate, 30);
        chest.add_at(1, ItemKind::IronPlate, 90);
        move_between(&mut player, 0, &mut chest, 1);
        assert_eq!(chest.slot(1), stack(ItemKind::IronPlate, 100));
        assert_eq!(player.slot(0), stack(ItemKind::IronPlate, 20));
        assert_eq!(
            player.count(ItemKind::IronPlate) + chest.count(ItemKind::IronPlate),
            120
        );
    }

    #[test]
    fn slot_level_add_and_take_respect_kind_and_size() {
        let mut inv = Inventory::new(2);
        assert_eq!(inv.add_at(0, ItemKind::IronOre, 150), 100);
        assert_eq!(inv.add_at(0, ItemKind::CopperOre, 1), 0);
        assert_eq!(inv.space_at(1, ItemKind::CopperOre), 100);
        assert_eq!(inv.take_at(0, 30), 30);
        assert_eq!(inv.take_at(1, 5), 0);
        assert_eq!(inv.take_at(0, 500), 70);
        assert_eq!(inv.slot(0), None);
    }

    #[test]
    fn participles_agree_in_gender_and_number() {
        assert_eq!(ItemKind::IronOre.agree(1, "récupéré"), "récupéré");
        assert_eq!(ItemKind::IronOre.agree(4, "déposé"), "déposés");
        assert_eq!(ItemKind::IronPlate.agree(1, "récupéré"), "récupérée");
        assert_eq!(ItemKind::CopperPlate.agree(12, "récupéré"), "récupérées");
    }

    #[test]
    fn buffer_is_bounded() {
        let mut buffer = ItemBuffer::new(ItemKind::CopperOre, 5);
        assert_eq!(buffer.add(3), 3);
        assert_eq!(buffer.add(10), 2);
        assert!(buffer.is_full());
        assert_eq!(buffer.take(2), 2);
        assert_eq!(buffer.take_all(), 3);
        assert_eq!(buffer.take(1), 0);
    }
}
