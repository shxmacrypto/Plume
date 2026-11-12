//! The tweaks Plume Boost offers, each one with what it changes, what it gains, what it costs and where the evidence
//! comes from. Only settings Windows itself has (no third-party drivers), none that weakens security (Defender, UAC,
//! memory integrity, Windows Update stay untouched), and none whose gain is folklore: see README.md for the ones left
//! out and why.

use crate::backend::{Hive, guids};
use crate::i18n::french;

/// A text in English and French.
#[derive(Clone, Copy, Debug)]
pub struct L {
    pub en: &'static str,
    pub fr: &'static str,
}

impl L {
    pub fn get(&self) -> &'static str {
        if french() { self.fr } else { self.en }
    }
}

const fn l(en: &'static str, fr: &'static str) -> L {
    L { en, fr }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Cat {
    Power,
    Games,
    Windows,
    Services,
    Fixes,
}

impl Cat {
    pub const ALL: [Cat; 5] = [Cat::Power, Cat::Games, Cat::Windows, Cat::Services, Cat::Fixes];
    pub fn title(self) -> L {
        match self {
            Cat::Power => l("Power", "Alimentation"),
            Cat::Games => l("Games", "Jeux"),
            Cat::Windows => l("Windows", "Windows"),
            Cat::Services => l("Services (advanced)", "Services (avancé)"),
            Cat::Fixes => l("Fixes for specific problems", "Correctifs pour des problèmes précis"),
        }
    }
    pub fn note(self) -> L {
        match self {
            Cat::Power => l(
                "One plan at a time. Plans change how fast the processor wakes up and how high it clocks.",
                "Un seul mode à la fois. Les modes changent la vitesse de réveil du processeur et sa fréquence.",
            ),
            Cat::Games => l(
                "Settings Windows has for games, most of them in Settings › Gaming and Display › Graphics.",
                "Des réglages que Windows a pour les jeux, la plupart dans Paramètres › Jeux et Affichage › Graphiques.",
            ),
            Cat::Windows => l(
                "Less work for the desktop and for apps you are not using.",
                "Moins de travail pour le bureau et pour les applications que vous n'utilisez pas.",
            ),
            Cat::Services => l(
                "Most PCs should leave these on. They help only in the cases written below.",
                "La plupart des PC devraient les laisser actifs. Ils n'aident que dans les cas décrits.",
            ),
            Cat::Fixes => l(
                "Not speed-ups: answers to one known problem. Tick only if you have it.",
                "Pas des accélérations : la réponse à un problème connu. À cocher seulement si vous l'avez.",
            ),
        }
    }
}

/// What it takes for a change to be in use.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum After {
    Now,
    /// The game has to be started again.
    Game,
    SignOut,
    Restart,
}

/// A registry value as written in the catalog.
#[derive(Clone, Copy, Debug)]
pub enum V {
    D(u32),
    S(&'static str),
    B(&'static [u8]),
}

impl V {
    pub fn val(self) -> crate::backend::Val {
        use crate::backend::Val;
        match self {
            V::D(d) => Val::Dword(d),
            V::S(s) => Val::Sz(s.into()),
            V::B(b) => Val::Bin(b.to_vec()),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Op {
    /// Sets a registry value. `absent`: what Windows does when the value doesn't exist.
    Reg { hive: Hive, path: &'static str, name: &'static str, val: V, absent: Option<V> },
    /// One `name=value;` pair of HKCU\Software\Microsoft\DirectX\UserGpuPreferences\DirectXUserGlobalSettings, the
    /// string Windows' graphics settings page writes.
    DxFlag { name: &'static str, val: &'static str },
    /// Makes a power plan active (copying it from Windows' hidden template if the PC doesn't list it yet).
    Plan(&'static str),
    /// A processor setting of the active plan.
    Power { setting: &'static str, ac: bool, val: u32 },
    /// A service's start type (2 automatic, 3 manual, 4 disabled).
    Service { name: &'static str, start: u32 },
}

impl Op {
    pub fn needs_admin(&self) -> bool {
        matches!(self, Op::Reg { hive: Hive::Machine, .. } | Op::Service { .. })
    }
}

/// What the PC must have for the tweak to make sense.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Needs {
    Nothing,
    Battery,
    /// Fast Startup works only with hibernation turned on.
    Hibernation,
    /// The plan (or its hidden template) exists on this PC.
    Plan(&'static str),
}

pub struct Tweak {
    pub id: &'static str,
    pub cat: Cat,
    pub title: L,
    pub summary: L,
    pub what: L,
    pub gain: L,
    pub cost: L,
    pub evidence: L,
    /// How to see the effect on this PC (a number or something visible).
    pub check: L,
    pub after: After,
    pub ops: &'static [Op],
    /// Tweaks of the same group exclude each other (one power plan).
    pub group: Option<&'static str>,
    pub needs: Needs,
}

impl Tweak {
    pub fn needs_admin(&self) -> bool {
        self.ops.iter().any(Op::needs_admin)
    }
}

const ADV: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";

pub static TWEAKS: &[Tweak] = &[
    // ── power ────────────────────────────────────────────────────────────────────────────────────────────────────
    Tweak {
        id: "plan_ultimate",
        cat: Cat::Power,
        title: l("Ultimate Performance plan", "Mode Performances optimales"),
        summary: l(
            "The processor never waits to wake up. For desktops plugged in.",
            "Le processeur n'attend jamais pour se réveiller. Pour les PC fixes branchés.",
        ),
        what: l(
            "Makes Windows' hidden \"Ultimate Performance\" plan visible and active. It is High performance with the last power-saving delays removed: the processor stays at full readiness and disks, USB and PCIe links don't go to sleep.",
            "Rend visible et actif le mode caché « Performances optimales » de Windows. C'est le mode Performances élevées sans les derniers délais d'économie : le processeur reste prêt en permanence, et disques, USB et liens PCIe ne s'endorment pas.",
        ),
        gain: l(
            "Shorter wake-up delays (measurable in Overview › Wake-up delay). Average frame rates usually move by 0 to 2 %, inside the noise of a benchmark; the gain is in fewer small hitches when the PC goes from idle to busy.",
            "Des délais de réveil plus courts (mesurables dans Aperçu › Délai de réveil). Les images par seconde moyennes bougent en général de 0 à 2 %, dans le bruit d'une mesure ; le gain est surtout moins de petits à-coups quand le PC passe du repos au travail.",
        ),
        cost: l(
            "More power and heat at idle, louder fans. On a laptop it empties the battery much faster. Some laptops (Modern Standby) refuse it: Windows keeps them on Balanced.",
            "Plus de consommation et de chaleur au repos, des ventilateurs plus bruyants. Sur un portable, la batterie se vide bien plus vite. Certains portables (veille moderne) le refusent : Windows les garde en Équilibré.",
        ),
        evidence: l(
            "Microsoft added it in Windows 10 1803 \"for workstations\" (GUID e9a42b02-d5df-448d-aa00-03f14749eb61). Independent comparisons find it identical to High performance for almost every setting when plugged in, and FPS changes of 1-2 %.",
            "Microsoft l'a ajouté dans Windows 10 1803 « pour les stations de travail » (GUID e9a42b02-d5df-448d-aa00-03f14749eb61). Les comparaisons indépendantes le trouvent identique à Performances élevées pour presque tous les réglages sur secteur, et des écarts de 1 à 2 % d'images par seconde.",
        ),
        check: l(
            "Overview › Wake-up delay, measured before and after: the typical delay drops (often by half on Balanced laptops).",
            "Aperçu › Délai de réveil, mesuré avant et après : le délai typique baisse (souvent de moitié sur un portable en Équilibré).",
        ),
        after: After::Now,
        ops: &[Op::Plan(guids::ULTIMATE)],
        group: Some("plan"),
        needs: Needs::Plan(guids::ULTIMATE),
    },
    Tweak {
        id: "plan_high",
        cat: Cat::Power,
        title: l("High performance plan", "Mode Performances élevées"),
        summary: l("The processor stays at full speed, plugged in or not.", "Le processeur reste à pleine vitesse, sur secteur ou non."),
        what: l(
            "Makes Windows' High performance plan active: the processor's minimum state is 100 % when plugged in, so it never slows down to save power.",
            "Active le mode Performances élevées de Windows : l'état minimal du processeur est de 100 % sur secteur, il ne ralentit donc jamais pour économiser.",
        ),
        gain: l(
            "Faster response to sudden work (opening apps, loading in games) and shorter wake-up delays. Same small FPS effect as Ultimate Performance.",
            "Une réponse plus rapide au travail soudain (ouvrir une appli, chargements en jeu) et des délais de réveil plus courts. Même petit effet sur les images par seconde que Performances optimales.",
        ),
        cost: l(
            "More power, heat and fan noise. On recent AMD Ryzen processors the chipset driver's own Balanced plan is tuned for them; High performance can cost boost headroom there.",
            "Plus de consommation, de chaleur et de bruit. Sur les Ryzen récents, le mode Équilibré du pilote de chipset AMD est réglé pour eux ; Performances élevées peut y réduire la marge de boost.",
        ),
        evidence: l(
            "Windows' built-in plan (GUID 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c), documented in Microsoft's power policy settings.",
            "Mode intégré à Windows (GUID 8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c), décrit dans la documentation Microsoft des réglages d'alimentation.",
        ),
        check: l(
            "Overview › Wake-up delay before and after; the processor clock at idle stays near its base clock.",
            "Aperçu › Délai de réveil avant et après ; au repos, la fréquence du processeur reste près de sa base.",
        ),
        after: After::Now,
        ops: &[Op::Plan(guids::HIGH)],
        group: Some("plan"),
        needs: Needs::Plan(guids::HIGH),
    },
    Tweak {
        id: "plan_balanced",
        cat: Cat::Power,
        title: l("Balanced plan", "Mode Équilibré"),
        summary: l(
            "Windows' default: full speed when needed, saving power otherwise.",
            "Le choix par défaut de Windows : pleine vitesse au besoin, économie sinon.",
        ),
        what: l(
            "Makes Windows' Balanced plan active. Modern processors (Intel Speed Shift, AMD CPPC) raise their clocks within milliseconds in this plan.",
            "Active le mode Équilibré de Windows. Les processeurs récents (Intel Speed Shift, AMD CPPC) y montent en fréquence en quelques millisecondes.",
        ),
        gain: l(
            "Lower temperature, quieter fans and much longer battery life, with nearly the same speed in games on recent processors.",
            "Moins de chaleur, des ventilateurs plus calmes et une bien meilleure autonomie, pour presque la même vitesse en jeu sur les processeurs récents.",
        ),
        cost: l(
            "Slightly longer wake-up delays than High performance (see Overview › Wake-up delay).",
            "Des délais de réveil un peu plus longs qu'en Performances élevées (voir Aperçu › Délai de réveil).",
        ),
        evidence: l(
            "Microsoft recommends Balanced for most PCs; Windows 11's Settings › Power mode slider works on top of it.",
            "Microsoft recommande Équilibré pour la plupart des PC ; le curseur Paramètres › Mode d'alimentation de Windows 11 s'appuie dessus.",
        ),
        check: l(
            "Overview › Processor clock at idle falls well below its base clock; battery time in Windows' battery report (powercfg /batteryreport).",
            "Aperçu › la fréquence du processeur au repos tombe bien sous sa base ; l'autonomie dans le rapport de batterie de Windows (powercfg /batteryreport).",
        ),
        after: After::Now,
        ops: &[Op::Plan(guids::BALANCED)],
        group: Some("plan"),
        needs: Needs::Plan(guids::BALANCED),
    },
    Tweak {
        id: "battery_boost_efficient",
        cat: Cat::Power,
        title: l("Gentler turbo on battery", "Turbo plus doux sur batterie"),
        summary: l("On battery, the processor boosts only when it's worth it.", "Sur batterie, le processeur ne monte en turbo que quand ça vaut la peine."),
        what: l(
            "Sets the active plan's \"processor performance boost mode\" to Efficient enabled on battery (plugged in is unchanged). The processor still boosts, but less eagerly.",
            "Règle le « mode de boost des performances du processeur » du mode actif sur Efficace activé sur batterie (sur secteur, rien ne change). Le processeur monte encore en turbo, mais moins vite.",
        ),
        gain: l(
            "Cooler and quieter laptop on battery, and more battery life: turbo clocks cost far more power than the speed they add.",
            "Un portable plus frais et plus silencieux sur batterie, et plus d'autonomie : les fréquences turbo coûtent bien plus d'énergie que la vitesse qu'elles ajoutent.",
        ),
        cost: l(
            "Short bursts of work (opening apps, compiling) are a little slower on battery.",
            "Les courtes rafales de travail (ouvrir une appli, compiler) sont un peu plus lentes sur batterie.",
        ),
        evidence: l(
            "PERFBOOSTMODE (be337238-0d82-4146-a960-4f3749d470c7) is documented by Microsoft in the processor power management options; values 0-6.",
            "PERFBOOSTMODE (be337238-0d82-4146-a960-4f3749d470c7) est documenté par Microsoft dans les options de gestion de l'alimentation du processeur ; valeurs 0 à 6.",
        ),
        check: l(
            "On battery, the processor clock under short loads (Overview) and the discharge rate in Windows' battery report.",
            "Sur batterie, la fréquence sous de courtes charges (Aperçu) et la vitesse de décharge dans le rapport de batterie de Windows.",
        ),
        after: After::Now,
        ops: &[Op::Power { setting: guids::BOOST_MODE, ac: false, val: 3 }],
        group: None,
        needs: Needs::Battery,
    },
    // ── games ────────────────────────────────────────────────────────────────────────────────────────────────────
    Tweak {
        id: "game_mode",
        cat: Cat::Games,
        title: l("Game Mode", "Mode Jeu"),
        summary: l(
            "Windows puts the game first and holds back updates during play.",
            "Windows donne la priorité au jeu et retient les mises à jour pendant la partie.",
        ),
        what: l(
            "Turns on Settings › Gaming › Game Mode. While a game runs, Windows gives it priority for processor time and doesn't install drivers or restart for updates.",
            "Active Paramètres › Jeux › Mode Jeu. Pendant qu'un jeu tourne, Windows lui donne la priorité sur le processeur et n'installe ni pilote ni redémarrage de mise à jour.",
        ),
        gain: l(
            "Steadier frame times when something works in the background; no update restart in the middle of a game. It is on by default: Plume Boost only turns it back on.",
            "Des temps d'image plus réguliers quand quelque chose travaille en arrière-plan ; pas de redémarrage de mise à jour en pleine partie. Il est actif par défaut : Plume Boost ne fait que le réactiver.",
        ),
        cost: l(
            "Almost none. Rarely, a game streamed or recorded at the same time gets fewer resources.",
            "Presque rien. Rarement, une diffusion ou un enregistrement en même temps reçoit moins de ressources.",
        ),
        evidence: l(
            "Microsoft Support: \"Use Game Mode while gaming on Windows\".",
            "Assistance Microsoft : « Utiliser le mode Jeu pendant le jeu sur Windows ».",
        ),
        check: l(
            "Frame-time graphs (PresentMon, CapFrameX) with a background task running: fewer spikes.",
            "Courbes de temps d'image (PresentMon, CapFrameX) avec une tâche de fond : moins de pics.",
        ),
        after: After::Now,
        ops: &[Op::Reg { hive: Hive::User, path: r"Software\Microsoft\GameBar", name: "AutoGameModeEnabled", val: V::D(1), absent: Some(V::D(1)) }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "hags",
        cat: Cat::Games,
        title: l("Hardware-accelerated GPU scheduling", "Planification GPU à accélération matérielle"),
        summary: l(
            "The graphics card schedules its own work. Needed for DLSS Frame Generation.",
            "La carte graphique planifie elle-même son travail. Nécessaire pour la génération d'images DLSS.",
        ),
        what: l(
            "Turns on Settings › Display › Graphics › Hardware-accelerated GPU scheduling: the graphics card's own processor manages its queue of work instead of Windows.",
            "Active Paramètres › Affichage › Graphiques › Planification GPU à accélération matérielle : le processeur de la carte graphique gère lui-même sa file de travail à la place de Windows.",
        ),
        gain: l(
            "A little less processor work and latency per frame; required by NVIDIA DLSS 3 Frame Generation and recommended by AMD for FSR Frame Generation. Average FPS rarely changes by more than a few %.",
            "Un peu moins de travail processeur et de latence par image ; exigée par la génération d'images NVIDIA DLSS 3 et recommandée par AMD pour celle de FSR. Les images par seconde moyennes changent rarement de plus de quelques %.",
        ),
        cost: l(
            "Needs a graphics card and driver with WDDM 2.7 or newer (NVIDIA GTX 10 / AMD RX 5000 and later); on older ones it does nothing. A few users see stutter with some capture tools: undo it if so.",
            "Demande une carte et un pilote WDDM 2.7 ou plus récents (NVIDIA GTX 10 / AMD RX 5000 et après) ; sur les plus anciens, rien ne change. Quelques utilisateurs voient des saccades avec certains outils de capture : annulez-la si c'est le cas.",
        ),
        evidence: l(
            "Microsoft DirectX Developer Blog \"Hardware Accelerated GPU Scheduling\" (2020); NVIDIA lists it as a requirement for DLSS Frame Generation. Value HwSchMode = 2.",
            "Blog des développeurs DirectX de Microsoft « Hardware Accelerated GPU Scheduling » (2020) ; NVIDIA l'indique comme condition pour la génération d'images DLSS. Valeur HwSchMode = 2.",
        ),
        check: l(
            "Settings › Display › Graphics shows it on after a restart; DLSS Frame Generation becomes available in games.",
            "Paramètres › Affichage › Graphiques l'indique actif après redémarrage ; la génération d'images DLSS devient disponible dans les jeux.",
        ),
        after: After::Restart,
        ops: &[Op::Reg { hive: Hive::Machine, path: r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers", name: "HwSchMode", val: V::D(2), absent: None }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "windowed_opt",
        cat: Cat::Games,
        title: l("Optimizations for windowed games", "Optimisations pour les jeux fenêtrés"),
        summary: l(
            "Older games in a window or borderless get the modern, faster way to show frames.",
            "Les jeux plus anciens en fenêtre ou sans bordure reçoivent la façon moderne et plus rapide d'afficher les images.",
        ),
        what: l(
            "Turns on Settings › Display › Graphics › Optimizations for windowed games: DirectX 10 and 11 games shown in a window switch to the \"flip model\" of presentation.",
            "Active Paramètres › Affichage › Graphiques › Optimisations pour les jeux fenêtrés : les jeux DirectX 10 et 11 affichés en fenêtre passent au « modèle flip » de présentation.",
        ),
        gain: l(
            "Lower input latency in windowed and borderless games, and they can use variable refresh rate and Auto HDR.",
            "Moins de latence dans les jeux en fenêtre et sans bordure, qui peuvent aussi profiter du taux de rafraîchissement variable et de l'Auto HDR.",
        ),
        cost: l(
            "A few old games show glitches with it; they can be excluded one by one in the same Settings page.",
            "Quelques vieux jeux ont des défauts d'affichage avec ; on peut les exclure un par un dans la même page de Paramètres.",
        ),
        evidence: l(
            "Microsoft DirectX Developer Blog \"Gaming performance and compatibility for windowed games\" (2022). Stored as SwapEffectUpgradeEnable=1 in DirectXUserGlobalSettings.",
            "Blog des développeurs DirectX de Microsoft sur les jeux fenêtrés (2022). Enregistré comme SwapEffectUpgradeEnable=1 dans DirectXUserGlobalSettings.",
        ),
        check: l(
            "PresentMon shows the game's present mode change from \"Composed: Copy\" to a flip mode in a window.",
            "PresentMon montre le mode de présentation du jeu passer de « Composed: Copy » à un mode flip en fenêtre.",
        ),
        after: After::Game,
        ops: &[Op::DxFlag { name: "SwapEffectUpgradeEnable", val: "1" }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "vrr",
        cat: Cat::Games,
        title: l("Variable refresh rate for all games", "Taux de rafraîchissement variable pour tous les jeux"),
        summary: l(
            "G-SYNC / FreeSync also for DirectX 11 games that don't ask for it.",
            "G-SYNC / FreeSync aussi pour les jeux DirectX 11 qui ne le demandent pas.",
        ),
        what: l(
            "Turns on Settings › Display › Graphics › Variable refresh rate: Windows lets DirectX 11 games use the screen's variable refresh even when they don't support it themselves.",
            "Active Paramètres › Affichage › Graphiques › Taux de rafraîchissement variable : Windows laisse les jeux DirectX 11 utiliser le rafraîchissement variable de l'écran même s'ils ne le gèrent pas eux-mêmes.",
        ),
        gain: l(
            "No tearing and less stutter when the frame rate is below the screen's refresh rate.",
            "Pas de déchirure et moins de saccades quand les images par seconde sont sous la fréquence de l'écran.",
        ),
        cost: l(
            "Only useful with a G-SYNC / FreeSync / adaptive-sync screen; otherwise nothing changes.",
            "Utile seulement avec un écran G-SYNC / FreeSync / adaptive-sync ; sinon rien ne change.",
        ),
        evidence: l(
            "Microsoft Support: \"Variable refresh rate for games\". Stored as VRROptimizeEnable=1 in DirectXUserGlobalSettings.",
            "Assistance Microsoft sur le taux de rafraîchissement variable. Enregistré comme VRROptimizeEnable=1 dans DirectXUserGlobalSettings.",
        ),
        check: l(
            "The screen's on-screen display shows the refresh rate following the frame rate in DirectX 11 games.",
            "L'affichage du moniteur montre la fréquence qui suit les images par seconde dans les jeux DirectX 11.",
        ),
        after: After::Game,
        ops: &[Op::DxFlag { name: "VRROptimizeEnable", val: "1" }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "gamedvr_off",
        cat: Cat::Games,
        title: l("Turn off Xbox Game Bar captures", "Couper les captures de la Xbox Game Bar"),
        summary: l("Games are no longer watched for recording in the background.", "Les jeux ne sont plus surveillés pour être enregistrés en arrière-plan."),
        what: l(
            "Turns off Game DVR (Settings › Gaming › Captures): background recording (\"Record what happened\") and the capture hook Windows puts in every game.",
            "Désactive Game DVR (Paramètres › Jeux › Captures) : l'enregistrement en arrière-plan (« Enregistrer ce qui s'est passé ») et le crochet de capture que Windows met dans chaque jeu.",
        ),
        gain: l(
            "If background recording was on: the graphics card's video encoder and a few % of GPU time come back to the game. If it was off, the gain is small (no capture hook).",
            "Si l'enregistrement en arrière-plan était actif : l'encodeur vidéo de la carte et quelques % de GPU reviennent au jeu. S'il était coupé, le gain est faible (plus de crochet de capture).",
        ),
        cost: l(
            "Win+Alt+R and Win+Alt+G recording stop working. NVIDIA ShadowPlay, AMD ReLive and OBS still work.",
            "Win+Alt+R et Win+Alt+G n'enregistrent plus. NVIDIA ShadowPlay, AMD ReLive et OBS fonctionnent toujours.",
        ),
        evidence: l(
            "Microsoft Support (Xbox Game Bar captures) states background recording \"may affect game performance\". Values GameDVR_Enabled, AppCaptureEnabled, HistoricalCaptureEnabled = 0.",
            "L'assistance Microsoft (captures de la Xbox Game Bar) indique que l'enregistrement en arrière-plan « peut affecter les performances du jeu ». Valeurs GameDVR_Enabled, AppCaptureEnabled, HistoricalCaptureEnabled = 0.",
        ),
        check: l(
            "Task Manager › GPU › Video Encode is idle during play when background recording was on before.",
            "Gestionnaire des tâches › GPU › Encodage vidéo reste au repos en jeu si l'enregistrement en arrière-plan était actif.",
        ),
        after: After::Game,
        ops: &[
            Op::Reg { hive: Hive::User, path: r"System\GameConfigStore", name: "GameDVR_Enabled", val: V::D(0), absent: Some(V::D(1)) },
            Op::Reg {
                hive: Hive::User,
                path: r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
                name: "AppCaptureEnabled",
                val: V::D(0),
                absent: Some(V::D(1)),
            },
            Op::Reg {
                hive: Hive::User,
                path: r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
                name: "HistoricalCaptureEnabled",
                val: V::D(0),
                absent: Some(V::D(0)),
            },
        ],
        group: None,
        needs: Needs::Nothing,
    },
    // ── windows ──────────────────────────────────────────────────────────────────────────────────────────────────
    Tweak {
        id: "visual_fx",
        cat: Cat::Windows,
        title: l("Visual effects for performance", "Effets visuels pour les performances"),
        summary: l("No animations, shadows or fades; text stays smooth.", "Plus d'animations, d'ombres ni de fondus ; le texte reste lisse."),
        what: l(
            "The \"Adjust for best performance\" choice of System › Advanced › Performance options, except smooth screen fonts and thumbnails, which are kept: window and taskbar animations, menu fades and shadows are turned off.",
            "Le choix « Ajuster afin d'obtenir les meilleures performances » des options de performances, sauf le lissage des polices et les miniatures, gardés : animations des fenêtres et de la barre des tâches, fondus et ombres sont coupés.",
        ),
        gain: l(
            "Windows feel instant (no 200 ms animations) and the desktop uses less GPU: noticeable on integrated graphics and older PCs, invisible on a gaming PC.",
            "Les fenêtres réagissent instantanément (plus d'animations de 200 ms) et le bureau sollicite moins le GPU : sensible sur une puce graphique intégrée ou un vieux PC, invisible sur un PC de jeu.",
        ),
        cost: l("Windows look plainer. Takes effect after signing out.", "Windows a l'air plus sobre. Prend effet après une déconnexion."),
        evidence: l(
            "The same values Windows writes for this choice: UserPreferencesMask 90 12 03 80 10 00 00 00, MinAnimate 0, TaskbarAnimations 0, VisualFXSetting 3 (custom).",
            "Les mêmes valeurs que Windows écrit pour ce choix : UserPreferencesMask 90 12 03 80 10 00 00 00, MinAnimate 0, TaskbarAnimations 0, VisualFXSetting 3 (personnalisé).",
        ),
        check: l(
            "Windows open and minimize without the 200 ms animation; Task Manager › GPU of dwm.exe drops on integrated graphics.",
            "Les fenêtres s'ouvrent et se réduisent sans l'animation de 200 ms ; le GPU de dwm.exe baisse sur une puce intégrée.",
        ),
        after: After::SignOut,
        ops: &[
            Op::Reg {
                hive: Hive::User,
                path: r"Control Panel\Desktop",
                name: "UserPreferencesMask",
                val: V::B(&[0x90, 0x12, 0x03, 0x80, 0x10, 0x00, 0x00, 0x00]),
                absent: None,
            },
            Op::Reg {
                hive: Hive::User,
                path: r"Software\Microsoft\Windows\CurrentVersion\Explorer\VisualEffects",
                name: "VisualFXSetting",
                val: V::D(3),
                absent: Some(V::D(0)),
            },
            Op::Reg { hive: Hive::User, path: r"Control Panel\Desktop\WindowMetrics", name: "MinAnimate", val: V::S("0"), absent: Some(V::S("1")) },
            Op::Reg { hive: Hive::User, path: ADV, name: "TaskbarAnimations", val: V::D(0), absent: Some(V::D(1)) },
        ],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "transparency_off",
        cat: Cat::Windows,
        title: l("No transparency effects", "Pas d'effets de transparence"),
        summary: l("Opaque Start menu, taskbar and windows: less GPU work.", "Menu Démarrer, barre des tâches et fenêtres opaques : moins de travail GPU."),
        what: l(
            "Turns off Settings › Personalization › Colors › Transparency effects (the blurred \"acrylic\" and \"mica\" backgrounds).",
            "Désactive Paramètres › Personnalisation › Couleurs › Effets de transparence (les fonds flous « acrylique » et « mica »).",
        ),
        gain: l(
            "Less GPU and power use by the desktop: helps integrated graphics and battery life, nothing on a strong graphics card.",
            "Moins de GPU et d'énergie pour le bureau : aide les puces graphiques intégrées et l'autonomie, rien sur une carte puissante.",
        ),
        cost: l("Flat colours instead of blur.", "Des couleurs unies à la place du flou."),
        evidence: l(
            "Windows' own setting; Windows turns it off by itself in Energy saver mode on battery.",
            "Réglage de Windows ; Windows le coupe lui-même en mode Économiseur d'énergie sur batterie.",
        ),
        check: l(
            "Task Manager: GPU use of dwm.exe and power use, on integrated graphics.",
            "Gestionnaire des tâches : utilisation GPU de dwm.exe et consommation, sur une puce intégrée.",
        ),
        after: After::Now,
        ops: &[Op::Reg {
            hive: Hive::User,
            path: r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
            name: "EnableTransparency",
            val: V::D(0),
            absent: Some(V::D(1)),
        }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "startup_delay_off",
        cat: Cat::Windows,
        title: l("No wait for startup apps", "Pas d'attente pour les applis au démarrage"),
        summary: l("Apps that start with Windows open right after you sign in.", "Les applis qui démarrent avec Windows s'ouvrent dès la connexion."),
        what: l(
            "Removes the delay (about 10 seconds) Windows waits after you sign in before starting the programs of the Startup list.",
            "Supprime le délai (environ 10 secondes) que Windows attend après la connexion avant de lancer les programmes de démarrage.",
        ),
        gain: l(
            "Your startup apps are ready several seconds sooner. Measurable with a stopwatch from sign-in.",
            "Vos applis de démarrage sont prêtes plusieurs secondes plus tôt. Mesurable au chronomètre depuis la connexion.",
        ),
        cost: l(
            "The first seconds after sign-in are busier; on a hard disk (not SSD) the desktop can feel slower to settle.",
            "Les premières secondes après la connexion sont plus chargées ; sur un disque dur (pas SSD), le bureau peut sembler plus lent à se poser.",
        ),
        evidence: l(
            "Explorer reads StartupDelayInMSec under Explorer\\Serialize (documented by Microsoft engineers in Windows Performance Toolkit talks).",
            "L'Explorateur lit StartupDelayInMSec sous Explorer\\Serialize (décrit par des ingénieurs Microsoft dans des présentations du Windows Performance Toolkit).",
        ),
        check: l(
            "A stopwatch from sign-in to your startup apps' icons near the clock: several seconds sooner.",
            "Un chronomètre de la connexion jusqu'aux icônes des applis de démarrage près de l'horloge : plusieurs secondes plus tôt.",
        ),
        after: After::SignOut,
        ops: &[Op::Reg {
            hive: Hive::User,
            path: r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
            name: "StartupDelayInMSec",
            val: V::D(0),
            absent: None,
        }],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "fast_startup_on",
        cat: Cat::Windows,
        title: l("Fast Startup", "Démarrage rapide"),
        summary: l(
            "\"Shut down\" saves Windows' core so the next start is quicker.",
            "« Arrêter » enregistre le cœur de Windows pour que le prochain démarrage soit plus rapide.",
        ),
        what: l(
            "Turns on Control Panel › Power options › Turn on fast startup. Shutting down saves the Windows kernel and drivers to disk (like hibernation, without your apps). \"Restart\" is not affected.",
            "Active Panneau de configuration › Options d'alimentation › Activer le démarrage rapide. L'arrêt enregistre le noyau et les pilotes sur le disque (comme la mise en veille prolongée, sans vos applis). « Redémarrer » n'est pas concerné.",
        ),
        gain: l(
            "A start from \"Shut down\" takes a few seconds less (more on hard disks). It is on by default: Plume Boost only turns it back on.",
            "Un démarrage après « Arrêter » prend quelques secondes de moins (plus sur un disque dur). Il est actif par défaut : Plume Boost ne fait que le réactiver.",
        ),
        cost: l(
            "Drivers are not fully restarted at each shutdown (use Restart after driver problems); dual-boot PCs may not see the Windows disk as cleanly closed.",
            "Les pilotes ne repartent pas de zéro à chaque arrêt (utilisez Redémarrer après un souci de pilote) ; en double démarrage, l'autre système peut voir le disque Windows mal fermé.",
        ),
        evidence: l(
            "Microsoft Learn: \"Distinguishing Fast Startup from Wake-from-Hibernation\". Value HiberbootEnabled = 1.",
            "Microsoft Learn : distinguer le démarrage rapide de la sortie de veille prolongée. Valeur HiberbootEnabled = 1.",
        ),
        check: l(
            "Event Viewer › Windows Logs › System › Kernel-Boot event 27 shows boot type 0x1 (fast startup); time from power button to sign-in.",
            "Observateur d'événements › Journaux Windows › Système › événement Kernel-Boot 27 : type 0x1 (démarrage rapide) ; temps du bouton à la connexion.",
        ),
        after: After::Now,
        ops: &[Op::Reg {
            hive: Hive::Machine,
            path: r"SYSTEM\CurrentControlSet\Control\Session Manager\Power",
            name: "HiberbootEnabled",
            val: V::D(1),
            absent: Some(V::D(1)),
        }],
        group: None,
        needs: Needs::Hibernation,
    },
    // ── services ─────────────────────────────────────────────────────────────────────────────────────────────────
    Tweak {
        id: "sysmain_off",
        cat: Cat::Fixes,
        title: l("Turn off SysMain (hard disk stuck at 100 %)", "Désactiver SysMain (disque dur bloqué à 100 %)"),
        summary: l(
            "Stops preloading apps into memory. Only for PCs with a slow hard disk that's always busy.",
            "Arrête le préchargement des applis en mémoire. Seulement pour un PC à disque dur lent toujours occupé.",
        ),
        what: l(
            "Sets the SysMain service to Disabled. SysMain learns which apps you use and loads them into free memory ahead of time.",
            "Règle le service SysMain sur Désactivé. SysMain apprend quelles applis vous utilisez et les charge à l'avance dans la mémoire libre.",
        ),
        gain: l(
            "Less disk and processor activity in the background, mostly right after startup on a hard disk. On an SSD the difference is hard to measure.",
            "Moins d'activité disque et processeur en arrière-plan, surtout juste après le démarrage sur un disque dur. Sur un SSD, la différence est difficile à mesurer.",
        ),
        cost: l(
            "Apps take longer to open, because nothing is preloaded. Microsoft recommends leaving it on.",
            "Les applis mettent plus de temps à s'ouvrir, puisque rien n'est préchargé. Microsoft recommande de le laisser actif.",
        ),
        evidence: l(
            "Microsoft Learn describes SysMain as maintaining and improving system performance; disabling it is a common fix for 100 % disk use on hard disks.",
            "Microsoft Learn décrit SysMain comme maintenant et améliorant les performances ; le désactiver est un remède courant au disque à 100 % sur les disques durs.",
        ),
        check: l(
            "Task Manager › Disk: the hard disk is no longer at 100 % for minutes after startup.",
            "Gestionnaire des tâches › Disque : le disque dur n'est plus à 100 % pendant des minutes après le démarrage.",
        ),
        after: After::Restart,
        ops: &[Op::Service { name: "SysMain", start: 4 }],
        group: None,
        needs: Needs::Nothing,
    },
    // ── fixes ────────────────────────────────────────────────────────────────────────────────────────────────────
    Tweak {
        id: "mpo_off",
        cat: Cat::Fixes,
        title: l("Turn off Multiplane Overlay (MPO)", "Désactiver la superposition multiplan (MPO)"),
        summary: l(
            "Fixes flicker, black flashes or stutter in browsers and games on some multi-screen setups.",
            "Corrige scintillements, flashs noirs ou saccades dans les navigateurs et jeux sur certaines configurations multi-écrans.",
        ),
        what: l(
            "Tells the desktop compositor not to use the graphics card's overlay planes (OverlayTestMode = 5).",
            "Demande au compositeur du bureau de ne pas utiliser les plans de superposition de la carte graphique (OverlayTestMode = 5).",
        ),
        gain: l(
            "Only if you have the problem: the flicker or stutter goes away. Otherwise there is nothing to gain.",
            "Seulement si vous avez le problème : les scintillements ou saccades disparaissent. Sinon, rien à gagner.",
        ),
        cost: l(
            "Video playback and windowed games use a little more GPU and power. Not a setting Microsoft documents publicly.",
            "La vidéo et les jeux fenêtrés consomment un peu plus de GPU et d'énergie. Réglage non documenté publiquement par Microsoft.",
        ),
        evidence: l(
            "NVIDIA's support article on MPO flickering ships this exact value (and its undo). Reports agree it still works on Windows 11 24H2.",
            "L'article d'assistance NVIDIA sur le scintillement MPO fournit exactement cette valeur (et son annulation). Les retours confirment qu'elle marche encore sous Windows 11 24H2.",
        ),
        check: l("The flicker or black flashes stop (or not: then undo it).", "Les scintillements ou flashs noirs disparaissent (sinon, annulez-le)."),
        after: After::Restart,
        ops: &[Op::Reg { hive: Hive::Machine, path: r"SOFTWARE\Microsoft\Windows\Dwm", name: "OverlayTestMode", val: V::D(5), absent: None }],
        group: None,
        needs: Needs::Nothing,
    },
];

pub fn by_id(id: &str) -> Option<&'static Tweak> {
    TWEAKS.iter().find(|t| t.id == id)
}

/// A tweak of now or of an earlier version (titles in the Changes page).
pub fn any_by_id(id: &str) -> Option<&'static Tweak> {
    by_id(id).or_else(|| RETIRED.iter().find(|t| t.id == id))
}

/// Tweaks earlier versions offered and that were taken out after the audit of version 1.2 (see NO_EFFECT): kept only
/// so an undo log that still holds them shows their names, and so snapshots save their values.
pub static RETIRED: &[Tweak] = &[
    Tweak {
        id: "background_apps_off",
        cat: Cat::Windows,
        title: l("Stop background apps", "Arrêter les applis en arrière-plan"),
        summary: l("Store apps no longer run when you're not using them.", "Les applis du Store ne tournent plus quand vous ne les utilisez pas."),
        what: l(
            "Stops apps from the Microsoft Store (Weather, Phone Link, Xbox app…) from running in the background. Classic desktop programs are not affected.",
            "Empêche les applis du Microsoft Store (Météo, Mobile connecté, appli Xbox…) de tourner en arrière-plan. Les programmes de bureau classiques ne sont pas concernés.",
        ),
        gain: l(
            "A few fewer processes and a little less memory and wake-ups; mostly helps battery life and PCs with 8 GB or less.",
            "Quelques processus en moins, un peu moins de mémoire et de réveils ; aide surtout l'autonomie et les PC de 8 Go ou moins.",
        ),
        cost: l(
            "Those apps stop updating live tiles and stop sending notifications until you open them (Phone Link, Teams from the Store, Mail).",
            "Ces applis ne mettent plus à jour leurs vignettes et n'envoient plus de notifications tant que vous ne les ouvrez pas (Mobile connecté, Teams du Store, Courrier).",
        ),
        evidence: l(
            "The per-user switch Windows 10 had in Settings › Privacy › Background apps (GlobalUserDisabled = 1, BackgroundAppGlobalToggle = 0); Windows 11 keeps a per-app version of it.",
            "L'interrupteur par utilisateur que Windows 10 avait dans Paramètres › Confidentialité › Applications en arrière-plan (GlobalUserDisabled = 1, BackgroundAppGlobalToggle = 0) ; Windows 11 en garde une version par appli.",
        ),
        check: l("", ""),
        after: After::Now,
        ops: &[
            Op::Reg {
                hive: Hive::User,
                path: r"Software\Microsoft\Windows\CurrentVersion\BackgroundAccessApplications",
                name: "GlobalUserDisabled",
                val: V::D(1),
                absent: Some(V::D(0)),
            },
            Op::Reg {
                hive: Hive::User,
                path: r"Software\Microsoft\Windows\CurrentVersion\Search",
                name: "BackgroundAppGlobalToggle",
                val: V::D(0),
                absent: Some(V::D(1)),
            },
        ],
        group: None,
        needs: Needs::Nothing,
    },
    Tweak {
        id: "search_off",
        cat: Cat::Services,
        title: l("Turn off Windows Search indexing", "Désactiver l'indexation de Windows Search"),
        summary: l(
            "No more file indexing. Start and Explorer search get slow.",
            "Plus d'indexation des fichiers. La recherche du menu Démarrer et de l'Explorateur devient lente.",
        ),
        what: l(
            "Sets the Windows Search service (WSearch) to Disabled: Windows stops keeping an index of your files and emails.",
            "Règle le service Windows Search (WSearch) sur Désactivé : Windows ne tient plus d'index de vos fichiers et courriels.",
        ),
        gain: l(
            "No indexing bursts of disk and processor after you add many files; measurable in Plume Task as SearchIndexer.exe.",
            "Plus de rafales d'indexation disque et processeur après l'ajout de nombreux fichiers ; mesurable dans Plume Task (SearchIndexer.exe).",
        ),
        cost: l(
            "Searching files from Start, Explorer and Outlook becomes slow or incomplete. Settings and app search still work.",
            "La recherche de fichiers depuis Démarrer, l'Explorateur et Outlook devient lente ou incomplète. La recherche d'applis et de paramètres marche toujours.",
        ),
        evidence: l(
            "Microsoft Learn: \"Windows Search\" service; Settings › Privacy & security › Searching Windows lets you limit indexing to fewer folders instead.",
            "Microsoft Learn : service « Windows Search » ; Paramètres › Confidentialité et sécurité › Recherche dans Windows permet plutôt de limiter l'index à moins de dossiers.",
        ),
        check: l("", ""),
        after: After::Restart,
        ops: &[Op::Service { name: "WSearch", start: 4 }],
        group: None,
        needs: Needs::Nothing,
    },
];

/// Popular "optimizations" Plume Boost doesn't offer, and why: none has a measurable effect on a current Windows, and
/// some do harm. Shown on the Tweaks page.
pub static NO_EFFECT: &[(L, L)] = &[
    (
        l("Stop background apps (Windows 11)", "Arrêter les applis en arrière-plan (Windows 11)"),
        l(
            "Removed in Plume Boost 1.2. Windows 11 suspends Store apps you aren't using by itself; the old global switch saves no measurable memory or power, and it stops notifications from Phone Link, Teams and Mail.",
            "Retiré dans Plume Boost 1.2. Windows 11 suspend déjà les applis du Store inutilisées ; l'ancien interrupteur global n'économise ni mémoire ni énergie mesurables, et coupe les notifications de Mobile connecté, Teams et Courrier.",
        ),
    ),
    (
        l("Turn off Windows Search indexing", "Couper l'indexation de Windows Search"),
        l(
            "Removed in Plume Boost 1.2. The indexer backs off by itself while you use the PC, play or run on battery, so games gain nothing measurable, and file search gets slow. Limit the indexed folders in Settings instead.",
            "Retiré dans Plume Boost 1.2. L'indexeur se met de lui-même en retrait quand vous utilisez le PC, jouez ou êtes sur batterie : les jeux n'y gagnent rien de mesurable, et la recherche devient lente. Limitez plutôt les dossiers indexés dans Paramètres.",
        ),
    ),
    (
        l("\"RAM cleaners\" and memory boosters", "« Nettoyeurs de RAM » et boosters de mémoire"),
        l(
            "Free memory is wasted memory: Windows keeps files cached in it and hands it back instantly. Emptying it only forces Windows to read those files again from disk.",
            "La mémoire libre est de la mémoire perdue : Windows y garde des fichiers en cache et la rend aussitôt. La vider oblige seulement Windows à relire ces fichiers sur le disque.",
        ),
    ),
    (
        l("Registry cleaners and registry \"speed\" values", "Nettoyeurs de Registre et valeurs « accélératrices »"),
        l(
            "Unused registry entries cost nothing measurable. Values like NetworkThrottlingIndex, SystemResponsiveness = 0, Win32PrioritySeparation or TcpAckFrequency come from Windows XP / Vista days; tests on current Windows show no gain, and some cause audio crackles.",
            "Les entrées inutilisées ne coûtent rien de mesurable. Des valeurs comme NetworkThrottlingIndex, SystemResponsiveness = 0, Win32PrioritySeparation ou TcpAckFrequency datent de Windows XP / Vista ; les tests sur un Windows actuel ne montrent aucun gain, et certaines font craquer le son.",
        ),
    ),
    (
        l("A global timer resolution of 0.5 ms", "Une résolution de minuterie globale de 0,5 ms"),
        l(
            "Since Windows 10 2004, timer requests are per program: games ask for their own. Forcing it for everything only costs power. Overview shows the timer as it is.",
            "Depuis Windows 10 2004, les demandes de minuterie sont par programme : les jeux demandent la leur. La forcer pour tout ne coûte que de l'énergie. L'Aperçu montre la minuterie telle qu'elle est.",
        ),
    ),
    (
        l("HPET off, bcdedit tweaks, MSI mode for every device", "HPET coupé, réglages bcdedit, mode MSI pour tous les périphériques"),
        l(
            "Results vary from one PC to the next and from one benchmark run to the next; done wrong they can stop Windows from starting. No consistent, reproducible gain.",
            "Les résultats varient d'un PC à l'autre et d'une mesure à l'autre ; mal faits, ils peuvent empêcher Windows de démarrer. Aucun gain constant et reproductible.",
        ),
    ),
    (
        l("Turning off Defender, memory integrity, Windows Update", "Couper Defender, l'intégrité de la mémoire, Windows Update"),
        l(
            "Memory integrity can cost a few % in some games (Microsoft said so in 2022), but it protects Windows' core. Plume Boost never weakens security.",
            "L'intégrité de la mémoire peut coûter quelques % dans certains jeux (Microsoft l'a dit en 2022), mais elle protège le cœur de Windows. Plume Boost n'affaiblit jamais la sécurité.",
        ),
    ),
    (
        l("A \"performance score\"", "Un « score de performance »"),
        l(
            "A score made up by an app measures nothing. Plume Boost shows only numbers Windows and the drivers report: clocks, wake-up delay, temperatures, power.",
            "Un score inventé par une appli ne mesure rien. Plume Boost ne montre que des chiffres donnés par Windows et les pilotes : fréquences, délai de réveil, températures, puissance.",
        ),
    ),
];

/// Every registry value the tweaks (current and retired), the DirectX string and the sign-in value can change.
pub fn registry_values() -> Vec<(Hive, String, String)> {
    let mut out = Vec::new();
    for t in TWEAKS.iter().chain(RETIRED.iter()) {
        for op in t.ops {
            match *op {
                Op::Reg { hive, path, name, .. } => out.push((hive, path.to_string(), name.to_string())),
                Op::DxFlag { .. } => {
                    out.push((Hive::User, r"Software\Microsoft\DirectX\UserGpuPreferences".to_string(), "DirectXUserGlobalSettings".to_string()))
                }
                _ => {}
            }
        }
    }
    out.push((Hive::User, crate::backend::RUN_KEY.to_string(), crate::backend::RUN_NAME.to_string()));
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    Gaming,
    Balanced,
    Battery,
}

impl Profile {
    pub const ALL: [Profile; 3] = [Profile::Gaming, Profile::Balanced, Profile::Battery];
    pub fn key(self) -> &'static str {
        match self {
            Profile::Gaming => "gaming",
            Profile::Balanced => "balanced",
            Profile::Battery => "battery",
        }
    }
    pub fn title(self) -> L {
        match self {
            Profile::Gaming => l("Gaming", "Jeu"),
            Profile::Balanced => l("Balanced", "Équilibré"),
            Profile::Battery => l("Laptop on battery", "Portable sur batterie"),
        }
    }
    pub fn line(self) -> L {
        match self {
            Profile::Gaming => l("Every frame counts. For a PC plugged in.", "Chaque image compte. Pour un PC branché."),
            Profile::Balanced => l("Snappy and sensible, for every day.", "Réactif et raisonnable, pour tous les jours."),
            Profile::Battery => l("Cooler, quieter, longer battery life.", "Plus frais, plus calme, plus d'autonomie."),
        }
    }
    /// The tweaks it ticks, in order of preference (the first available plan of the list is used).
    pub fn ids(self) -> &'static [&'static str] {
        match self {
            Profile::Gaming => &["plan_ultimate", "plan_high", "game_mode", "hags", "windowed_opt", "vrr", "gamedvr_off"],
            Profile::Balanced => &["plan_balanced", "game_mode", "windowed_opt", "gamedvr_off", "startup_delay_off", "fast_startup_on"],
            Profile::Battery => &["plan_balanced", "battery_boost_efficient", "game_mode", "transparency_off", "visual_fx", "gamedvr_off", "fast_startup_on"],
        }
    }
    /// The ticks of this profile on a PC where `available` tells what exists: only one plan, the first available.
    pub fn ticks(self, available: impl Fn(&str) -> bool) -> Vec<&'static str> {
        let mut out: Vec<&'static str> = Vec::new();
        let mut plan = false;
        for id in self.ids() {
            let Some(t) = by_id(id) else { continue };
            if !available(id) {
                continue;
            }
            if t.group == Some("plan") {
                if plan {
                    continue;
                }
                plan = true;
            }
            out.push(id);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_consistent() {
        let mut ids: Vec<&str> = TWEAKS.iter().map(|t| t.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), TWEAKS.len(), "ids are unique");
        for t in TWEAKS {
            assert!(!t.ops.is_empty(), "{} changes something", t.id);
            for text in [t.title, t.summary, t.what, t.gain, t.cost, t.evidence, t.check] {
                assert!(!text.en.is_empty() && !text.fr.is_empty(), "{} is written in both languages", t.id);
            }
        }
        for t in RETIRED {
            assert!(by_id(t.id).is_none(), "{} is retired, not offered", t.id);
        }
        for (a, b) in NO_EFFECT {
            assert!(!a.en.is_empty() && !a.fr.is_empty() && !b.en.is_empty() && !b.fr.is_empty());
        }
        for p in Profile::ALL {
            for id in p.ids() {
                assert!(by_id(id).is_some(), "{id} of {:?} exists", p);
            }
        }
    }

    #[test]
    fn profiles_tick_one_plan() {
        let all = Profile::Gaming.ticks(|_| true);
        assert!(all.contains(&"plan_ultimate") && !all.contains(&"plan_high"));
        // a laptop that hides Ultimate Performance falls back to High performance
        let no_ult = Profile::Gaming.ticks(|id| id != "plan_ultimate");
        assert!(no_ult.contains(&"plan_high") && !no_ult.contains(&"plan_ultimate"));
        // a desktop: no battery tweak
        let desk = Profile::Battery.ticks(|id| id != "battery_boost_efficient");
        assert!(!desk.contains(&"battery_boost_efficient") && desk.contains(&"plan_balanced"));
    }
}
