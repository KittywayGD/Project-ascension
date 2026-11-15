# Project Ascension — TODO.md

> **Projet :** sandbox industriel 3D en **Rust**, centré sur l'exploration, la construction et l'automatisation.
>
> **Statut :** conception / préprototype  
> **Approche :** développer une **vertical slice** jouable, puis élargir progressivement.  
> **V0 :** build de test personnel — pas de sauvegarde, pas de progression.  
> **V0.1 :** arbre de technologies primitif + sauvegardes.  
> **Légende :** `[ ]` à faire · `[x]` fait · `⭐` indispensable à la V0 · `↗` évolution ultérieure · `⚠️` risque à surveiller.

---

## 0. Vision et règles du projet

### Identité du jeu

> Principes directeurs, pas des tâches : à relire à chaque jalon.

- Description en une phrase : *« Un sandbox industriel 3D sur une carte naturelle procédurale, où l'on assemble des machines modulaires pour exploiter et automatiser des ressources infinies. »*
- Privilégier l'**ingénierie et l'automatisation** plutôt que la survie ou le farm manuel.
- Carte **finie**, **aléatoire**, **reproductible à partir d'une seed**.
- **Terrain lisse** (maillage 3D / heightmap), **pas** un univers de cubes destructibles.
- Terrain principal **non destructible** ; seuls certains rochers/obstacles sont des objets destructibles distincts.
- **Gisements inépuisables** dont le **débit** d'extraction est limité.
- **Machines fixes** avec **noyau + modules physiques connectables**.
- Les usines continuent à tourner **hors du champ de vision** du joueur.
- Pas de progression technologique en V0 ; un arbre primitif arrive en V0.1.
- Construction précise et agréable (accrochage / placement / rotation).

### Hors périmètre de la V0

- [ ] **↗** Arbre de technologies, paliers et recherche — **prévu en V0.1** (version primitive).
- [ ] **↗** Sauvegarde / chargement — **prévu en V0.1**.
- [ ] **↗** Écran titre et menus (la V0 se lance directement dans un monde).
- [ ] **↗** Trains, rails, signaux et gares.
- [ ] **↗** Drones et stations logistiques.
- [ ] **↗** Véhicules libres, machines roulantes et physique façon Besiege.
- [ ] **↗** Terrain entièrement déformable, minage voxel et tunnels creusés à volonté.
- [ ] **↗** Multijoueur, serveurs dédiés et synchronisation réseau.
- [ ] **↗** Fluides complexes, chimie et réseaux haute pression.
- [ ] **↗** Faune, combats, santé/faim et survie.
- [ ] **↗** Système de mods public, scripting joueur et marketplace.
- [ ] **↗** Carte immense ultra-détaillée avant d'avoir une boucle de gameplay fonctionnelle.

### Définition du succès

**Vertical slice V0 :** lancer le jeu, créer un monde avec une seed, se déplacer sur un terrain naturel, trouver un gisement, poser un extracteur, connecter une alimentation et une machine, transférer une ressource, ajouter/retirer un module visible, observer la production. La V0 est un **build de test personnel** : pas de sauvegarde ni de progression (voir V0.1).

- [ ] **⭐** La vertical slice fonctionne **sans intervention dans le code pendant la partie**.
- [ ] **⭐** La seed reproduit les mêmes caractéristiques du monde avec les mêmes paramètres/version de génération.
- [ ] **⭐** Une usine éloignée continue de produire au rythme prévu.
- [ ] **⭐** Les outils de test (accélération du temps, items et pose gratuits) permettent de vérifier un débit en quelques secondes.
- [ ] **⭐** Les erreurs ordinaires (manque d'énergie, sortie pleine, module invalide) ne font pas planter le jeu.

---

## 1. Décisions techniques à verrouiller

> Ne pas décider tout immédiatement. Écrire une courte décision dans `docs/decisions/` pour chaque choix structurant.

- [ ] **⭐** Valider **Bevy** comme moteur (ECS, rendu, entrée, scènes, UI initiale).
- [ ] **⭐** Vérifier la **compatibilité des versions** Rust/Bevy/physique avant de fixer `Cargo.toml` et `Cargo.lock`.
- [ ] **⭐** Choisir l'outil de physique **uniquement** pour les collisions utiles (joueur, sol, objets), sans surdimensionner — envisager **aucun moteur physique** en V0 (hauteur du sol lue sur la heightmap, raycast maison).
- [x] **⭐** Choisir un générateur de bruit déterministe pour les reliefs (Perlin maison, sans libm).
- [ ] **⭐** Définir **unités et conventions** : mètres, secondes, puissance et énergie, coordonnées et axes (fait pour mètres, axes et ticks ; puissance/énergie en M3).
- [x] **⭐** Définir un **tick de simulation fixe** indépendant de la fréquence d'affichage (20 Hz provisoire).
- [ ] **⭐** Décider si la V0 reste dans **un seul crate** organisé en modules (recommandé au départ).
- [ ] **⭐** Définir des identifiants stables pour la simulation : ne jamais utiliser les `Entity` Bevy comme clés (indispensable même sans sauvegarde, prépare la V0.1).
- [ ] **⭐** Garder la simulation dans une **struct Rust pure** (`SimWorld`) indépendante de Bevy ; Bevy lit cet état et affiche les meshes.
- [ ] **⭐** Utiliser des **entiers** dans la simulation (progression en ticks, énergie en `u64`, items en `u32`) pour éviter la dérive flottante.
- [ ] **⭐** Décrire bâtiments, recettes et modules dans des **fichiers de données** (RON ou TOML), modifiables sans recompiler.
- [ ] **⭐** Épingler la version de Bevy et ne monter de version qu'entre deux jalons.
- [x] **⭐** ~~Épingler la crate de bruit~~ Bruit maison figé avec le projet ; tout changement de relief incrémente `WorldGenVersion`.
- [ ] **↗** Choisir un format de sauvegarde versionné (V0.1).
- [ ] **⭐** Prévoir des paramètres de génération regroupés dans une structure versionnée.
- [ ] Documenter les décisions dans `docs/decisions/0001-architecture.md`.
- [ ] Écrire un mini glossaire : node, gisement, core, module, socket, réseau, tick, chunk, secteur, seed.

### Organisation du dépôt proposée

```text
project-ascension/
├── Cargo.toml
├── Cargo.lock
├── README.md
├── TODO.md
├── assets/
│   ├── data/          # bâtiments, recettes, modules (RON/TOML)
│   ├── materials/
│   ├── meshes/
│   ├── shaders/
│   └── ui/
├── docs/
│   ├── architecture.md
│   ├── save-format.md
│   ├── worldgen.md
│   └── decisions/
├── tests/
│   └── fixtures/
└── src/
    ├── main.rs
    ├── app.rs
    ├── config.rs
    ├── player/
    ├── world/
    │   ├── generation.rs
    │   ├── terrain.rs
    │   ├── chunks.rs
    │   └── resources.rs
    ├── building/
    ├── machines/
    │   ├── core.rs
    │   ├── modules.rs
    │   └── recipes.rs
    ├── simulation/
    │   ├── clock.rs
    │   ├── energy.rs
    │   ├── logistics.rs
    │   └── production.rs
    ├── persistence/   # V0.1
    ├── ui/
    └── debug/
```

> Le découpage est **indicatif**. Ne pas créer 20 crates ni une architecture de plugins compliquée avant d'en avoir besoin.

---

## 2. Jalons — ordre recommandé

| Jalon | Livrable visible | Critère de passage |
|---|---|---|
| **M0 — Boot** | Fenêtre 3D + contrôles | Lancement reproductible et propre |
| **M1 — Terrain** | Carte lisse procédurale | Seed stable, collision, déplacement |
| **M2 — Ressources** | Gisement + extracteur + placement | Extracteur posé en jeu, extraction périodique mesurable |
| **M3 — Industrie** | Générateur + consommateur | Production bloquée sans énergie |
| **M4 — Modules** | Noyau + modules visibles | Ajout/retrait modifie la simulation |
| **M5 — Logistique** | Transfert entre machines | Petite chaîne entièrement automatique |
| **M6 — Hors champ** | Simulation sans rendu | Usine éloignée produit au rythme prévu |
| **M7 — V0 jouable** | UX minimale, tests | Boucle complète jouable en test perso |
| **V0.1** | Arbre techno primitif + sauvegardes | Palier débloqué par livraisons ; partie identique après redémarrage |

> **Règle de progression :** ne pas lancer M(n+1) si le critère principal de M(n) n'est pas vérifié. On peut garder des petites expérimentations en parallèle, pas restructurer sans fin.

---

## M0 — Initialisation du projet ⭐

### Environnement

- [x] Installer/vérifier `rustup`, `cargo`, `rustfmt`, `clippy` (Rust 1.100.0).
- [x] Créer le dépôt Git et le projet Cargo.
- [x] Ajouter `.gitignore` adapté à Rust, IDE, caches et builds.
- [x] Définir une version minimale de Rust cohérente avec les dépendances retenues (`rust-version = "1.97.1"`, imposée par Bevy 0.20).
- [x] Ajouter Bevy (0.20.1) et démarrer une fenêtre 3D.
- [x] Réduire les temps de compilation : `[profile.dev] opt-level = 1`, `[profile.dev.package."*"] opt-level = 3`, feature `bevy/dynamic_linking` en dev (alias `cargo dev`).
- [x] Afficher un sol, un cube et une lumière.
- [x] Mettre en place une **caméra libre (vol)** en premier.
- [ ] **↗** Ajouter plus tard la marche FPS (gravité, hauteur du sol), avec un raccourci de bascule vol/marche.
- [x] Ajouter pause, reprise et fermeture propre (Échap, Maj+Échap, pause auto à la perte de focus).
- [x] Ajouter un overlay debug : FPS, position, état et temps simulé (F3).
- [x] Configurer un niveau de logs lisible (`info`, `warn`, `error`).
- [x] Définir un style de code et activer formatage/lint (rustfmt par défaut, clippy `-D warnings`).
- [x] Rédiger `README.md` avec installation, lancement et commandes.
- [x] Ajouter une CI : `cargo fmt --check`, `cargo clippy`, `cargo test` (`.github/workflows/ci.yml`, **pas encore exécutée** : aucun remote).

**Validation M0**

- [ ] Le jeu démarre depuis une installation propre suivant le README.
- [ ] La caméra libre se déplace et regarde autour sans à-coups.
- [ ] Aucune erreur au démarrage ; les checks CI passent.

---

## M1 — Monde fini, lisse et procédural ⭐

### Prototype du relief

- [x] Choisir une taille **très petite pour commencer** (512 × 512 m, 1 m par cellule).
- [x] Créer `WorldSeed`, `WorldGenConfig` et `WorldGenVersion`.
- [x] Écrire un générateur de hauteur **déterministe** (mêmes entrées → même sortie) : Perlin maison, voir `docs/worldgen.md`.
- [x] Générer un maillage de terrain à partir d'une heightmap.
- [x] Calculer normales et UV pour un rendu correct.
- [x] Ajouter une collision du terrain cohérente avec son maillage (`height_at` interpole sur les triangles affichés).
- [x] Ajouter eau/mer simplifiée pour visualiser les zones basses.
- [x] Ajouter une ou deux matières de sol dépendant de l'altitude ou de la pente (couleurs de sommets : fond marin, sable, herbe, roche).
- [x] Définir une frontière explicite du monde : bande côtière qui plonge sous la mer, eau profonde infranchissable à pied, bords de carte bloquants.
- [x] Afficher seed, dimensions et position dans le debug overlay.
- [x] Mode marche avec gravité et saut, bascule marche/vol (V), sans moteur physique.
- [x] Seed en argument : `cargo dev -- --seed 42` (aléatoire sinon, affichée dans l'overlay).
- [x] Vitesse réelle du joueur dans l'overlay (totale, horizontale, verticale).

### Terrain par secteurs / chunks

- [x] Découper le terrain en secteurs stables, indépendants de la caméra (chunks de 64 × 64 m).
- [x] Éviter les fissures visibles aux frontières des secteurs (heightmap et normales globales, test dédié).
- [x] Générer/préparer les maillages sans bloquer excessivement la frame principale (génération 8 ms + maillage 17 ms au démarrage).
- [ ] Afficher/masquer les secteurs selon la distance de vue (terrain : inutile à 512 m, tout reste affiché ; bâtiments : fait en M6).
- [ ] Libérer les ressources graphiques des secteurs non visibles (idem : bâtiments déchargés au-delà de 170 m).
- [x] **Ne pas supprimer les données de simulation** quand un secteur est masqué.
- [ ] Ajouter LOD **uniquement lorsque les mesures montrent qu'il est nécessaire**.

### Génération à plus grande échelle ↗ (après M5 / post-V0)

- [ ] Séparer la génération en phases : forme générale, relief, climat, biomes, ressources, végétation.
- [ ] Préparer masques de température, humidité et pente.
- [ ] Introduire progressivement 2–3 biomes, puis davantage si utile.
- [ ] Éviter les transitions absurdes entre régions.
- [ ] Définir des points de départ valides et accessibles.
- [ ] Tester une carte intermédiaire de **4 × 4 km**.
- [ ] Évaluer une carte cible de **8 × 8 km (64 km²)** comme hypothèse, **pas** comme obligation.
- [ ] Vérifier que l'ensemble est agréable à parcourir, pas seulement vaste.

### Rochers destructibles (sans terrain déformable)

- [x] Modéliser les rochers destructibles comme **objets distincts** du terrain (`world/rocks.rs`, placement déterministe, ID stables).
- [x] Ajouter état intact/détruit et collision associée (repoussé par les hauts, enjambe les bas, glisse dessus en vol).
- [x] Ajouter une interaction simple de destruction : réticule, rocher visé surligné à 8 m, clic gauche.
- [ ] **↗** Persister leur destruction dans la sauvegarde (V0.1).
- [x] S'assurer que détruire un rocher ne déclenche **aucune reconstruction du terrain** (test : empreinte du terrain inchangée).

**Validation M1**

- [x] Une seed et une config identiques produisent le même relief (test `reference_seed_checksum`, empreinte identique jeu/tests).
- [ ] Le joueur marche sur un terrain naturel sans trous visibles entre secteurs.
- [ ] Les limites de la carte sont gérées proprement.
- [x] Un rocher destructible peut disparaître sans modifier la géométrie du sol.

---

## M2 — Gisements infinis et extraction ⭐

### Modèle de ressource

- [x] Définir `ResourceKind` : **fer** (obligatoire) et cuivre (facultatif).
- [x] Définir `Deposit` : type, position, pureté, rayon (le point d'attache est le centre).
- [x] Documenter la règle : **réserve infinie, débit limité**, sans décrément de stock du gisement (`docs/simulation.md`).
- [x] Définir une unité de débit (unités/minute, accumulateur entier par tick : `⌊n × r / 1200⌋`).
- [x] Définir la relation entre pureté, extracteur et production (impur 15, normal 30, pur 60 /min).
- [x] Rendre gisements facilement identifiables en jeu (amas de pépites colorées, plus nombreuses si pur).
- [x] Ajouter affichage des caractéristiques d'un gisement ciblé (overlay F3 + bandeau construction).

### Placement procédural des ressources

- [x] Générer les positions à partir de la seed, indépendamment de l'ordre de chargement des secteurs (génération globale, `WORLDGEN_VERSION` 2).
- [x] Éviter les gisements hors carte, dans l'eau ou sur des pentes inaccessibles (emprise 3 × 3 m plate à 60 cm près).
- [x] Définir des distances minimales entre gisements selon leur type (70 m même type, 30 m tous types).
- [ ] **↗** Répartir les gisements par zones / biomes quand cela a du sens (pas de biomes en V0).
- [x] Prévoir un quota minimal **par type de ressource nécessaire au gameplay** (`ResourceKind::MANDATORY`).
- [x] Prévoir un **contrôle après génération** : toute ressource obligatoire est présente (`check_quotas`, testé sur 24 seeds).
- [x] Prévoir une correction déterministe ou une régénération ciblée si un quota échoue (fer de départ créé en spirale autour du spawn).
- [x] Vérifier l'accessibilité d'au moins un gisement de démarrage depuis le spawn (≤ 64 m, à pied sans eau profonde).
- [x] Ne pas introduire trop tôt de minerais rares ou de progression technologique.

### Placement minimal (avancé depuis M4)

- [x] Créer un mode construction (B) avec **fantôme** d'objet avant la pose.
- [x] Ajouter un raycast de sélection sur le terrain et un aperçu valide/invalide (vert/rouge + motif du refus).
- [x] Permettre la rotation par pas de 90° (R).
- [x] Trancher la règle de placement sur terrain irrégulier : **grille de 1 m + fondation automatique** ; un extracteur s'aligne sur le centre du gisement.
- [x] Pose gratuite en V0 (mode créatif). Démontage au clic droit.

### Outils de test

- [x] Accélération du temps : ×1, ×10, ×100 (T ; N ticks de simulation par tick fixe).
- [x] Apparition d'items à la demande dans un inventaire ciblé (I : +10, O : vider).
- [x] Overlay debug des flux et de l'état des machines (machine visée + résumé global ; réseaux électriques en M3).

### Extracteur simple

- [x] Créer un extracteur qui s'aligne sur le gisement.
- [x] Empêcher le placement si aucun gisement compatible n'est ciblé.
- [x] Autoriser l'extraction uniquement quand les conditions de fonctionnement sont remplies (gisement + place en sortie ; énergie en M3).
- [x] Mettre les ressources extraites dans un **inventaire de sortie** borné (50).
- [x] Suspendre la production lorsque la sortie est pleine (progression conservée).
- [x] Afficher le débit réel et l'état (`actif`, `bloqué` ; `sans énergie` en M3), voyant vert/orange sur la machine.
- [x] Créer des tests unitaires pour le calcul de production.

### Inventaire et panneau de machine (demandé en test)

- [x] Inventaire du joueur : 27 cases, piles de 100 (`src/inventory.rs`, Rust pur), ouvert avec Tab.
- [x] Panneau d'extracteur (E en le visant) : état, débits, avancement vers la prochaine unité, sortie, bouton « Tout prendre ».
- [x] Transfert sortie → inventaire **sans perte ni duplication** (ce qui ne rentre pas reste dans la machine ; testé).
- [x] Menu ouvert : souris libre, joueur immobile, **simulation qui continue**.
- [x] Types d'objets (`ItemKind`) séparés des ressources de gisement : l'extracteur produit du **minerai**.
- [x] Aide contextuelle sous le réticule (E : ouvrir, clic : détruire…).

**Validation M2**

- [ ] L'extracteur se pose via le mode construction (fantôme + rotation), sans commande debug.
- [ ] Un extracteur sur un gisement produit à un débit connu pendant plusieurs minutes (vérifiable en accéléré).
- [x] Le gisement ne s'épuise pas (test : 10 h simulées).
- [x] Le débit ne dépend ni des FPS ni de la distance à la caméra (simulation en ticks, indépendante du rendu ; test dédié).
- [x] Toutes les ressources **actuellement déclarées obligatoires** sont présentes sur une nouvelle carte (test sur 24 seeds).

---

## M3 — Simulation industrielle et réseau d'énergie ⭐

### Horloge et état

- [x] Définir un pas de simulation fixe (20 Hz, à mesurer plus tard ; indépendant du rendu).
- [x] Séparer **données de jeu persistantes** (`SimWorld`, `PlacedBuildings`, inventaire) et **entités/meshes de présentation**.
- [x] Définir `MachineState` : actif, en attente de minerai, sortie pleine, sans énergie, éteint.
- [x] Mettre les calculs de production et consommation dans des systèmes testables sans fenêtre 3D (`simulation/`, 97 tests).
- [x] Accumuler les fractions de production en **entiers** (ticks × millièmes d'alimentation).
- [x] Éviter que la simulation dépende de l'ordre d'itération non déterministe des entités (`BTreeMap` par `BuildingId`).
- [x] Prévoir explicitement la politique de simulation quand le jeu est en pause (pause = figé ; menu ouvert = continue).

### Électricité : modèle initial volontairement simple

- [x] Définir des unités cohérentes de puissance, énergie et durée : watts (`_w`), joules (`_j`), ticks.
- [x] Créer un générateur produisant une quantité fixe de puissance (100 kW, sans carburant en V0).
- [x] Créer un consommateur avec demande énergétique (extracteur 30 kW, four 50 kW).
- [ ] **↗** Créer un stockage/buffer facultatif minimal si nécessaire à la démonstration (pas nécessaire pour l'instant).
- [x] Créer des connexions visibles simples : **poteaux à rayon de connexion automatique** (10 m) et câbles droits.
- [x] Construire les réseaux à partir des connexions effectives (graphe, union-find, `simulation/energy.rs`).
- [x] Recalculer le réseau uniquement après pose/retrait (jamais à chaque tick).
- [x] Définir la règle si la puissance disponible est insuffisante : **satisfaction proportionnelle** (millièmes, arrondi inférieur), documentée.
- [x] Bloquer la consommation si la machine est éteinte, non reliée, bloquée ou sans minerai.
- [x] Afficher puissance produite, demandée et disponible pour le réseau (panneau de machine + overlay F3).
- [x] Écrire des tests : réseau isolé, câble coupé, surcharge, reconnexion, boucle, générateur éteint.

### Première machine consommatrice

- [x] Créer une machine de transformation : le **four**.
- [x] Définir au moins **une recette** entrée → sortie avec durée et coût énergétique (minerai → plaque, 2 s, 100 kJ ; fer et cuivre).
- [x] Afficher le progrès et la raison d'un éventuel arrêt (barre d'avancement, état, gueule rougeoyante, voyant).
- [x] Empêcher l'usage de ressources inexistantes, les productions négatives ou les doubles consommations (consommation unique à la fin de la recette, testée).

**Validation M3**

- [x] Générateur → câble → machine : la recette progresse (testé ; à valider en jeu).
- [x] Retirer le câble ou couper le générateur : la machine s'arrête correctement (testé).
- [x] Remettre l'énergie : la machine reprend selon la règle définie (progression conservée, testé).
- [x] Le résultat est identique sur des durées simulées équivalentes, quels que soient les FPS (testé).

---

## M4 — Machines modulaires façon Oritech ⭐

### Concepts

- [x] Définir un **noyau** de machine : extracteur et four (identité, inventaires, recette, énergie, états, `MachineStats`).
- [x] Définir les **modules comme des objets du monde**, attachés physiquement à un noyau (visibles sur le toit).
- [x] Définir des points d'attache (`Socket`) : 2 emplacements sur le toit de chaque noyau.
- [x] Définir une règle lisible de **rattachement** : noyau compatible, un module par emplacement, emplacement libre le plus proche du point visé.
- [x] Définir des limites explicites : 2 modules par noyau, jamais de module sur un module (aucun cycle), consommation ≥ 20 %.
- [x] Choisir comment représenter et stocker les modules : `SimWorld.modules` (id stable, hôte, emplacement), stats recalculées à chaque changement.
- [x] Séparer **effet de simulation** (`simulation/modules.rs`) et **modèle visuel** (`building/mod.rs`).
- [x] Ne pas coder des exceptions spécifiques à chaque type de machine : bonus additifs génériques appliqués à toute machine.

### Deux modules de démonstration

- [x] Ajouter un module **vitesse** : +50 % de vitesse, +60 % de consommation.
- [x] Ajouter un module **efficacité** : −30 % de consommation (à équilibrer plus tard).
- [x] Recalculer les statistiques à l'ajout ou retrait d'un module (immédiat, testé).
- [x] Autoriser le retrait sans perdre ni dupliquer le contenu de la machine (testé).
- [x] Afficher clairement les stats résultantes dans l'interface (panneau de machine : modules, vitesse, consommation, kW ; débit nominal / théorique / réel).
- [x] Ajouter effets visuels discrets sur les modules actifs/inactifs (pastille lumineuse quand la machine travaille).
- [x] Prévoir le traitement des modules incompatibles (pose refusée avec motif) ou détachés (retirés avec leur machine, aucun module fantôme).

### Placement avancé (le placement de base est en M2)

- [x] Ajouter snap selon le type d'objet : grille de 1 m, centre du gisement (extracteur), emplacements de toit (modules), ports (convoyeurs).
- [x] Vérifier les collisions et empêcher les chevauchements non autorisés (bâtiments, rochers, joueur, gisements).
- [x] Ajouter suppression/démontage propre des objets construits (bâtiments, convoyeurs, modules ; cascade sans fantôme).
- [x] Prévoir des fondations simples pour construire au-dessus du sol irrégulier (fondation automatique, 2,5 m max).
- [x] Confirmer la gratuité de la construction en V0 (mode créatif) ; coûts éventuels avec l'arbre techno en V0.1.

**Validation M4**

- [x] Le module est visible **dans le monde**, pas seulement une case dans l'UI.
- [x] Le même noyau accepte au moins deux types de modules (vitesse et efficacité, combinables).
- [x] Les stats et la production changent immédiatement et correctement (testé : 30 → 45 /min, 30 → 48 kW).
- [x] Placement invalide refusé ; retrait propre ; aucun module fantôme (testé).

---

## M5 — Logistique minimale ⭐

> **Avancé pendant M3 à la demande** : convoyeurs point à point, coffre, glisser-déposer et menu de construction.
> Hub et objectifs ajoutés avec M6.

> **Objectif :** une chaîne fonctionnelle. Les convoyeurs animés complets sont facultatifs au début : un transfert par ports connectés suffit pour valider la simulation.

### Inventaires et ports

- [x] Définir `ItemStack`, inventaire à capacité bornée et type d'item (`src/inventory.rs`).
- [x] Définir ports d'entrée/sortie et compatibilités (sortie jaune, entrée bleue ; extracteur → sortie, four et coffre → entrée + sortie).
- [x] Ajouter transfert d'items entre deux bâtiments connectés : **convoyeurs point à point** (60/min, 1,5 m/s, 40 m max).
- [x] Définir les cas `entrée pleine`, `sortie pleine`, `mauvais item`, `pas de connexion` (le tapis s'accumule puis s'arrête de prendre).
- [x] Garantir la **conservation des objets** (ni duplication ni perte accidentelle ; test de chaîne complète).
- [x] Écrire des tests de transfert, notamment sur les limites de capacité.

### Visualisation / ergonomie

- [x] Ajouter un élément visuel de lien logistique (tapis droit entre les ports).
- [x] Afficher les flux et le contenu des buffers dans l'overlay debug (convoyeur visé, objets en route).
- [x] Ajouter retour visuel quand un item est transféré (cubes colorés qui défilent sur le tapis).
- [x] Éviter de créer une entité physique **par item** (pool de cubes recyclés, purement visuels).
- [x] **Glisser-déposer** dans les inventaires (déplacer, compléter, échanger ; inventaire ↔ coffre ↔ four).
- [x] **Menu de construction** (B) à la place des touches 1–4 ; Échap quitte la construction.

### Chaîne de démonstration

- [x] Construire **gisement → extracteur → four → coffre** (test headless de 5 min simulées).
- [x] Alimenter extracteur et four depuis un générateur.
- [x] Permettre d'accélérer le four par un module de vitesse.
- [x] Ajouter un **hub** qui compte les items livrés, avec 5 objectifs successifs (suivi en haut à droite, notification).
- [x] Tester la saturation du stockage et la reprise après libération de place (tests de convoyeur bloqué).

**Validation M5**

- [ ] La chaîne produit automatiquement sans action du joueur après installation.
- [ ] Les objets sont conservés et les saturations gérées correctement.
- [ ] Le système fonctionne aussi si le joueur s'éloigne.

---

## M6 — Simulation hors champ ⭐

> Les sauvegardes sont reportées en **V0.1** (voir la section dédiée après M7).

### Simulation indépendante du rendu

- [x] Distinguer chargement **visuel** et existence **logique** des usines : `SimWorld` contient tout, l'affichage des bâtiments est chargé à 150 m et déchargé à 170 m.
- [x] Garder en mémoire les machines actives hors champ (simulation complète, pas d'agrégat nécessaire à cette échelle).
- [x] Valider la production pendant que les entités visuelles sont déchargées (la simulation ignore l'affichage ; overlay « Affichage : x/y chargés »).
- [x] Vérifier qu'une reconnexion visuelle n'entraîne pas de duplication d'objets ni de reset du progrès (l'affichage est recréé à partir de l'état de simulation, jamais l'inverse).
- [x] Ajouter un test headless : N ticks simulés sans rendu → production attendue (chaîne extracteur → four → coffre, hub).

**Validation M6**

- [x] Un test de simulation hors champ donne la production attendue.
- [ ] S'éloigner puis revenir ne duplique ni ne réinitialise rien (garanti par construction ; à confirmer en jeu).

---

## M7 — Vertical slice jouable et stabilisation ⭐

### UX minimale

- [x] Lancement direct dans un monde, seed en argument (`cargo dev -- --seed 42`), aléatoire par défaut ; `--demo` pour la scène standard.
- [x] Indication des contrôles et raccourcis à l'écran (H : aide complète, rappel permanent en bas à gauche, bandeaux contextuels).
- [x] Barre de construction simple → **menu de construction** (B) : extracteur, générateur, four, coffre, poteau, hub, convoyeur, modules.
- [x] Survol/sélection d'une machine : aide sous le réticule + panneau (E) avec énergie, inventaires, progression, modules.
- [x] Couleurs / icônes cohérentes pour `actif`, `hors tension`, `bloqué`, `invalide` (voyants vert/orange/rouge/gris, fantôme vert/rouge, ports bleu/jaune).
- [x] Messages d'erreur utiles plutôt que des plantages silencieux (motifs de refus dans les bandeaux et panneaux).
- [x] Outils de test accessibles par raccourci : T (temps), G (minerais), I/O (machine visée), F3 (overlay), listés dans l'aide (H).

### Qualité

- [x] Ajouter tests d'intégration du cycle complet de production (extracteur → four → coffre ; scène de démo → hub).
- [x] Créer une scène test standard avec plusieurs machines (`--demo`, `src/scene.rs`, testée sur 16 seeds).
- [x] Ajouter une seed de référence et tests de déterminisme (relief, rochers, gisements).
- [ ] Profiler séparément CPU simulation, génération, rendu, collisions et mémoire (simulation, génération, maillage et FPS mesurés ; collisions et mémoire non instrumentées).
- [x] Mesurer les performances avant d'ajouter LOD avancé, threading complexe ou optimisations ECS (`docs/known-issues.md` : rien n'est nécessaire).
- [x] Tester le comportement à faible FPS et pendant les pauses (ticks fixes, test d'indépendance au regroupement des ticks).
- [x] Tester monde vide, monde à nombreux câbles et machines (test de charge 1 000 bâtiments : 38 µs/tick ; réseau en grille spatiale).
- [x] Documenter les bugs connus et les contournements (`docs/known-issues.md`).
- [x] Préparer une build de démonstration et une courte procédure de reproduction (README, section « Démo »).

### Critères d'acceptation V0

- [x] **Créer une partie** avec seed et voir un terrain lisse.
- [x] **Marcher / regarder / construire** sans passer par un outil externe.
- [x] **Trouver un gisement infini** et poser un extracteur.
- [x] **Produire de l'électricité** et alimenter des bâtiments.
- [x] **Transformer une ressource** puis la stocker automatiquement.
- [x] **Brancher/débrancher un module physique** avec effet mesurable.
- [ ] **S'éloigner** et revenir sans perte de simulation.
- [ ] **Jouer plusieurs minutes sans crash** sur une machine de test.
- [ ] **Avoir une boucle plaisante**, même avec seulement 2–3 bâtiments.

---

## V0.1 — Arbre techno primitif + sauvegardes ↗

> Ne pas commencer avant la validation de la V0.

### Arbre de technologies primitif

- [ ] Ajouter un champ `unlocked_by` aux définitions de bâtiments, recettes et modules.
- [ ] Débloquer les paliers en livrant des items au hub (réutilise le compteur de M5).
- [ ] Afficher les paliers et leur état dans une UI simple.
- [ ] Décider si la construction devient payante avec la progression.

### Sauvegarde

- [ ] Définir un `SaveGame` **versionné**.
- [ ] Sauvegarder seed, version/config de worldgen, paramètres de monde.
- [ ] Sauvegarder position/orientation du joueur et état pertinent de la partie.
- [ ] Sauvegarder bâtiments, transforms et identifiants persistants.
- [ ] Sauvegarder connexions d'énergie et de logistique via identifiants persistants.
- [ ] Sauvegarder inventaires, buffers, énergie et progrès des recettes.
- [ ] Sauvegarder modules attachés et leurs configurations.
- [ ] Sauvegarder la liste des rochers destructibles déjà détruits.
- [ ] Sauvegarder la progression de l'arbre techno et le compteur du hub.
- [ ] Restaurer correctement les réseaux après chargement.
- [ ] Écrire atomiquement (fichier temporaire + remplacement) pour réduire le risque de corruption.
- [ ] Prévoir message d'erreur et solution propre pour sauvegarde absente/incompatible/corrompue.
- [ ] Ajouter une option de sauvegarde manuelle ; autosave ensuite.
- [ ] Définir ce que fait le jeu pendant qu'il est **fermé** (recommandation : **aucune production hors ligne**).
- [ ] Ajouter tests de sauvegarde → rechargement → poursuite de simulation.
- [ ] Ajouter un écran titre minimal : nouvelle partie, charger, quitter.

**Validation V0.1**

- [ ] Après fermeture et relance, la même usine et ses connexions sont retrouvées.
- [ ] Une sauvegarde interrompue ou invalide ne détruit pas silencieusement l'unique copie valide.
- [ ] Un palier se débloque en livrant au hub les items demandés.

---

## 3. Checklist transversale (à revisiter à chaque jalon)

### Architecture / code

- [ ] Garder le domaine de simulation aussi indépendant que possible du rendu.
- [ ] Limiter les dépendances croisées entre `world`, `building`, `machines` et `simulation`.
- [ ] Préférer des composants et systèmes explicites plutôt qu'une hiérarchie d'objets opaque.
- [ ] Centraliser les définitions de bâtiments, recettes et modules dans des données configurables si cela apporte de la valeur.
- [ ] Documenter les invariants : conservation d'items, capacités, puissance, validité des connexions.
- [ ] Rendre les opérations de placement/suppression sûres en cas d'échec partiel.
- [ ] Séparer les mécanismes génériques des règles d'équilibrage.
- [ ] Réduire les allocations répétées dans les boucles de simulation.

### Tests

- [ ] Tester pureté × niveau d'extracteur × temps simulé.
- [ ] Tester les réseaux séparés, reconnectés et surchargés.
- [ ] Tester la conservation des ressources dans les transferts.
- [ ] Tester les modules ajoutés, supprimés et incompatibles.
- [ ] **↗** Tester les sauvegardes chargées sur une nouvelle session (V0.1).
- [ ] Tester plusieurs seeds (dont cas limites) et leurs garanties de ressources.
- [ ] Tester qu'une simulation identique à des FPS différents produit le même résultat.
- [ ] Automatiser au moins un parcours de simulation **headless**, si faisable avec l'architecture choisie.

### Performance / robustesse

- [ ] Définir une cible indicative : **60 FPS en scène simple**, à ajuster selon machine et résolution.
- [ ] Définir une limite de temps de génération acceptable pour le prototype et la mesurer.
- [ ] Instrumenter le nombre de chunks visibles et le nombre de bâtiments simulés.
- [ ] Instrumenter le coût CPU d'un tick industriel et la mémoire associée.
- [ ] Éviter de recalculer tous les réseaux à chaque frame.
- [ ] Éviter un collider complexe pour chaque détail décoratif.
- [ ] Éviter de simuler graphiquement des milliers d'items individuels.
- [ ] Prévoir tests de charge progressifs : 10, 100, puis 1 000 bâtiments **selon besoin**.

### Documentation / suivi

- [ ] Garder ce `TODO.md` à jour lors de chaque session.
- [ ] Créer `CHANGELOG.md` dès qu'il existe une première version testable.
- [ ] Consigner les décisions techniques importantes et leur justification.
- [ ] Ajouter captures ou vidéos courtes des jalons pour vérifier la progression.
- [ ] Reprioriser après chaque démonstration jouable.

---

## 4. Risques majeurs et parades

| Risque | Parade concrète |
|---|---|
| **⚠️** Construire un moteur complet avant un jeu | Priorité à une usine minimaliste jouable |
| **⚠️** Monde immense trop tôt | Commencer à 512 m–1 km, agrandir après profilage |
| **⚠️** Complexité des simulations hors champ | Simulation logique séparée du rendu dès M3 |
| **⚠️** Non-déterminisme de la génération | Seed + configuration/version + tests de référence |
| **⚠️** Modules difficiles à gérer | Points d'attache simples et règles explicites en V0 |
| **⚠️** Dupe/perte dans les convoyeurs | Invariants + transferts atomiques + tests |
| **⚠️** Sauvegardes incompatibles (V0.1) | IDs stables dès la V0, formats versionnés dès la V0.1 |
| **⚠️** Temps de compilation Bevy | Profil dev optimisé pour les dépendances + `dynamic_linking` dès M0 |
| **⚠️** Boucle sans but | Hub qui compte les livraisons dès M5, base de l'arbre techno V0.1 |
| **⚠️** Trop de contenus avant la base | Un gisement, un extracteur, un transformateur, deux modules |
| **⚠️** Refonte lourde après changement de librairie | Versions validées et dépendances encapsulées |

---

## 5. Backlog post-V0 — **ne pas commencer maintenant** ↗

### Monde

- [ ] Ajouter davantage de biomes, végétation, météo et variations de relief.
- [ ] Générer des grottes / falaises en surplomb par maillages dédiés si besoin.
- [ ] Agrandir la carte et enrichir les points d'intérêt.
- [ ] Ajouter ressources spécifiques à certaines régions, avec quotas garantis.
- [ ] Améliorer LOD, streaming et occlusion si les mesures le justifient.

### Industrie

- [ ] Multiblocs, modules de nouveaux types et chaînes de modules.
- [ ] Plusieurs vitesses de traitement, parallélisme, surcadencement.
- [ ] Nouvelles machines et recettes complexes.
- [ ] Réseaux électriques avec tensions, transformateurs, batteries et pertes éventuelles.
- [ ] Tuyaux, liquides, réservoirs et stations de pompage.
- [ ] Convoyeurs courbés, splitters, mergers, filtres et tri automatique.
- [ ] Contrôle logique / capteurs / conditions d'activation.

### Logistique avancée

- [ ] Trains, rails, embranchements, gares, planification et signaux.
- [ ] Drones, stations, charge et itinéraires.
- [ ] Carte stratégique des flux et des débits.

### Jeu / confort

- [ ] Arbre technologique complet (la version primitive arrive en V0.1).
- [ ] Modes de jeu, coûts de construction et équilibrage.
- [ ] Plans de construction (blueprints), copier-coller, annuler/refaire.
- [ ] UI avancée de statistiques, overlays de réseaux et carte interactive.
- [ ] Son, animations, effets visuels et direction artistique aboutie.
- [ ] Textures de sol et de rochers (UV du terrain déjà prêtes, en mètres / 16).
- [ ] Support manette, accessibilité et options graphiques.
- [ ] Multijoueur : seulement après stabilisation des IDs, simulation et sauvegardes.

---

## 6. Première session de développement — démarrage immédiat

> **Ne faire que ceci pour commencer.** Le but est de passer de « l'idée » à « une fenêtre 3D dans laquelle je peux bouger ».

- [x] Initialiser Git et Cargo dans `Project-ascension`.
- [x] Ajouter Bevy dans une version vérifiée comme stable/compatible, et l'épingler.
- [x] Configurer le profil dev pour réduire les temps de compilation.
- [x] Ouvrir une fenêtre 3D et afficher un sol plat.
- [x] Ajouter un cube, une lumière et une caméra.
- [x] Implémenter une caméra libre : WASD/ZQSD selon le clavier, souris pour regarder autour.
- [x] Afficher la position du joueur en debug.
- [x] Lancer `cargo fmt --check`, `cargo clippy`, `cargo test`.
- [ ] Faire un premier commit : `feat: initial 3d sandbox prototype`.
- [ ] Faire une capture du prototype et noter le prochain obstacle concret.

**Prochaine session ensuite :** remplacer le sol plat par une heightmap basée sur une seed, sans encore coder l'intégralité de l'industrie.

---

## 7. Journal de bord

| Date | Jalon | Ce qui fonctionne | Bloquant / prochaine action |
|---|---|---|---|
| 2026-11-15 | Cadrage | TODO revu : nom Project Ascension, V0 = test perso, saves + techno en V0.1, placement en M2 | Initialiser le projet (section 6) |
| 2026-11-15 | M0 | Bevy 0.20.1, fenêtre 3D, sol + cube + piliers, caméra libre, pause, overlay F3, tick fixe 20 Hz, 8 tests, fmt/clippy OK | Valider la caméra en jeu, premier commit, pousser sur GitHub pour lancer la CI |
| 2026-11-15 | M1 (relief) | Heightmap 512 m déterministe, 64 chunks sans fissures, mer, couleurs sol, mode marche + gravité, seed en argument, 34 tests | Juger le relief et la marche en jeu ; ensuite affichage par distance et rochers destructibles |
| 2026-11-15 | M1 (rochers) | Relief et marche validés en jeu. 87 rochers (seed 42), collisions, visée + destruction, vitesse dans l'overlay, 42 tests | Valider M1 en jeu ; reste l'affichage par distance (à mesurer), puis M2 |
| 2026-11-15 | M2 | Gisements infinis (8 fer + 5 cuivre, fer de départ garanti), mode construction (fantôme, rotation, fondation auto), extracteurs à débit entier, sortie bornée, accélération du temps, 69 tests | Valider en jeu la pose et le débit sur quelques minutes accélérées ; puis M3 (énergie) |
| 2026-11-15 | M2 (inventaire) | Inventaire joueur (Tab), panneau d'extracteur (E) avec « Tout prendre », état « menu » où la simulation continue, 73 tests | Valider les menus en jeu ; puis M3 (énergie) |
| 2026-11-15 | M3 | Générateur, poteaux (10 m) et câbles, réseaux en graphe, satisfaction proportionnelle, four (minerai → plaque), barre 1–4, panneaux par machine avec marche/arrêt, 97 tests | Valider la chaîne générateur → poteau → extracteur / four en jeu ; puis M4 (modules) |
| 2026-11-15 | M3 + M5 (partiel) | Chaîne M3 validée en jeu. Menu de construction (B), coffre, convoyeurs point à point avec objets visibles, glisser-déposer, 111 tests | Valider convoyeurs et glisser-déposer en jeu ; puis M4 (modules) et le hub |
| 2026-11-15 | M4 | Convoyeurs et ports validés en jeu. Modules vitesse / efficacité sur le toit (2 emplacements), stats additives, retrait depuis le panneau ou au clic droit, 123 tests | Valider les modules en jeu ; puis le hub (M5) et M6 (hors champ) |
| 2026-11-15 | M5 + M6 | Modules validés en jeu. Hub de livraison + 5 objectifs (suivi, notification), affichage des bâtiments déchargé au-delà de 170 m sans toucher à la simulation, 128 tests | S'éloigner puis revenir en jeu ; puis M7 (UX minimale, stabilisation) |
| 2026-11-15 | M7 | Hub validé en jeu. Four : entrée et sortie opposées. Aide (H), scène de démo (`--demo`), outil G, coût du tick dans l'overlay, test de charge (1 000 bâtiments : 38 µs/tick), bugs connus, 133 tests | Valider : s'éloigner/revenir, plusieurs minutes sans crash, boucle plaisante ; puis V0.1 (sauvegardes + arbre techno) |

---

## 8. Questions ouvertes — à trancher seulement au bon moment

- [ ] Vue FPS ou caméra libre ? → *Proposé :* caméra libre d'abord, marche FPS ensuite avec bascule.
- [ ] Mode créatif ou coûts symboliques ? → *Proposé :* créatif en V0, coûts éventuels avec l'arbre techno V0.1.
- [ ] Valeur du tick de simulation et politique de pause ?
- [x] Règle de partage électrique en cas de surcharge ? → **Retenu :** satisfaction proportionnelle (M3).
- [ ] Portée et orientation des points d'attache des modules ?
- [ ] Placement sur fondations : grille, snap libre, ou mode hybride ? → **À trancher en M2.** *Proposé :* grille de 1 m + rotation 90° + fondation automatique.
- [ ] Nombre de types de ressources obligatoires lors de la première vraie génération ?
- [ ] Taille de carte définitive après premiers tests de déplacement et de performance ?
- [ ] Production hors ligne : sans objet en V0 (pas de sauvegarde), à trancher en V0.1.

---

**Principe directeur :** *on ne cherche pas à construire tout Satisfactory ou tout GregTech ; on cherche d'abord à construire une minuscule usine qui fonctionne vraiment, sur un terrain qui donne envie d'explorer.*
