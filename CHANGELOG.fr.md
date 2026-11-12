# Notes de version

Toutes les nouveautés de Plume, version après version (en anglais : [CHANGELOG.md](CHANGELOG.md)). Le site les affiche sur sa page « Notes de version ».

## 1.2.0

Quatre nouvelles applications rejoignent Plume Suite : **Plume Notepad** remplace le Bloc-notes de Windows et ajoute des pense-bêtes, **Plume Cut** monte vos vidéos, **Plume Create** retouche vos photos et crée vos visuels, et **Plume Boost** applique des réglages de performance mesurés et annulables. Plume FEx gagne les dossiers récents, les catégories et les mini-panneaux, Plume Media les dossiers épinglés et une frise, et Plume Suite peut faire des applis Plume vos applications par défaut de Windows. Win+E ouvre de nouveau Plume FEx sur le dernier Windows 11, et Plume Viewer fonctionne depuis un iPhone.

Déjà sur Plume Suite 1.1 ? Plume Suite et chaque application affichent la mise à jour : un clic l'installe. Les nouvelles applis sont ensuite à un clic dans Plume Suite.

### Nouvelles applications
- **Plume Notepad** : des onglets qui reviennent après un redémarrage, rechercher-remplacer avec expressions régulières, tous les encodages, d'énormes fichiers en un clin d'œil. Un interrupteur, et Windows l'ouvre à la place du Bloc-notes. Des **pense-bêtes** restent au-dessus de toutes les fenêtres (Ctrl+Alt+N).
- **Plume Cut** : un éditeur vidéo avec une vraie frise (pistes vidéo, texte, incrustation, son et musique), coupes, textes, transitions, couleur et vitesse, et des exports en 9:16, 16:9 ou 1:1 avec votre carte graphique. Sans filigrane.
- **Plume Create** : retouche photo et création graphique en une appli : calques, recadrage, réglages et looks, détourage en un clic, effets de texte, et des modèles de stories, posts et miniatures.
- **Plume Boost** : les réglages de performance de Windows qui changent vraiment quelque chose, chacun expliqué (gain, coût, preuves, comment le vérifier), trois profils, des chiffres en direct avant et après, et un clic pour tout annuler. Pas de faux score, pas de « nettoyeur de RAM », et il ne coupe jamais Defender, l'UAC ni Windows Update.
  - **Réglage de la carte graphique** NVIDIA, AMD et Intel Arc : décalage des fréquences et de la mémoire, tension (AMD, Arc), limite de puissance et ventilateurs, toujours dans les limites données par le pilote. Chaque changement est d'abord essayé et revient seul après 15 secondes si vous ne le gardez pas ; un test de stabilité le vérifie ; un plantage du pilote ou un redémarrage brutal remet les réglages d'avant. Profils, application à l'ouverture de session, et un moniteur toujours au premier plan.
  - **Réglages du processeur** que Windows propose par mode d'alimentation : préférence énergie/performance, mode turbo, état minimum et maximum, mise en veille des cœurs.
  - **Une copie de vos réglages avec Plume Backup** avant chaque changement, et chaque copie peut être restaurée. Installer Plume Boost installe aussi Plume Backup.

### Plume Suite
- **Définir par défaut** : Plume Media, Music, PDF, Notepad, Cut et Create peuvent devenir l'application par défaut de Windows pour leurs fichiers, depuis leur carte.
- **Les applis amènent ce dont elles ont besoin** : installer une appli qui en demande une autre installe les deux (Plume Boost et Plume Backup), et retirer une appli dont une autre se sert le dit d'abord.

### Plume FEx
- **Dossiers récents** sous Ce PC, et des **catégories** de dossiers que vous créez, nommez et colorez.
- **Réordonnez la barre latérale en glissant** : vos dossiers du haut, les catégories et les dossiers qu'elles contiennent.
- **Ctrl+clic droit sur un dossier** l'ouvre dans un petit panneau par-dessus la barre latérale, pour glisser des fichiers d'un dossier à l'autre ; Ctrl+Z annule.
- **Filtres de recherche** par date de modification et par taille, et *Trier par* dans le menu Affichage.
- **Revenir en arrière garde votre place** : le dossier d'où vous venez reste sélectionné dans son parent.
- **Win+E ouvre de nouveau Plume FEx** sur Windows 11 25H2, où Windows ne demande plus au registre quel explorateur ouvrir. Quand *Ouvrir les dossiers et Win+E avec Plume FEx* est activé, un tout petit assistant (lancé avec Windows) attrape Win+E et ouvre Plume FEx ; le menu Démarrer reste fermé.
- **Retirer Plume FEx rend toujours tout à l'Explorateur de fichiers de Windows** : double-clic sur les dossiers et les lecteurs, Win+E et l'entrée du clic droit, même si Plume FEx n'a pas pu faire le ménage lui-même.

### Plume Viewer
- **Fonctionne depuis un iPhone (Safari)** : l'écran du téléphone reste allumé pendant qu'il attend l'accord du PC, la connexion ne tombe plus pendant que vous allez au PC.
- Un téléphone qui a décroché ne laisse plus le PC « déjà connecté » : l'essai suivant prend sa place.
- Depuis un ordinateur, votre pointeur reste visible sur l'écran du PC.

### Plume Media
- **Dossiers épinglés** dans la barre latérale, avec une **frise** par dossier (tri, filtres, sous-dossiers) et un rail de dates pour remonter les années.
- **Ouvrir dans Plume Cut** envoie vos vidéos et photos directement dans un nouveau projet vidéo.

### Plume Clipboard
- **Les mots de passe, adresses IP, clés et jetons copiés sont masqués** dans l'historique, scellés sur ce PC et retirés après quelques minutes (chaque type peut être désactivé).

### Plume Convert
- **Photos en GIF** : une photo devient un GIF, plusieurs photos deviennent un GIF animé (durée par photo, boucle, taille).

### Toutes les applications
- **Un seul style** : les mêmes couleurs, cartes arrondies, boutons en pilule, interrupteurs, espacements et fenêtres de dialogue dans toutes les applications Plume, en clair et en sombre, avec la couleur de chaque application assortie à son icône.
- **« Afficher dans le dossier » ouvre Plume FEx** : quand Plume FEx ouvre vos dossiers, un téléchargement de Plume Web (et « Afficher dans le dossier » dans Convert, Media, Music, Clean, Drop, Task et Launcher) ouvre Plume FEx sur le dossier, le fichier sélectionné, au lieu de l'Explorateur de fichiers de Windows.

## 1.1.0 - 2026-11-11

Treize nouvelles applications rejoignent Plume Suite. **Plume Music** joue votre musique et YouTube Music, avec vos playlists Spotify et Apple Music. **Plume Viewer** vous permet de contrôler votre PC depuis votre téléphone. Onze outils du quotidien les accompagnent : Drop, Launcher, Passwords, Media, Task, Clean, Backup, Clipboard, PDF, Calendar et Convert.

Déjà sur Plume Suite 1.0 ? Mettez-la à jour, puis installez les nouvelles applications depuis Plume Suite en un clic.

### Plume Music
- **Tout dans un seul lecteur** : la musique de votre PC, n'importe quel titre de YouTube Music (recherche pendant la frappe, avec suggestions), SoundCloud, et vos playlists.
- **Importer une playlist en collant son lien** : Spotify (playlist, album, titre), Apple Music, SoundCloud et YouTube. Les titres Spotify et Apple sont retrouvés sur YouTube Music par titre, artiste et durée (100 titres en 25 secondes environ). Une playlist Spotify entière, au-delà des 100 titres d'un lien partagé : sélectionnez ses titres dans Spotify (Ctrl+A, Ctrl+C) et cliquez sur **Coller les titres copiés**.
- **Sans attente** : un titre démarre en 3 secondes environ et le suivant est préparé pendant l'écoute. Lecture sans blanc, fondu enchaîné, normalisation du volume, égaliseur 10 bandes avec préréglages, aléatoire, répétition, minuteur, et lecture automatique de titres similaires à la fin de la file.
- **Paroles synchronisées** qui s'allument ligne par ligne, dans un panneau ou en plein écran ; un clic sur une ligne y va.
- **« En cours » en plein écran** : pochettes animées quand Apple Music en a une, sur un fond vivant (les couleurs de la pochette, la pochette floutée, ou votre GIF animé, aussi possible derrière toute la fenêtre).
- **Téléchargements** d'un titre, d'un album ou d'une playlist, en M4A (sans perte) ou MP3 320 kbit/s, avec titre, artiste, album et pochette.
- Albums, artistes, titres likés, playlists, thèmes clair et sombre, couleur d'accent prise sur la pochette, panneau multimédia de Windows et touches multimédia, anglais et français.

### Plume Viewer
- **Scannez, c'est parti** : Plume Viewer affiche l'ID du PC, un code à usage unique et un QR code. Scannez-le avec l'appareil photo du téléphone (ou tapez l'ID et le code dans n'importe quel navigateur), la personne devant le PC accepte, et le téléphone voit l'écran et utilise la souris et le clavier. Rien à installer sur le téléphone.
- **Rapide** : seul ce qui change à l'écran est envoyé, directement du PC au téléphone, chiffré. Un réseau lent reçoit moins d'images, jamais un retard qui s'accumule.
- **Pensé pour le tactile** : un appui pour cliquer, un appui long pour le clic droit, glisser, deux doigts pour zoomer ou défiler, le clavier du téléphone pour écrire, plus la touche Windows, Échap et le plein écran. Depuis un ordinateur, la souris et le clavier marchent comme sur le PC.
- **Mot de passe permanent** pour vos propres appareils : se connecter sans que personne n'accepte. Plume Viewer reste alors près de l'horloge et peut démarrer avec Windows.
- **Sûr par conception** : le code et le mot de passe ne quittent jamais les appareils ; cinq essais faux bloquent le PC 5 minutes ; un bandeau rouge montre qui est connecté, avec un bouton Déconnecter.

### Onze applications de plus
- **Plume Drop** : envoyez fichiers, photos, textes et liens entre le PC et votre téléphone. Scannez le QR code, le navigateur du téléphone fait le reste. Rien à installer sur le téléphone, les fichiers arrivent dans `Téléchargements\Plume Drop`, et un nouveau téléphone demande votre accord une fois.
- **Plume Launcher** : appuyez sur **Alt+Espace** et tapez. Il trouve les applications, les fichiers de vos dossiers (indexés avec le moteur de Plume FEx), les pages des paramètres de Windows et les calculs rapides (unités et pourcentages compris). La fenêtre apparaît en quelques millisecondes, et les derniers choix passent en premier.
- **Plume Passwords** : un coffre chiffré pour vos identifiants, codes 2FA (avec leur compte à rebours), cartes et notes. Il a un générateur de mots de passe, un rapport sur les mots de passe faibles et réutilisés, la saisie automatique dans la bonne fenêtre, le déverrouillage avec Windows Hello, l'import CSV et des sauvegardes chiffrées. Il ne va jamais en ligne.
- **Plume Media** : une visionneuse de photos instantanée (une photo de 12 Mpx à l'écran en une demi-seconde) et un lecteur vidéo. Elle ajoute une frise par mois qui reste fluide avec des milliers de photos, la rotation sans perte des JPEG, les détails des photos, les sous-titres, les pistes audio et la reprise là où vous vous êtes arrêté.
- **Plume Task** : les applications et les processus en arrière-plan, regroupés par programme. Une carte nomme ce qui ralentit le PC, et **Fin de tâche** l'arrête. Des graphiques en direct montrent le processeur, la mémoire, le disque, le réseau et la carte graphique, et les applications au démarrage s'activent ou se désactivent. Il utilise lui-même moins de 0,1 % du processeur.
- **Plume Clean** : une carte de ce qui remplit chaque disque, les plus gros fichiers, et des suggestions sûres : fichiers temporaires, caches, anciens téléchargements, doublons et dossiers vides. Chaque suggestion dit ce que c'est et rien n'est libéré sans votre accord.
- **Plume Backup** : des sauvegardes automatiques de vos dossiers sur une clé USB, un autre disque ou un partage réseau, toutes les heures, chaque jour ou au branchement du disque. Les sauvegardes sont incrémentielles et versionnées (un fichier inchangé ne prend pas de place en plus) et reprennent après un arrêt. Rien n'est jamais effacé à la source, et un fichier retiré de la source est gardé dans un dossier daté. Vous pouvez restaurer n'importe quelle version, à sa place ou ailleurs.
- **Plume Clipboard** : **Ctrl+Maj+V** ouvre l'historique du presse-papiers (textes, liens, images, fichiers) près du curseur, avec la recherche, les épingles, des extraits appelés par un court déclencheur (`;sig`) et le collage en texte brut. Les mots de passe copiés depuis un gestionnaire ne sont jamais gardés, et l'historique peut être scellé par Windows pour votre compte.
- **Plume PDF** : lisez vos PDF avec la recherche, le sommaire, les miniatures et les pages sombres. Surlignez, ajoutez des notes, dessinez, ajoutez du texte et signez. Fusionnez, découpez, réordonnez, faites pivoter et extrayez des pages, et remplissez des formulaires. Les modifications sont enregistrées en vraies annotations PDF, visibles dans les autres lecteurs.
- **Plume Calendar** : vues mois, semaine, jour et agenda, avec glisser pour créer, déplacer et redimensionner. L'ajout rapide comprend « Déjeuner avec Max demain 12h30 » en français et en anglais. Il gère les événements récurrents, les rappels, plusieurs calendriers, l'import et l'export `.ics`, et les abonnements à des liens de calendrier (école, sport, jours fériés).
- **Plume Convert** : déposez une vidéo, un son ou une image et choisissez un préréglage : pour téléphone, pour Discord (10, 25 ou 50 Mo), fichier plus léger, audio seul, GIF, images (WebP, JPEG, PNG, ICO), couper, sans son ou pivoter. Il utilise l'encodeur de la carte graphique quand elle en a un, n'écrase jamais un fichier, et télécharge FFmpeg la première fois qu'il en a besoin.

### Plume Suite
- Les treize nouvelles applications rejoignent la liste : installez-les, ouvrez-les, mettez-les à jour et retirez-les comme les autres. La fenêtre les montre sur deux colonnes et défile quand elles ne tiennent pas toutes.

### Plume Music
- Corrigé : sur un PC sans FFmpeg, son téléchargement (nécessaire aux téléchargements en MP3) échouait, car l'archive dépassait la taille acceptée par l'application.

## 1.0.1 - 2026-11-11

Des icônes plus grandes dans Plume FEx, des dossiers qui s'ouvrent avec lui une fois installé, et une correction de l'assistant de bienvenue sur batterie.

Déjà sur Plume Suite 1.0.0 ? Plume Suite et chaque application annoncent la mise à jour : un clic l'installe.

### Plume FEx
- **Les tailles d'icônes de l'Explorateur** : très grandes, grandes, moyennes et petites icônes, en plus de Détails. Choisissez-les avec le nouveau bouton **Affichage** à côté du menu `…`, avec Ctrl+Maj+1 à 4 (Ctrl+Maj+6 pour Détails) ou avec **Ctrl + molette**. Photos et vidéos montrent leur vraie miniature à chaque taille ; la taille est gardée.
- **Installé depuis Plume Suite, Plume FEx ouvre vos dossiers** : un double-clic sur un dossier ou un lecteur, et Win+E, l'ouvrent dans Plume FEx. Plume Suite le dit, avec le moyen de l'annuler : menu `…` de Plume FEx, *Ouvrir les dossiers et Win+E avec Plume FEx*. Retirer Plume FEx rend tout à l'Explorateur de Windows.

### Plume Web
- Corrigé : *Revoir l'assistant de bienvenue* (Paramètres › À propos) restait figé sur un portable sur batterie. L'économie de batterie coupe toujours les animations de l'interface, mais plus celles de l'écran de démarrage et de l'assistant de bienvenue ; *Réduire les animations* coupe toujours tout.

### En coulisses
- Le code du navigateur est maintenant dans `apps/web`, à côté des autres applications ; tous les logos sont dans `assets/brand`.

## 1.0.0 - 2026-11-11

Plume devient **Plume Suite** : de petites applications natives au même visage, installées et mises à jour ensemble.

- **Plume Web**, le navigateur que vous connaissez.
- **Plume FEx** (nouveau), un explorateur de fichiers qui cherche dans tout le PC à la frappe.
- **Plume Mail** arrive bientôt : il est prêt et attend sa connexion Microsoft. Plume Suite l'affiche déjà.

Déjà sur Plume 0.7 ? Acceptez la barre de mise à jour comme d'habitude : elle installe Plume Suite avec Plume Web dedans, garde votre profil, et vous ajoutez FEx depuis l'application Plume Suite si vous le voulez.

### Plume Suite
- **Une application pour installer les autres** : Plume Suite présente les applications Plume avec *Installer*, *Ouvrir* et *Retirer*. Rien n'est téléchargé : l'installeur les contient déjà.
- **Une mise à jour pour tout** : une nouvelle version s'annonce dans Plume Suite et dans chaque application (un bouton dans sa barre de titre). Un clic met à jour la suite et toutes les applications installées, vérifiées avec le SHA-256 publié avec la version.
- **Une seule entrée dans Paramètres › Applications** : *Plume Suite*. La désinstaller retire toutes les applications ; vos données restent dans `%LOCALAPPDATA%\Plume`.

### Plume FEx (nouveau)
- Les dossiers s'ouvrent en quelques millisecondes, et la recherche répond à la frappe, dans le dossier ouvert ou partout, avec la syntaxe d'Everything (mots, jokers, expressions régulières, types, tailles, dates, doublons).
- Sans droits d'administrateur : l'index suit le journal des modifications NTFS. Les clés USB, cartes mémoire et lecteurs réseau sont indexés aussi, et quittent les résultats quand on les débranche.
- **Recherche dans le contenu des fichiers** (`content:`, ou le menu des filtres), un **volet d'aperçu** (Alt+P), un affichage en **miniatures** (Ctrl+Maj+2) et **annuler** (Ctrl+Z) pour renommer, nouveau dossier, déplacer et supprimer.
- Windows fait le travail sur les fichiers : le vrai menu contextuel de l'Explorateur, la Corbeille, les fenêtres de copie, le glisser-déposer.
- Archives : extraire ZIP, 7z, RAR et tar ; créer un **ZIP ou un 7z protégé par mot de passe AES-256** (un 7z cache aussi les noms de fichiers), ou un RAR quand WinRAR est installé.
- La taille des dossiers tirée de l'index, les serveurs en **SFTP** (bêta, avec la progression de chaque transfert), des onglets, sombre et clair, français et anglais.
- En option : *Ouvrir dans Plume FEx* dans le menu des dossiers, et un réglage réversible qui ouvre les dossiers et Win+E dans Plume FEx.

### Plume Web
- **Des onglets entre les fenêtres** : glissez un onglet hors de la barre pour l'ouvrir dans une nouvelle fenêtre, ou sur une autre fenêtre Plume pour l'y déplacer, sans recharger. Les autres onglets s'écartent pendant que vous réordonnez.
- **Les pages de Plume en onglets** (Paramètres › Onglets, désactivé par défaut) : Paramètres, Historique, Téléchargements et les autres pages de Plume s'ouvrent comme de vrais onglets (`plume://settings`…).
- Le panneau de téléchargement vidéo et audio s'ouvre tout de suite ; l'analyse le complète.
- La barre *Enregistrer le mot de passe* survit aux redirections qui suivent une connexion.
- Corrigé : Plume 0.7.0 et 0.7.1 pouvaient se fermer au démarrage quand le PC était sur batterie.

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
