# Limites et bugs connus (V0)

> V0.1 : sauvegardes et arbre techno primitif. Les points ci-dessous sont connus et assumés pour l'instant.

## Jeu

| Sujet | Comportement actuel | Contournement / suite |
|---|---|---|
| Sauvegarde | Une partie lancée par `--seed` ou `--demo` sans `--name` n'est pas sauvegardée tant qu'on n'appuie pas sur F5. | Créer le monde depuis l'écran titre, ou ajouter `--name`. |
| Production hors ligne | Rien ne tourne quand le jeu est fermé (choix assumé). | — |
| Démontage | Ce qui ne tient pas dans l'inventaire est perdu (message dans le bandeau). | Faire de la place avant de démonter un coffre plein. |
| Kit de départ | Livrer les 50 plaques du kit au hub débloque tout de suite les convoyeurs (mais il ne reste rien pour construire). | À rééquilibrer après une vraie partie. |
| Changement de générateur | Une sauvegarde d'une autre version du générateur de monde est refusée (le relief serait différent). | Pas de conversion pour l'instant. |
| Convoyeurs | Toujours droits ; ils ne vérifient pas s'ils traversent un bâtiment ou un rocher, seulement le relief. | Les tracer à l'écart, ou poser un coffre intermédiaire. |
| Ports | Un seul convoyeur par entrée et par sortie. | Plusieurs hubs, ou un coffre comme relais. |
| Pentes | À pied, on gravit n'importe quelle pente. | Limite de pente à ajouter si gênant. |
| Glisser-déposer | Pas de séparation de pile, pas de Maj+clic, pas de jet d'objet. | Boutons « Tout prendre », « Tout ranger », « Déposer du minerai ». |
| Énergie | Pas de stockage (batteries), générateurs sans carburant. | Prévu plus tard. |
| Affichage lointain | Les bâtiments réapparaissent d'un coup à la distance d'affichage (150 m par défaut). | Réglable dans Options → « Distance d'affichage des bâtiments » (75 à 400 m). |
| Visée d'un module | Un module est préféré au bâtiment s'il est touché jusqu'à 1 m derrière l'entrée dans la boîte du bâtiment (approximation). | Viser franchement le toit. |

## Technique

| Sujet | Détail |
|---|---|
| Fermeture | Bevy affiche « Skipped event Destroyed for unknown winit Window Id » en quittant : sans conséquence. |
| Liaison dynamique | `./target/debug/project-ascension` ne démarre pas seul : passer par `cargo dev` (ou `cargo run --release`). |
| Touches F (macOS) | Sur certains claviers Mac, F3 demande Fn+F3. |
| Terrain | Les 64 chunks restent affichés : à 512 m, aucune mesure ne justifie de découper par distance ou de faire du LOD. |
| CI | Le workflow GitHub n'a jamais été exécuté (pas de dépôt distant). |
| Rendu | Ciel atmosphérique, bloom et ombres en cascade : 60 FPS (vsync) sur Apple M4. Non mesuré sur une machine plus modeste. |
| Captures | `--screenshot` bascule la partie en jeu et capture la souris quelques secondes. |

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
