# Project Ascension

Sandbox industriel 3D en Rust ([Bevy](https://bevy.org) 0.20) : exploration, construction et automatisation sur une carte procédurale.

> **Statut :** **V0 jouable** (M0 à M7) : terrain, gisements, énergie, modules, convoyeurs, hub et objectifs, simulation hors champ, aide et scène de démo. Prochaine étape : V0.1 (sauvegardes + arbre techno).
> Feuille de route complète : [`TODO.md`](TODO.md).

## Installation

1. Installer Rust via [rustup](https://rustup.rs) (version minimale : **1.97.1**, imposée par Bevy 0.20) :

   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. Cloner le dépôt puis, depuis sa racine :

   ```bash
   cargo dev
   ```

   La première compilation de Bevy prend plusieurs minutes ; les suivantes sont rapides.

Sur Linux, Bevy demande aussi quelques bibliothèques système (`libasound2-dev`, `libudev-dev`, `libwayland-dev`, `libxkbcommon-dev`).

## Démo

Une usine complète et automatique, en deux secondes :

```bash
cargo dev -- --seed 42 --demo
```

La scène pose, près du point d'apparition : extracteur sur le gisement de départ → convoyeur → four → convoyeur → hub, alimentés par deux générateurs via un poteau, avec des modules de vitesse et d'efficacité. Pour la voir tourner :

1. Clique pour jouer, puis repère les bâtiments (l'overlay F3 indique le gisement de départ).
2. **T** accélère le temps (×10, ×100) : l'objectif en haut à droite avance jusqu'à la notification « 50 plaques de fer livrées ».
3. **E** sur une machine ouvre son panneau : états, débits, modules, réseau.
4. Éloigne-toi à plus de 170 m (**V** pour voler) : l'overlay montre l'affichage déchargé, et la production continue. Reviens : tout réapparaît.
5. **H** affiche toutes les commandes.

Build optimisée à partager : `cargo build --release`, puis `./target/release/project-ascension --demo`.
Limites connues : [`docs/known-issues.md`](docs/known-issues.md).

## Commandes

| Commande | Rôle |
|---|---|
| `cargo dev` | Lance le jeu en développement (liaison dynamique de Bevy, recompilation rapide), sur une seed aléatoire |
| `cargo dev -- --seed 42` | Lance le jeu sur une seed précise (même seed → même monde) |
| `cargo dev -- --demo` | Lance le jeu avec la scène de démonstration |
| `cargo test --features dev stress -- --ignored --nocapture` | Test de charge (1 000 bâtiments) |
| `cargo run --release` | Lance une build optimisée (plus longue à compiler) |
| `cargo test` | Tests unitaires (génération, déterminisme, simulation, déplacement) |
| `cargo fmt --check` | Vérifie le formatage |
| `cargo clippy --all-targets -- -D warnings` | Lint, avertissements traités comme des erreurs |

## Contrôles

Les touches sont **physiques** : sur un clavier AZERTY, W/A/S/D correspondent automatiquement à Z/Q/S/D.

| Touche | Action |
|---|---|
| Clic gauche | Capturer la souris et jouer |
| Souris | Regarder autour |
| Z Q S D (AZERTY) / W A S D (QWERTY) | Avancer, gauche, reculer, droite |
| V | Basculer entre marche et vol |
| Espace | Sauter (marche) / monter (vol) |
| Ctrl gauche | Descendre (vol) |
| Maj | Courir (marche) / accélérer ×4 (vol) |
| Échap | Pause / reprendre, ou fermer le menu ouvert |
| Maj + Échap | Quitter |
| Clic gauche (en jeu) | Détruire le rocher visé (surligné, portée 12 m) |
| Tab | Inventaire (glisser-déposer les cases pour les déplacer) |
| E (en visant un bâtiment) | Ouvrir son panneau : état, réseau, entrée/sortie, marche/arrêt |
| B | Menu de construction : bâtiment, convoyeur ou module |
| Clic gauche / droit (construction) | Poser / démonter (ou annuler un convoyeur en cours, retirer un module visé) |
| R (construction) | Tourner de 90° |
| Échap (construction) | Arrêter de construire |
| H | Aide : toutes les commandes, légende des voyants |
| G | +50 minerais de fer et de cuivre (outil de test) |
| F3 | Afficher / masquer l'overlay debug |
| T | Vitesse du temps ×1 / ×10 / ×100 (outil de test) |
| I / O | Remplir la machine visée (+10) / vider sa sortie (outil de test) |

## Conventions

- 1 unité = **1 mètre**, axe **Y vers le haut**, repère main droite de Bevy.
- La simulation avance à **pas fixe** (20 Hz, provisoire), indépendamment des FPS, et s'arrête en pause.
- Le temps simulé est compté en **ticks entiers** (`src/simulation/clock.rs`), sans dépendance à Bevy.
- Le monde est centré sur l'origine, la mer est à y = 0. La génération est déterministe : voir [`docs/worldgen.md`](docs/worldgen.md).
- Gisements infinis, débits entiers, placement : voir [`docs/simulation.md`](docs/simulation.md).

## Organisation

```text
src/
├── main.rs          # assemblage de l'application
├── config.rs        # options de lancement (--seed)
├── app.rs           # état en jeu / pause, souris, fermeture
├── world/           # bruit, heightmap, rochers, gisements (Rust pur), chunks, mer, soleil
├── player/          # marche (gravité, saut, collisions) et vol
├── interaction.rs   # réticule, visée (terrain, rocher, bâtiment), destruction des rochers
├── building/        # mode construction ; règles de placement en Rust pur
├── collision.rs     # obstacles cercle / rectangle, sans moteur physique
├── inventory.rs     # objets, piles, inventaire, transferts (Rust pur)
├── ui/              # menus : inventaire et panneaux de machine, aide sous le réticule
├── simulation/      # SimWorld à pas fixe : machines, modules, convoyeurs, réseau électrique (Rust pur)
└── debug/           # overlay F3 et bandeau de pause
```
