# Notes de version

Toutes les nouveautés de Plume, version après version (en anglais : [CHANGELOG.md](CHANGELOG.md)). Le site les affiche sur sa page « Notes de version ».

## 0.7.1 - 2026-11-10

Les mises à jour fonctionnent même quand GitHub est lent, et on voit toujours ce qu'elles font.

Déjà sur Plume 0.6 ou 0.7 ? Il installe cette mise à jour tout seul : acceptez la barre, puis *Redémarrer*.

### Mises à jour
- **Vérifiées à chaque démarrage** : Plume compare sa version à la dernière publiée à chaque lancement, au lieu de réutiliser une réponse vieille d'un jour au plus : une nouvelle version apparaît tout de suite. La vérification lit vers où pointe la page « dernière version » de GitHub : une seule requête légère, sans passer par l'API de GitHub (limitée pour les visiteurs anonymes, et la première à lâcher quand GitHub est surchargé). L'API ne sert qu'au moment d'installer.
- **Plus de blocage silencieux** : les requêtes à GitHub abandonnent après environ 15 s sans connexion ou 30 s sans réponse, et sont refaites une fois après une erreur du serveur (5xx), un délai dépassé ou une connexion coupée. Un téléchargement incomplet est repéré et recommencé. La vérification SHA-256 de l'installeur ne change pas.
- **On voit ce qui se passe** : « Recherche de mises à jour… » et « Connexion à GitHub… » dans la barre de mise à jour et dans Réglages › À propos. Après 10 secondes sans progrès : « GitHub tarde à répondre… » avec un bouton *Télécharger sur GitHub*.
- **Les échecs s'affichent** : une vérification ou une installation qui échoue affiche la raison, avec *Réessayer* et *Télécharger sur GitHub*. Avant, une installation ratée faisait disparaître la barre : on avait l'impression qu'il ne se passait rien.
- Une vérification qui se termine pendant une installation ne cache plus sa progression ni le bouton *Redémarrer*.
- La météo, l'image du jour, la vérification des mots de passe divulgués et les autres fonctions en ligne profitent des mêmes délais.

## 0.7.0 - 2026-11-09

36 nouvelles fonctions, des téléchargements repensés, et Plume peut maintenant être votre navigateur par défaut. Chaque fonction se coupe, et une fonction coupée ne travaille pas en arrière-plan : aucun minuteur, aucun script dans les pages, aucun appel réseau. En mode **Ultra-léger** avec ses réglages par défaut, Plume utilise à peu près autant de mémoire que la 0.6.1.

Déjà sur Plume 0.6 ? Il installe cette mise à jour tout seul : acceptez la barre, puis *Redémarrer*.

Les valeurs par défaut sont notées *Ultra-léger / Complet*. 🌐 = contacte un service en ligne, seulement si vous l'activez.

### Réglages
- Réglages refaits : chaque option a une description d'une ligne et sa propre valeur par défaut en mode Ultra-léger et Complet. Les options que vous changez vous-même sont retenues : changer de mode ne les écrase jamais.
- Recherche dans tous les réglages, réinitialisation d'une section ou de tout, et de nouvelles sections : Performances, Onglets, Navigation, Confidentialité, Apparence, Outils.
- Votre profil 0.6 est migré automatiquement.

### Performances
- **Badge mémoire et gestionnaire de tâches** (non / oui) : la mémoire utilisée par chaque onglet. `Maj+Échap` ouvre un gestionnaire de tâches qui liste les onglets, les extensions et les processus WebView2 avec leur processeur et leur mémoire, avec *Terminer le processus* et *Décharger*.
- **Économie de batterie** (oui / oui) : dès que vous débranchez, les onglets s'endorment plus tôt et les animations de l'interface s'arrêtent ; une feuille s'affiche dans la barre d'outils. Elle peut aussi attendre un niveau de batterie.
- **Liste « jamais en veille »** (oui / oui) : les sites choisis, les onglets qui jouent du son, ceux qui ont un formulaire non envoyé et ceux en appel ne sont jamais mis en veille.
- **Préchargement intelligent** (non / oui) : survoler un lien prépare la connexion (ou toute la page) pour qu'il s'ouvre plus vite ; en pause sur batterie ou sur une connexion limitée.
- **Mode Lite par site** (disponible, rien n'est bloqué par défaut) : coupez JavaScript, les images, les polices web ou la lecture automatique des vidéos pour un site, avec un badge dans la barre d'adresse.
- **Statistiques « Plume vous a fait économiser »** (oui / oui) : mémoire libérée, traqueurs bloqués, données et temps économisés, sur la nouvelle page et dans `plume://stats`. Tout reste sur votre PC.

### Onglets
- **Espaces de travail** (non / oui) : des ensembles d'onglets séparés (Travail, Perso…) avec leur icône et leur couleur, `Ctrl+Alt+1…9`, et au choix leurs propres favoris et un conteneur.
- **Fermer les onglets inactifs** (non / non) : les onglets que vous n'avez pas ouverts depuis un moment vont dans une archive (`plume://archive`) d'où vous pouvez les rouvrir.
- **Écran partagé** (oui / oui) : deux onglets côte à côte ou l'un au-dessus de l'autre, séparateur réglable, défilement synchronisé au choix (`Ctrl+Maj+S`).
- **Recherche dans les onglets** (oui / oui) : recherche approximative dans les onglets ouverts, les onglets fermés récemment et l'archive (`Ctrl+Maj+A`).
- **Aperçu** (non / oui) : `Maj`+clic (ou appui long) ouvre un lien dans un aperçu flottant, sans quitter la page.
- **Sessions** (oui / oui) : enregistrez une fenêtre ou un groupe d'onglets comme session, puis rouvrez-la, renommez-la, exportez-la ou importez-la ; la dernière session est gardée automatiquement.
- **Notes d'onglet** (non / oui) : une note par page ou par site, chiffrée avec votre coffre de mots de passe si vous en avez un.

### Navigation
- **Palette de commandes** (oui / oui) : `Ctrl+K` pour les actions, les onglets, les favoris, l'historique et les réglages au même endroit.
- **Calculs et conversions dans la barre d'adresse** (oui / oui) : `12*7`, `10 km en miles`, `20 °C en F` ; devises 🌐 (taux du jour de frankfurter.app).
- **Rechercher dans un site avec Tab** (oui / oui) : tapez le début d'un site comme `yout`, appuyez sur `Tab`, et la barre d'adresse cherche directement sur YouTube. Fonctionne avec une vingtaine de sites populaires (Wikipédia, Amazon, GitHub, Reddit, Twitch…).
- **Gestes de la souris** (non / non) : clic droit et dessinez pour reculer, avancer, recharger, fermer ou rouvrir un onglet, avec une trace visible.
- **Boutons latéraux de la souris et balayage du pavé tactile** (oui / oui) pour reculer et avancer.

### Confidentialité
- **Conteneurs** (non / oui) : des onglets avec leurs propres cookies et connexions, pour utiliser deux comptes à la fois. Une ligne de couleur les signale ; clic droit sur un onglet pour le rouvrir dans un conteneur ou pour toujours ouvrir un site dans l'un d'eux ; un espace de travail peut utiliser un conteneur ; les extensions sont ajoutées à chaque conteneur.
- **Effacer les données à la fermeture** (non / non) : cookies, stockage des sites, cache et historique effacés quand Plume se ferme, ou quand le dernier onglet d'un site se ferme, sauf pour les sites que vous gardez.
- **Rapport de confidentialité** (oui / oui) : cliquez sur le bouclier pour voir les traqueurs bloqués sur ce site (avec leur entreprise), les autres sites contactés, les cookies, les autorisations accordées et le HTTPS.
- **Générateur et audit de mots de passe** (oui / oui) : un mot de passe solide (ou une phrase de passe) est proposé sur les formulaires d'inscription ; Mots de passe signale ceux qui sont faibles ou réutilisés.
- **Vérification des fuites de données** 🌐 (non / non) : Have I Been Pwned avec l'anonymat k, donc seuls 5 caractères d'une empreinte SHA-1 quittent votre PC ; vérification au choix quand un mot de passe est enregistré.
- **Windows Hello** (non / non) : déverrouillez vos mots de passe avec votre code PIN, votre empreinte ou votre visage, une fois par déverrouillage ou avant chaque remplissage. Le mot de passe maître fonctionne toujours aussi.
- **DNS sécurisé** 🌐 (non / non) : DNS over HTTPS avec Cloudflare, Quad9, NextDNS, Google ou votre propre serveur, appliqué au prochain démarrage.

### Apparence
- **Teinte adaptative** (non / oui) : la barre d'outils prend la couleur du site. L'intensité de l'**effet verre** se règle.
- **Fond de la nouvelle page** : dégradé (par défaut), dégradé animé (arrêté sur batterie), couleur unie, votre propre image avec flou et assombrissement, ou image du jour 🌐 (Bing, désactivée par défaut).
- **Widgets de la nouvelle page** (aucun / horloge et sites les plus visités) : horloge, météo 🌐 (Open-Meteo, seulement avec une ville que vous saisissez), liste de tâches et sites les plus visités ; ajoutez-les, retirez-les, réorganisez-les.
- **CSS et JavaScript personnalisés par site** (non / non) : menu de la page › *Personnaliser ce site*. Le CSS s'applique pendant le chargement de la page, même sur les sites avec une politique de sécurité stricte. Le JavaScript doit être autorisé à part et affiche un avertissement clair. Export et import.
- **Densité compacte** et **mode Zen** (non / non) : Zen cache les onglets et la barre d'adresse ; ils reviennent quand la souris touche le haut de la fenêtre, avec `Ctrl+Maj+Z`, ou automatiquement en plein écran.
- **Plein écran** : `F11`, et les vidéos qui passent en plein écran remplissent maintenant tout l'écran.

### Outils
- **Picture-in-picture automatique** (non / oui) : quittez un onglet qui joue une vidéo et elle continue dans une fenêtre flottante ; elle revient quand vous revenez. Des sites peuvent être exclus.
- **Contrôle des médias** (oui / oui) : un bouton apparaît dans la barre d'outils quand un onglet joue du son (lecture / pause, suivant, aller à l'onglet), une icône de haut-parleur sur les onglets les met en sourdine, et les onglets ouverts en arrière-plan peuvent démarrer en sourdine.
- **Onglets en arrière-plan** : `Ctrl`+clic ou clic du milieu sur un lien l'ouvre maintenant en arrière-plan, comme dans Chrome.
- **Mode focus et Pomodoro** (non / non) : bloque une liste de sites pendant 25 minutes avec un minuteur dans la barre d'outils et une page douce « revenez dans 12 min », des pauses entre deux, *5 minutes de plus* si vous l'autorisez, et un son à la fin.
- **Outils de développement rapides** (oui / oui) : pipette avec loupe (couleur copiée en HEX, RGB ou HSL) et règle, depuis la palette de commandes et Plume DevTools.
- **QR code de la page** (oui / oui) : créé sur votre PC, sans service en ligne, depuis une icône de la barre d'adresse, avec *Copier l'image*.
- **Enregistrer en PDF ou en Markdown** (oui / oui) : depuis le menu de la page ; le Markdown reprend le texte du mode lecture.
- **Lire plus tard** (oui / oui) : enregistre le texte et les images de l'article pour les lire hors ligne dans `plume://reading-list`, avec lu / non lu et un nettoyage automatique.

### Téléchargements
- **Enregistrer sous à chaque fois** (oui / oui) : choisissez le dossier et le nom de chaque fichier avant qu'il se télécharge ; Plume retient le dernier dossier choisi. Désactivez-le dans Réglages › Général pour enregistrer directement dans Téléchargements.
- **Bulle des téléchargements** : le bouton de la barre d'outils ouvre vos derniers téléchargements juste en dessous, avec leur progression, *Ouvrir*, *Afficher dans le dossier*, *Annuler* et *Tous les téléchargements*. Elle s'ouvre aussi un instant quand un téléchargement commence.
- **Fini les onglets vides** : un lien qui ne fait que télécharger un fichier ne laisse plus d'onglet vide derrière lui.
- **Les téléchargements continuent** quand vous fermez leur onglet ou quand Plume le met en veille (avant, ils restaient bloqués sur *en cours*).
- **Fermer Plume** pendant un téléchargement vous demande maintenant confirmation.

### Navigateur par défaut
- **Faire de Plume votre navigateur par défaut** : proposé à la fin de l'assistant de bienvenue. Windows ouvre ses paramètres sur Plume, où vous cliquez sur *Définir par défaut*.
- **Rappel** (oui / oui) : une petite barre le propose environ une fois par semaine tant que Plume n'est pas votre navigateur par défaut, avec *Ne plus demander*.
- **Réglages › Général** indique si Plume est votre navigateur par défaut, avec un bouton pour qu'il le devienne (version installée).
- **Liens des autres applications** : les liens et les fichiers `.html` ouverts depuis d'autres applications s'ouvrent dans un nouvel onglet de la fenêtre Plume déjà ouverte, au lieu de lancer un deuxième Plume.

### Corrections
- Le bouclier, le mode lecture et les autres icônes de la barre d'adresse répondent maintenant au premier clic (la barre les cachait en prenant le focus).
- Les menus, réglages et pages ouverts ne rejouent plus leur animation et ne ratent plus de clic quand un onglet se met à jour en arrière-plan.
- Ouvrir un fichier `.html` local pouvait faire planter Plume.

### Sous le capot
- Plume se construit maintenant avec une petite copie modifiée de wry (`vendor/wry`) qui ajoute les profils WebView2 (utilisés par les conteneurs) et ne plante plus sur les messages des pages locales.
- Tests unitaires pour la migration des réglages, la calculatrice et les conversions, la recherche approximative, la génération de mots de passe, l'anonymat k, les QR codes, le DNS sécurisé et les conteneurs : `cargo test --release` et `node --test tests/ui07.test.js`.

## 0.6.1 - 2026-11-06

- **Correction** : Windows Defender signalait à tort `PlumeSetup.exe` 0.6.0 comme `Trojan:Win32/Bearfoos.A!ml` (un faux positif de l'apprentissage automatique) et le supprimait. L'installeur ne vérifie plus si Plume est ouvert avec un mutex nommé, le procédé qui le déclenchait.
- `PlumeSetup.exe` et `plume.exe` affichent maintenant leur nom, leur éditeur et leur version dans Propriétés › Détails, et indiquent qu'ils s'exécutent sans droits administrateur.
- Plume 0.6.0 installe cette mise à jour tout seul : acceptez la barre, puis *Redémarrer*.

## 0.6.0 - 2026-11-06

- **Installeur** : `PlumeSetup.exe` remplace le fichier unique `Plume.exe`. Plume s'installe pour votre compte dans `%LOCALAPPDATA%\Programs\Plume` sans droits administrateur, ajoute des raccourcis dans le menu Démarrer et sur le bureau, et apparaît dans Paramètres Windows › Applications, d'où vous pouvez le désinstaller (en gardant vos données ou non).
- **Mises à jour en un clic** : quand une nouvelle version sort, une barre vous prévient. Plume télécharge l'installeur depuis GitHub, vérifie son SHA-256, l'installe en arrière-plan, et *Redémarrer* rouvre vos onglets dans la nouvelle version.
- Votre profil, vos favoris et vos mots de passe passent de la 0.5 automatiquement.

## 0.5.0 - 2026-11-06

Première version publique.

- **Navigateur ultra-léger** pour Windows 10/11, écrit en Rust sur le moteur WebView2 (Edge) du système : un seul `Plume.exe` d'environ 2 Mo.
- **Économiseur de mémoire** : les onglets en arrière-plan sont suspendus, puis déchargés après quelques minutes ; les onglets restaurés dorment jusqu'à ce que vous les ouvriez.
- **Bloqueur de pubs et de traqueurs** (environ 3 600 domaines + filtres visuels, interrupteur par site) et la protection contre le pistage d'Edge.
- **Coffre de mots de passe chiffré** (Argon2id + AES-256-GCM) avec proposition d'enregistrement et remplissage automatique ; import CSV.
- **Import** de l'historique, des favoris et de leurs icônes depuis Chrome, Edge, Brave, Opera, Opera GX, Vivaldi et Firefox.
- **Extensions Chrome** depuis le Chrome Web Store, épinglage dans la barre d'outils, popups flottantes et panneau latéral.
- **Onglets** : groupes, onglets épinglés, onglets verticaux, fenêtres InPrivate, plusieurs fenêtres, glisser pour réorganiser, déplacer un onglet dans une nouvelle fenêtre.
- **Mode lecture**, lecture à voix haute, traduction des pages, mode sombre par site.
- **Téléchargement de vidéos et d'audio** (MP4, WEBM, MKV, MP3, WAV, FLAC…) avec yt-dlp + FFmpeg, installés à la demande.
- **Plume DevTools**, panneau intégré (console, éléments, réseau, stockage, performances).
- Assistant de bienvenue (prénom, mode Ultra-léger / Complet, import), interface en français et en anglais, réglages d'accessibilité.
- Vérification des mises à jour : Plume vous prévient quand une nouvelle version est disponible sur GitHub.
