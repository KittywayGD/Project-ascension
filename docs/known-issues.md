# Limites et bugs connus (V0)

> V0 = build de test personnel, sans sauvegarde ni progression. Les points ci-dessous sont connus et assumés pour l'instant.

## Jeu

| Sujet | Comportement actuel | Contournement / suite |
|---|---|---|
| Sauvegarde | Aucune : fermer le jeu perd la partie. | V0.1. `--demo` reconstruit une usine en 2 secondes. |
| Production hors ligne | Rien ne tourne quand le jeu est fermé. | Choix V0, à rediscuter en V0.1. |
| Démontage | Le contenu d'un bâtiment ou d'un tapis démonté est perdu (mode créatif). | Vider la machine (E, « Tout prendre ») avant de démonter. |
| Convoyeurs | Toujours droits ; ils ne vérifient pas s'ils traversent un bâtiment ou un rocher, seulement le relief. | Les tracer à l'écart, ou poser un coffre intermédiaire. |
| Ports | Un seul convoyeur par entrée et par sortie. | Plusieurs hubs, ou un coffre comme relais. |
| Pentes | À pied, on gravit n'importe quelle pente. | Limite de pente à ajouter si gênant. |
| Glisser-déposer | Pas de séparation de pile, pas de Maj+clic, pas de jet d'objet. | Boutons « Tout prendre », « Tout ranger », « Déposer du minerai ». |
| Énergie | Pas de stockage (batteries), générateurs sans carburant. | Prévu plus tard. |
| Affichage lointain | Les bâtiments réapparaissent d'un coup à 150 m (un peu avant le brouillard). | Voulu pour le M6 : réglable via `VIEW_LOAD_M` / `VIEW_UNLOAD_M` dans `building/mod.rs`. |
| Visée d'un module | Un module est préféré au bâtiment s'il est touché jusqu'à 1 m derrière l'entrée dans la boîte du bâtiment (approximation). | Viser franchement le toit. |

## Technique

| Sujet | Détail |
|---|---|
| Fermeture | Bevy affiche « Skipped event Destroyed for unknown winit Window Id » en quittant : sans conséquence. |
| Liaison dynamique | `./target/debug/project-ascension` ne démarre pas seul : passer par `cargo dev` (ou `cargo run --release`). |
| Touches F (macOS) | Sur certains claviers Mac, F3 demande Fn+F3. |
| Terrain | Les 64 chunks restent affichés : à 512 m, aucune mesure ne justifie de découper par distance ou de faire du LOD. |
| CI | Le workflow GitHub n'a jamais été exécuté (pas de dépôt distant). |

## Performances mesurées (MacBook, Apple M4, profil dev)

| Mesure | Valeur |
|---|---|
| Génération du monde 512 m (relief, rochers, gisements) | ≈ 10 ms |
| Maillage des 64 chunks | ≈ 20 ms |
| Tick de simulation, 1 000 bâtiments + 250 convoyeurs | ≈ 38 µs (0,08 % d'un cœur à 20 Hz) |
| Pose de ces 1 000 bâtiments (recalculs du réseau compris) | ≈ 165 ms |

Le test de charge se relance avec `cargo test --features dev stress -- --ignored --nocapture`. En jeu, l'overlay F3 affiche le coût d'un tick et la charge CPU.

## Faible FPS et pause

- La simulation avance par **ticks fixes** (20 Hz) : à faible FPS, Bevy rattrape les ticks manqués, et le résultat ne dépend que du nombre de ticks (test `result_depends_only_on_the_number_of_ticks`).
- La **pause** (Échap) fige tout ; un **menu ouvert** fige le joueur mais pas l'usine.
- Le déplacement du joueur limite le pas de temps à 0,1 s : après un gros ralentissement, il ne « téléporte » pas.
