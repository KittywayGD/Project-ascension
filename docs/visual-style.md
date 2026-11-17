# Style visuel — passe de lisibilité (M7)

> But : que le jeu se **comprenne au premier coup d'œil**, pas qu'il soit beau.
> Le vrai travail artistique (modèles, textures, shaders, sons) est reporté après la V0.1 :
> on ne modélise pas une machine dont la forme ou le rôle peut encore changer.

## Couleurs par famille

| Famille | Bâtiments | Couleur | Signe distinctif |
|---|---|---|---|
| Extraction | Extracteur | Orange | Colonne de forage à l'avant |
| Énergie | Générateur, poteau | Violet | Tambour argenté couché ; isolateur violet en haut du poteau |
| Transformation | Four | Brique | Gueule qui rougeoie quand il fond, cheminée |
| Logistique | Coffre, convoyeur | Bois / acier sombre | Tapis avec rails et flèches jaunes (sens de déplacement) |
| Livraison | Hub | Blanc et or | Colonne, anneau doré et balise |

Ces couleurs ne doivent pas servir à autre chose : un futur bâtiment d'une famille reprend sa couleur.

## Signaux (réservés, à ne pas réutiliser comme décor)

| Signal | Sens |
|---|---|
| Bloc **bleu** | Entrée d'un bâtiment |
| Bloc **jaune** | Sortie d'un bâtiment |
| Voyant **vert** | Actif |
| Voyant **orange** | En attente d'entrée ou sortie pleine |
| Voyant **rouge clignotant** | Sans énergie |
| Voyant **gris** | Éteint |
| Fantôme **vert / rouge** | Pose possible / impossible |
| Modules **cyan / vert** | Vitesse / efficacité ; pastille allumée quand la machine travaille |

## Rendu

- **Éclairage physique** (`src/world/sky.rs`) : soleil à ~130 000 lux, 38° au-dessus de l'horizon, caméra exposée à EV100 13, tonemapping TonyMcMapface.
- **Ciel** : atmosphère de Bevy (diffusion de Rayleigh / Mie), qui éclaire aussi la scène (`AtmosphereEnvironmentMapLight`).
- **Émissions** : toujours multipliées par `GLOW` (×12), sinon elles disparaissent sous l'exposition « plein soleil ». Un bloom discret les fait briller.
- **Brume** légère (visibilité 2,6 km) pour la profondeur ; la mer fait 30 km de côté pour ne jamais découper l'horizon.
- **Terrain** : herbe jusqu'à ~25° de pente (taches sèches ou grasses selon un bruit lent), terre de 25 à 37°, roche nette au-delà de ~40°. Purement visuel : ne change ni le relief ni son empreinte.
- **Police** : Fira Sans (OFL, `assets/fonts/`), embarquée dans le binaire. La police par défaut de Bevy ne contient que l'ASCII et n'affiche pas les accents.

## Juger un changement visuel

```bash
cargo dev -- --seed 42 --demo --screenshot screenshots/vue.png
```

Le jeu prend trois vues fixes (`vue-usine.png`, `vue-detail.png`, `vue-paysage.png`) avec le nombre de FPS dans les logs, puis quitte. En jeu, **F12** enregistre une capture dans `screenshots/`.
