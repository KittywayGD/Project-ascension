# Simulation

> État : **M6** — horloge à pas fixe, extracteurs, four, coffre, hub, modules, réseau électrique, convoyeurs et simulation hors champ.

## Démarrer une usine

1. Pose un **générateur** (touche 2 en mode construction) sur un terrain libre.
2. Pose un **poteau** (4) à moins de 10 m : un câble apparaît.
3. Pose un **extracteur** (1) sur un gisement à moins de 10 m d'un poteau, en ajoutant des poteaux pour faire la distance.
4. Pose un **four** (3) relié au réseau, ouvre-le (E), dépose du minerai : il en fait des plaques.
5. Pour automatiser : pose un **coffre**, puis des **convoyeurs** de la sortie de l'extracteur (bloc jaune) à l'entrée du four (bloc bleu), et de la sortie du four à l'entrée du coffre.

Les bâtiments se choisissent dans le **menu de construction** (B).

## Hub de livraison et objectifs (`src/simulation/hub.rs`)

- Le **hub** accepte tout objet, par convoyeur (entrée à l'arrière) ou à la main (« Livrer tout l'inventaire » dans son panneau). L'objet est **consommé** : il quitte le jeu et est compté.
- Les livraisons de tous les hubs s'additionnent et font avancer **5 objectifs successifs** : 50 plaques de fer, 50 plaques de cuivre, 300 plaques de fer, 200 plaques de cuivre, 1 000 plaques de fer. Ce qui a été livré d'avance compte dès que l'objectif devient le courant.
- L'objectif en cours est affiché en haut à droite, et une notification apparaît quand il est atteint. Il préfigure l'arbre technologique de la V0.1.
- Le hub n'a pas besoin d'énergie. Son voyant est vert quand un convoyeur lui apporte quelque chose.

## Simulation hors champ (M6)

- `SimWorld` contient **tout** l'état de jeu ; l'affichage en est une projection.
- L'affichage d'un bâtiment est **chargé** quand le joueur passe à moins de 150 m, et **déchargé** au-delà de 170 m (l'écart évite les clignotements à la limite). Ses modules, ses convoyeurs et ses câbles suivent.
- La simulation ne sait pas où est le joueur : une usine déchargée produit exactement comme si on la regardait. Au retour, l'affichage est recréé depuis l'état de simulation : rien n'est dupliqué ni remis à zéro.
- Le terrain (512 m) reste entièrement affiché : rien ne justifie encore de le découper par distance.
- Jeu fermé : aucune production hors ligne. Il n'y a pas encore de sauvegarde (V0.1).

## Modules (`src/simulation/modules.rs`)

- **Noyau** : l'extracteur et le four ont chacun **2 emplacements** sur le toit. Le générateur, le poteau et le coffre n'en ont pas.
- **Pose** : choisis un module dans le menu de construction, puis vise une machine. Le fantôme se place sur l'emplacement libre le plus proche du point visé ; clic gauche pour le fixer.
- **Retrait** : clic droit sur le module en mode construction, ou bouton « Retirer » dans le panneau de la machine (E). Le contenu de la machine n'est pas touché. Démonter la machine retire ses modules.
- **Effets** (bonus **additifs**) :

  | Module | Vitesse | Consommation |
  |---|---|---|
  | Vitesse | +50 % | +60 % |
  | Efficacité | — | −30 % |

  Exemples : 2 modules de vitesse donnent ×2 en vitesse et 220 % de consommation ; vitesse + efficacité donnent ×1,5 en vitesse et 130 % de consommation. La vitesse ne descend jamais sous 100 %, ni la consommation sous 20 %.
- **Calcul** : la vitesse `v` (%) multiplie l'avancement à chaque tick (`r × p × v` pour l'extracteur, `p × v` pour le four), toujours en entiers. La puissance demandée vaut `nominale × consommation / 100`. Les stats sont recalculées immédiatement à chaque pose ou retrait.
- Un extracteur normal avec un module de vitesse produit 45/min pour 48 kW. Un four à ×2 sort une plaque par seconde.
- La pastille d'un module s'allume quand sa machine travaille.

## Convoyeurs (`src/simulation/logistics.rs`)

- **Point à point** : on clique la sortie d'un bâtiment (bloc jaune), puis l'entrée d'un autre (bloc bleu). Le tapis est droit, mesure de 0,5 à 40 m, et ne doit pas passer sous le terrain.
- **Ports** : l'extracteur n'a qu'une sortie (à l'avant). Le four a son entrée à l'arrière et sa sortie à l'avant (gueule sur le côté gauche). Le coffre a son entrée à l'arrière et sa sortie à l'avant. Un seul convoyeur par port.
- **Vitesse** : 1,5 m/s, avec au moins 1 s d'écart entre deux objets, soit **60 objets/min au maximum**. Un tapis de `L` m contient au plus `⌊L / 1,5⌋ + 1` objets.
- **Calcul** : chaque objet a un âge entier en ticks ; il arrive quand son âge atteint la durée du trajet. À chaque tick, dans l'ordre des identifiants : les objets avancent, celui du bout est livré si l'entrée l'accepte, puis un objet est pris à la source s'il y a la place.
- **Conservation** : un objet quitte la source au moment exact où il monte sur le tapis, et n'en descend qu'au moment exact où la destination l'accepte. Si l'entrée est pleine ou refuse l'objet (par exemple une plaque dans un four), le tapis s'accumule en gardant l'écart, puis cesse de prendre. Rien n'est perdu.
- Les convoyeurs ne consomment pas d'énergie en V0. Démonter un convoyeur, ou l'un de ses bâtiments, détruit les objets en route (mode créatif).
- **Affichage** : chaque tapis a un petit stock de cubes colorés, recyclés pour montrer les objets en route. Ce ne sont pas des entités physiques par objet.

## Coffre et glisser-déposer

- Le **coffre** a 18 cases avec des piles de 100, et accepte tout objet. Il donne ses objets en commençant par sa dernière case non vide.
- Dans les menus, on **glisse une case sur une autre** : vers une case vide, la pile s'y déplace ; vers une pile du même type, elle la complète ; vers une pile d'un autre type, les deux s'échangent. Ça marche entre inventaire et coffre, et de l'inventaire vers l'entrée du four. L'entrée et les sorties des machines se vident vers une case de l'inventaire. Chaque déplacement conserve exactement les objets.

## Principes

- La simulation est un état en **Rust pur** (`SimWorld`, `src/simulation/sim_world.rs`), indépendant de Bevy et du rendu. Les entités 3D ne font qu'afficher cet état : une machine produit même quand elle n'est pas à l'écran.
- Elle avance par **ticks fixes** de 20 Hz (`TICK_RATE_HZ`, valeur provisoire), dans `FixedUpdate`, indépendamment des FPS. En pause, elle s'arrête.
- **Tout est entier** : temps en ticks, débits en unités/minute, accumulateurs entiers. Pas de flottant, donc pas de dérive ni de perte.
- Les machines sont parcourues dans l'ordre de leur `BuildingId` (`BTreeMap`) : le résultat ne dépend pas de l'ordre d'itération de l'ECS.
- Le résultat ne dépend que du **nombre de ticks**, pas de leur regroupement par frame (test `result_depends_only_on_the_number_of_ticks`).

## Gisements : réserve infinie, débit limité

- Un gisement (`src/world/deposits.rs`) n'a **pas de stock** et ne s'épuise jamais. Seul le débit de l'extracteur limite la production.
- Pour les ressources, le fer est **obligatoire** : le contrôle `check_quotas` le vérifie après génération. Le cuivre est présent mais facultatif pour l'instant.
- Chaque carte garantit un **gisement de fer de départ** à 64 m au plus du point d'apparition, accessible à pied sans traverser d'eau profonde. S'il n'y en a pas, il est créé de façon déterministe (recherche en spirale autour du spawn).

## Extracteur

| Pureté | Multiplicateur | Débit |
|---|---|---|
| Impur | 50 % | 15 /min |
| Normal | 100 % | 30 /min |
| Pur | 200 % | 60 /min |

- **Débit** = `EXTRACTOR_BASE_RATE_PER_MIN` (30) × pureté.
- **Calcul** : à chaque tick, le débit `r` s'ajoute à un accumulateur. Chaque fois qu'il atteint `TICKS_PER_MINUTE` (1 200), une unité est produite. Après `n` ticks, la production vaut exactement `⌊n × r / 1200⌋`.
- **Sortie** : un inventaire borné de 50 unités. Quand il est plein, l'extracteur passe à l'état `bloqué (sortie pleine)` : la production est suspendue, la progression conservée, et elle reprend dès qu'il y a de la place.
- **Débit mesuré** : le nombre d'unités produites pendant la dernière minute simulée (extrapolé si l'extracteur a moins d'une minute).
- **Conditions de fonctionnement** : être allumé, avoir de la place en sortie, et recevoir du courant. Sans courant : `sans énergie`. À alimentation partielle `p`, la machine tourne à `p` × sa vitesse.

## Four

- **Recettes** : minerai de fer → plaque de fer, minerai de cuivre → plaque de cuivre. Chaque recette prend 1 minerai pour 1 plaque, en **2 s** à pleine puissance, et coûte 100 kJ à 50 kW.
- Le **minerai déposé** choisit la recette. L'entrée et la sortie (50 chacune) ne contiennent qu'un type à la fois : on ne mélange pas, et le cuivre n'est accepté qu'une fois la sortie de plaques de fer vidée.
- Le minerai n'est **consommé qu'à la fin** d'une fabrication, et une seule fois. Une coupure de courant ou une sortie pleine suspend la fabrication sans la perdre.
- États : `actif`, `en attente de minerai`, `bloqué (sortie pleine)`, `sans énergie`, `éteint`. Le four ne demande du courant que s'il peut fabriquer.

## Réseau électrique (`src/simulation/energy.rs`)

- **Unités** : puissance en **watts** (entiers, suffixe `_w`), énergie en joules (`_j`), durées en ticks.
- **Production** : un générateur allumé fournit 100 kW, sans carburant en V0.
- **Demande** : un extracteur en marche demande 30 kW, un four 50 kW. Une machine éteinte, pleine ou sans minerai ne demande rien.
- **Connexions** : un poteau relie tout bâtiment dont le centre est à **10 m** ou moins (machines et autres poteaux). Deux machines ne se relient jamais directement. Les câbles sont visibles.
- **Réseaux** : composantes connexes du graphe (union-find), recalculées **uniquement à la pose ou au retrait** d'un bâtiment.
- **Surcharge : satisfaction proportionnelle.** Chaque tick, `p = min(1000, 1000 × produit / demandé)` millièmes, arrondi à l'inférieur. Toutes les machines du réseau tournent à `p`. Exemple : 100 kW pour 110 kW demandés donne `p` = 909, et un extracteur à 30/min produit environ 27/min.
- Les tests couvrent un réseau isolé, un câble coupé, la reconnexion, la surcharge, les boucles et un générateur éteint.

## Objets et inventaire (`src/inventory.rs`)

- Les **ressources** (`ResourceKind`) décrivent les gisements ; les **objets** (`ItemKind`) sont ce qu'on transporte. Un extracteur sur du fer produit du *minerai de fer*. Les produits transformés arriveront en M3.
- L'inventaire du joueur a 27 cases, avec des piles de 100. Un ajout complète d'abord les piles existantes, puis remplit les cases vides dans l'ordre.
- **Conservation** : `transfer` retire de la machine exactement ce qu'il ajoute à l'inventaire, jamais plus que la place libre. Le reste attend dans la machine.
- **Menu ouvert** (Tab, ou E sur un extracteur) : la souris est libre et le joueur immobile, mais la simulation **continue**. Seule la pause (Échap) la fige.

## Placement (règles dans `src/building/placement.rs`)

- Un extracteur ne se pose **que sur un gisement libre** et **s'aligne sur son centre**. Hors gisement, le fantôme se cale sur une grille de 1 m.
- **Fondation automatique** : un socle va de 30 cm sous le point le plus bas de l'emprise (3 × 3 m) jusqu'à 5 cm au-dessus du point le plus haut. La machine repose sur ce plateau.
- La pose est refusée dans l'eau, hors gisement, sur un gisement déjà équipé, sur un rocher ou sur le joueur. Le motif s'affiche dans le bandeau du mode construction.
- **Rotation** par quarts de tour (R). La sortie, le bloc jaune, regarde vers le nord (-Z) à 0. Elle servira à la logistique en M5.
- En V0 (mode créatif), la pose est gratuite et le démontage détruit le contenu de la sortie.

## Outils de test

| Touche | Effet |
|---|---|
| T | Vitesse du temps ×1 → ×10 → ×100 (chaque tick fixe exécute N ticks de simulation) |
| I | +10 dans la machine visée : sortie de l'extracteur, ou minerai de fer en entrée du four |
| O | Vide la sortie de la machine visée (pour tester la reprise) |

L'overlay F3 affiche, pour la machine visée, son état, son débit réel et théorique et le contenu de sa sortie. Il affiche aussi le résumé de toutes les machines.
