# Génération du monde

> État : **M2** — `WORLDGEN_VERSION = 2` (v1 : relief + rochers ; v2 : ajout des gisements, relief et rochers inchangés).

## Garantie

**Même seed + même `WorldGenConfig` + même `WORLDGEN_VERSION` → même heightmap, au bit près.**

Le test `reference_seed_checksum` (`src/world/generation.rs`) fige l'empreinte de la seed 42.
S'il casse, le relief a changé : vérifier que c'est voulu, **incrémenter `WORLDGEN_VERSION`**, puis mettre à jour la valeur de référence.

L'empreinte de la heightmap est aussi affichée dans les logs au lancement, ce qui permet de comparer deux exécutions.

## Pipeline

1. **Bruit** (`noise.rs`) : Perlin 2D maison, permutation mélangée par SplitMix64 à partir de la seed.
2. **fBm** : 5 octaves (longueur d'onde de base 256 m, persistance 0,5, lacunarité 2), avec un décalage aléatoire par octave. La somme est normalisée dans [-1, 1], puis décalée de `land_bias`.
3. **Côtes** : sur les `coast_width_m` derniers mètres, l'élévation glisse (smoothstep) vers `border_elevation`. Le bord de la carte est donc toujours sous la mer, ce qui forme la frontière naturelle du monde.
4. **Courbe** : `h = e·|e| · max_height_m`. Les plaines restent douces et les reliefs plus marqués ; la courbe est symétrique sous la mer.
5. **Échantillonnage** : une grille de `(size/cell + 1)²` points, centrée sur l'origine, avec la mer à y = 0.

Avec la configuration par défaut (512 m, 1 m par cellule), la génération prend quelques dizaines de millisecondes.

## Pourquoi un bruit maison plutôt qu'une crate

Une mise à jour de crate peut changer le relief d'une seed sans prévenir. Le code est ici court (≈ 60 lignes) et figé avec le projet.

**Règle :** dans `noise.rs` et `generation.rs`, n'utiliser que des opérations IEEE exactes (`+ - * /`, `floor`, comparaisons) et des polynômes. Pas de `sin`, `cos`, `exp`, `powf`, `sqrt` dans le calcul des hauteurs : leur résultat peut varier d'une plateforme ou d'une libm à l'autre.

## Terrain affiché et sol du joueur

- La carte est découpée en **chunks de 64 × 64 cellules** (8 × 8 chunks pour 512 m). Leurs indices dépendent de la carte, pas de la caméra.
- Tous les chunks lisent la même heightmap, hauteurs **et** normales (différences centrées). Les sommets partagés sont donc identiques et il n'y a pas de fissures (test `neighbouring_chunks_share_identical_border_vertices`).
- Chaque cellule est coupée selon la diagonale (i, j) → (i+1, j+1). `HeightField::height_at` interpole **sur ces mêmes triangles** : le joueur marche exactement sur la surface affichée, sans moteur physique.
- Les couleurs de sommets dépendent de l'altitude (fond marin, sable, herbe) et de la pente (roche au-delà d'environ 30°).

## Rochers destructibles

- Ce sont des **objets distincts du terrain** (`src/world/rocks.rs`) : en détruire un ne touche jamais la heightmap ni les chunks.
- **Placement** : une grille de cases de 16 m, avec au plus un rocher par case (probabilité 30 %), sur la terre ferme (≥ 1 m), sur pente douce et à plus de 8 m du point d'apparition. Chaque case consomme toujours 6 tirages aléatoires : changer une règle de rejet ne déplace pas les rochers des autres cases.
- Le placement fait partie de la génération : il suit les mêmes règles de déterminisme et le test `reference_seed_rocks` le fige. Les collisions et la visée sont du gameplay ; elles peuvent utiliser `sqrt`.
- **Identifiants** : `RockId` = rang dans l'ordre de génération, donc stable pour une seed et une version données. L'état à sauvegarder (V0.1) est l'ensemble `destroyed` des identifiants détruits.
- **Forme** : un ellipsoïde de révolution (rayon 0,5 à 2 m), enfoncé d'un quart de sa hauteur. La visée est un lancer de rayon exact sur cet ellipsoïde ; la collision traite le rocher comme un cylindre. On enjambe un rocher qui dépasse les pieds de moins de 45 cm, sinon il repousse.

## Limites connues (à traiter plus tard)

- Tous les chunks sont affichés en permanence ; l'affichage selon la distance de vue viendra quand la carte grandira (à mesurer d'abord).
- Il n'y a pas encore de biomes. Les gisements sont décrits dans [`simulation.md`](simulation.md).
- ~~La visée ne tient pas compte du relief~~ : depuis M2, la visée prend le plus proche entre le terrain, les rochers et les bâtiments.
- Les collisions avec les rochers parcourent toute la liste (≈ 90 rochers) ; une grille spatiale viendra si le nombre augmente.
- À pied, on gravit n'importe quelle pente. Une limite de pente sera ajoutée si besoin.
