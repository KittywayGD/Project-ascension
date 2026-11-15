//! Réseau électrique, en Rust pur : connexions, réseaux et satisfaction.
//!
//! Unités : **puissance en watts** (`_w`, entiers), énergie en joules (`_j`), durée en ticks.
//!
//! - **Connexions** : un poteau relie tout bâtiment (machine ou poteau) dont le centre est à
//!   moins de `POLE_RANGE_M`. Deux machines ne se relient jamais directement.
//! - **Réseaux** : composantes connexes du graphe de connexions. Ils ne sont recalculés
//!   qu'à la pose ou au retrait d'un bâtiment, jamais à chaque tick.
//! - **Satisfaction** : à chaque tick, `p = min(1000, 1000 × produit / demandé)` millièmes,
//!   arrondi à l'entier inférieur. Toutes les machines d'un réseau tournent à `p` : en
//!   surcharge, elles ralentissent toutes ensemble (règle proportionnelle).

use std::collections::BTreeMap;

use super::production::FULL_POWER;
use super::sim_world::BuildingId;

/// Portée de connexion d'un poteau, en mètres (entre centres).
pub const POLE_RANGE_M: f32 = 10.0;

/// Puissance fournie par un générateur allumé, en watts.
pub const GENERATOR_POWER_W: u64 = 100_000;

/// Un bâtiment vu par le réseau électrique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PowerNode {
    pub id: BuildingId,
    pub x: f32,
    pub z: f32,
    pub is_pole: bool,
}

/// Identifiant d'un réseau : rang du réseau trié par plus petit membre. Valable jusqu'au
/// prochain recalcul (sert à l'affichage, pas à la sauvegarde).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NetworkId(pub u32);

/// Bilan d'un réseau au dernier tick.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkStats {
    pub members: Vec<BuildingId>,
    pub produced_w: u64,
    pub demanded_w: u64,
    /// Alimentation des machines du réseau, en millièmes.
    pub satisfaction: u32,
}

/// Connexions et découpage en réseaux, recalculés à chaque changement de bâtiments.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PowerGrid {
    edges: Vec<(BuildingId, BuildingId)>,
    network_of: BTreeMap<BuildingId, NetworkId>,
    networks: Vec<NetworkStats>,
}

impl PowerGrid {
    pub fn build(nodes: &[PowerNode]) -> Self {
        let mut nodes = nodes.to_vec();
        nodes.sort_by_key(|n| n.id);
        let range_sq = POLE_RANGE_M * POLE_RANGE_M;

        let mut edges = Vec::new();
        for (i, a) in nodes.iter().enumerate() {
            for b in &nodes[i + 1..] {
                let (dx, dz) = (a.x - b.x, a.z - b.z);
                if (a.is_pole || b.is_pole) && dx * dx + dz * dz <= range_sq {
                    edges.push((a.id, b.id));
                }
            }
        }

        // Union-find sur les indices (nœuds triés par identifiant).
        let index: BTreeMap<BuildingId, usize> =
            nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
        let mut parent: Vec<usize> = (0..nodes.len()).collect();
        fn root(parent: &mut [usize], mut i: usize) -> usize {
            while parent[i] != i {
                parent[i] = parent[parent[i]];
                i = parent[i];
            }
            i
        }
        for (a, b) in &edges {
            let (ra, rb) = (root(&mut parent, index[a]), root(&mut parent, index[b]));
            // La plus petite racine gagne : résultat indépendant de l'ordre des arêtes.
            let (low, high) = (ra.min(rb), ra.max(rb));
            parent[high] = low;
        }

        // Réseaux numérotés dans l'ordre de leur plus petit membre.
        let mut network_of = BTreeMap::new();
        let mut networks: Vec<NetworkStats> = Vec::new();
        let mut by_root: BTreeMap<usize, NetworkId> = BTreeMap::new();
        for (i, node) in nodes.iter().enumerate() {
            let r = root(&mut parent, i);
            let id = *by_root.entry(r).or_insert_with(|| {
                networks.push(NetworkStats {
                    satisfaction: FULL_POWER,
                    ..NetworkStats::default()
                });
                NetworkId(networks.len() as u32 - 1)
            });
            networks[id.0 as usize].members.push(node.id);
            network_of.insert(node.id, id);
        }

        Self {
            edges,
            network_of,
            networks,
        }
    }

    /// Paires de bâtiments reliés par un câble.
    pub fn edges(&self) -> &[(BuildingId, BuildingId)] {
        &self.edges
    }

    pub fn network_of(&self, id: BuildingId) -> Option<NetworkId> {
        self.network_of.get(&id).copied()
    }

    pub fn network(&self, id: NetworkId) -> Option<&NetworkStats> {
        self.networks.get(id.0 as usize)
    }

    pub fn networks(&self) -> &[NetworkStats] {
        &self.networks
    }

    /// Enregistre le bilan du tick et calcule la satisfaction de chaque réseau.
    /// `balance` donne, pour chaque bâtiment, (puissance produite, puissance demandée).
    pub fn update_balance(&mut self, balance: impl IntoIterator<Item = (BuildingId, u64, u64)>) {
        for network in &mut self.networks {
            network.produced_w = 0;
            network.demanded_w = 0;
        }
        for (id, produced, demanded) in balance {
            if let Some(n) = self.network_of.get(&id) {
                let network = &mut self.networks[n.0 as usize];
                network.produced_w += produced;
                network.demanded_w += demanded;
            }
        }
        for network in &mut self.networks {
            network.satisfaction = satisfaction(network.produced_w, network.demanded_w);
        }
    }

    /// Alimentation (millièmes) du réseau d'un bâtiment ; 0 s'il n'est dans aucun réseau.
    pub fn satisfaction_of(&self, id: BuildingId) -> u32 {
        self.network_of(id)
            .and_then(|n| self.network(n))
            .map_or(0, |n| n.satisfaction)
    }
}

/// `min(1000, 1000 × produit / demandé)`, arrondi à l'inférieur ; 1000 sans demande.
pub fn satisfaction(produced_w: u64, demanded_w: u64) -> u32 {
    let full = u64::from(FULL_POWER);
    (produced_w * full)
        .checked_div(demanded_w)
        .map_or(FULL_POWER, |p| p.min(full) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: u64, x: f32, is_pole: bool) -> PowerNode {
        PowerNode {
            id: BuildingId(id),
            x,
            z: 0.0,
            is_pole,
        }
    }

    #[test]
    fn machines_need_a_pole_to_connect() {
        // Deux machines côte à côte, sans poteau : deux réseaux isolés.
        let grid = PowerGrid::build(&[node(0, 0.0, false), node(1, 3.0, false)]);
        assert!(grid.edges().is_empty());
        assert_eq!(grid.networks().len(), 2);

        let grid =
            PowerGrid::build(&[node(0, 0.0, false), node(1, 3.0, false), node(2, 1.5, true)]);
        assert_eq!(grid.networks().len(), 1);
        assert_eq!(
            grid.network_of(BuildingId(0)),
            grid.network_of(BuildingId(1))
        );
    }

    #[test]
    fn poles_chain_within_range_only() {
        let grid = PowerGrid::build(&[
            node(0, 0.0, false),
            node(1, 8.0, true),
            node(2, 17.0, true),
            node(3, 26.0, false),
            node(4, 40.0, true), // trop loin de tout
        ]);
        assert_eq!(
            grid.network_of(BuildingId(0)),
            grid.network_of(BuildingId(3))
        );
        assert_ne!(
            grid.network_of(BuildingId(0)),
            grid.network_of(BuildingId(4))
        );
    }

    #[test]
    fn loops_do_not_double_count() {
        // Trois poteaux en triangle + une machine : un seul réseau, chaque membre une fois.
        let nodes = [
            PowerNode {
                id: BuildingId(0),
                x: 0.0,
                z: 0.0,
                is_pole: true,
            },
            PowerNode {
                id: BuildingId(1),
                x: 6.0,
                z: 0.0,
                is_pole: true,
            },
            PowerNode {
                id: BuildingId(2),
                x: 3.0,
                z: 5.0,
                is_pole: true,
            },
            PowerNode {
                id: BuildingId(3),
                x: 3.0,
                z: 2.0,
                is_pole: false,
            },
        ];
        let mut grid = PowerGrid::build(&nodes);
        assert_eq!(grid.networks().len(), 1);
        assert_eq!(grid.networks()[0].members.len(), 4);
        grid.update_balance([(BuildingId(3), 0, 50_000), (BuildingId(0), 100_000, 0)]);
        assert_eq!(grid.networks()[0].demanded_w, 50_000);
        assert_eq!(grid.satisfaction_of(BuildingId(3)), 1000);
    }

    #[test]
    fn result_does_not_depend_on_node_order() {
        let a = [node(0, 0.0, false), node(1, 5.0, true), node(2, 9.0, false)];
        let mut b = a;
        b.reverse();
        assert_eq!(PowerGrid::build(&a), PowerGrid::build(&b));
    }

    #[test]
    fn satisfaction_is_proportional_and_capped() {
        assert_eq!(satisfaction(100_000, 0), 1000);
        assert_eq!(satisfaction(100_000, 80_000), 1000);
        assert_eq!(satisfaction(100_000, 110_000), 909);
        assert_eq!(satisfaction(0, 30_000), 0);
    }
}
