//! Serveurs SFTP, par le client OpenSSH livré avec Windows (`sftp.exe`) : rien à installer, et le chiffrement,
//! les clés et les serveurs déjà connus sont ceux de Windows.
//!
//! Une session garde un `sftp.exe` ouvert. On lui écrit des commandes ; on lit ce qu'il répond. Deux habitudes de
//! ce programme règlent la façon de lire :
//! - il répète chaque commande reçue (« sftp> ls ... ») avant d'y répondre : la réponse d'une commande est ce qui
//!   se trouve entre sa répétition et celle de la commande suivante, qui est toujours `version` ;
//! - sous Windows, sa sortie ne part que par blocs de 4 Ko : pour qu'une réponse courte arrive tout de suite, on
//!   la fait suivre de quelques `help`, dont le texte pousse le bloc dehors.
//!
//! Entre guillemets, un chemin est pris tel quel par ce `sftp.exe` : espaces, crochets et accents compris.
//!
//! Quand le serveur veut un mot de passe, `ssh` lance le programme que désigne SSH_ASKPASS : nous-mêmes, qui
//! passons la question à la fenêtre par un tuyau nommé (voir [`askpass`]).

use std::{
    fs::{File, OpenOptions},
    hash::{BuildHasher, Hasher},
    io::{BufRead, BufReader, Read, Write},
    os::windows::{io::FromRawHandle, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{FILETIME, SYSTEMTIME},
        Storage::FileSystem::PIPE_ACCESS_DUPLEX,
        System::{
            Pipes::{ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE, PIPE_WAIT},
            SystemInformation::GetLocalTime,
            Time::{SystemTimeToFileTime, TzSpecificLocalTimeToSystemTime},
        },
    },
    core::PCWSTR,
};

/// Variable d'environnement qui fait de Plume FEx le programme d'aide de `ssh` : le nom du tuyau où poser la question.
pub const ASKPASS_ENV: &str = "PLUME_FEX_ASKPASS";

/// Un serveur : qui s'y connecte, où, et par quel port.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Server {
    pub user: String,
    pub host: String,
    pub port: u16,
}

impl Server {
    /// « utilisateur@hôte » ou « utilisateur@hôte:port ». Rien d'autre que des lettres, des chiffres et quelques
    /// signes : ce texte finit sur la ligne de commande de `sftp.exe`.
    pub fn parse(text: &str) -> Option<Server> {
        let (user, rest) = text.trim().split_once('@')?;
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, port.parse().ok()?),
            None => (rest, 22),
        };
        let plain = |s: &str, signs: &str| !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_alphanumeric() || signs.contains(c));
        (plain(user, "._-+") && plain(host, ".-") && port != 0).then(|| Server { user: user.to_string(), host: host.to_string(), port })
    }

    /// Le nom affiché et retenu : le port n'y figure que s'il n'est pas celui de tout le monde.
    pub fn label(&self) -> String {
        if self.port == 22 { format!("{}@{}", self.user, self.host) } else { format!("{}@{}:{}", self.user, self.host, self.port) }
    }
}

/// Un élément d'un dossier du serveur.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub dir: bool,
    /// Lien symbolique : on ne sait pas s'il mène à un dossier ou à un fichier.
    pub link: bool,
    pub size: u64,
    /// Date de modification en FILETIME (0 : inconnue). À la minute près pour les fichiers récents, au jour près
    /// pour les anciens : c'est tout ce que `sftp.exe` en dit.
    pub mtime: u64,
}

/// Ce que `ssh` demande pendant la connexion.
pub struct Question {
    pub text: String,
    /// La réponse est un secret (mot de passe, phrase d'une clé).
    pub secret: bool,
    /// La même question revient : la réponse précédente n'a pas convenu.
    pub again: bool,
}

/// Répond aux questions de `ssh` (None : l'utilisateur renonce).
pub type Ask = Box<dyn FnMut(Question) -> Option<String> + Send>;

const NO_WINDOW: u32 = 0x0800_0000;
/// De quoi pousser dehors un bloc de sortie de 4 Ko : `help` écrit environ 2 Ko.
const FILLER: &str = "help\nhelp\nhelp\n";
const SENTINEL: &str = "sftp> version";
const CLOSED: &str = "The connection to the server is closed.";
const QUICK: Duration = Duration::from_secs(90);
const LONG: Duration = Duration::from_secs(24 * 3600);

/// Le client SFTP de Windows (None : la fonction facultative « Client OpenSSH » n'est pas installée).
pub fn client() -> Option<PathBuf> {
    let exe = Path::new(&std::env::var_os("SystemRoot")?).join("System32").join("OpenSSH").join("sftp.exe");
    exe.is_file().then_some(exe)
}

/// Un chemin tel que `sftp.exe` le lit dans une commande. None : il contient ce qu'une ligne de commande ne peut
/// pas porter (guillemet, retour à la ligne) ou ce que ce programme transforme (barre oblique inverse).
fn quote(path: &str) -> Option<String> {
    (!path.is_empty() && !path.chars().any(|c| c == '"' || c == '\\' || c.is_control())).then(|| format!("\"{path}\""))
}

/// Un chemin de ce PC pour `sftp.exe` : avec des barres obliques.
fn local(path: &Path) -> Option<String> {
    quote(&path.display().to_string().replace('\\', "/"))
}

/// Le chemin d'un élément dans un dossier du serveur.
pub fn join(dir: &str, name: &str) -> String {
    format!("{}/{name}", dir.trim_end_matches('/'))
}

fn today() -> (u16, u16, u16) {
    let t = unsafe { GetLocalTime() };
    (t.wYear, t.wMonth, t.wDay)
}

/// Une date et une heure d'ici, en FILETIME (0 : date impossible).
fn filetime(year: u16, month: u16, day: u16, hour: u16, minute: u16) -> u64 {
    let local = SYSTEMTIME { wYear: year, wMonth: month, wDay: day, wHour: hour, wMinute: minute, ..Default::default() };
    let (mut utc, mut ft) = (SYSTEMTIME::default(), FILETIME::default());
    unsafe {
        if TzSpecificLocalTimeToSystemTime(None, &local, &mut utc).is_err() || SystemTimeToFileTime(&utc, &mut ft).is_err() {
            return 0;
        }
    }
    ((ft.dwHighDateTime as u64) << 32) | ft.dwLowDateTime as u64
}

/// Une ligne de « ls -ln » : droits, liens, propriétaire, groupe, taille, mois, jour, heure ou année, puis le nom.
/// La date n'a pas d'année quand le fichier a moins de six mois : c'est alors la plus récente qui ne soit pas future.
fn parse_line(line: &str, today: (u16, u16, u16)) -> Option<Entry> {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let mut rest = line;
    let mut fields = [""; 8];
    for f in &mut fields {
        rest = rest.trim_start_matches(' ');
        let end = rest.find(' ')?;
        (*f, rest) = rest.split_at(end);
    }
    // un seul espace sépare la date du nom (qui peut commencer par des espaces) ; le nom vient avec son dossier
    let name = rest.strip_prefix(' ')?.rsplit('/').next()?;
    let kind = fields[0].chars().next()?;
    if !matches!(kind, 'd' | 'l' | '-' | 'b' | 'c' | 'p' | 's') || name.is_empty() || name == "." || name == ".." {
        return None;
    }
    let size = fields[4].parse().ok()?;
    let month = MONTHS.iter().position(|m| *m == fields[5])? as u16 + 1;
    let day: u16 = fields[6].parse().ok()?;
    let mtime = match fields[7].split_once(':') {
        Some((h, m)) => {
            let future = (month, day) > (today.1, today.2);
            filetime(today.0 - future as u16, month, day, h.parse().ok()?, m.parse().ok()?)
        }
        None => filetime(fields[7].parse().ok()?, month, day, 0, 0),
    };
    Some(Entry { name: name.to_string(), dir: kind == 'd', link: kind == 'l', size, mtime })
}

/// Les lignes d'une sortie du programme, lues sur un fil à part.
fn lines(from: impl Read + Send + 'static) -> mpsc::Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut from = BufReader::new(from);
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match from.read_until(b'\n', &mut buf) {
                Ok(n) if n > 0 => {
                    let line = String::from_utf8_lossy(&buf);
                    if tx.send(line.trim_end_matches(['\r', '\n']).to_string()).is_err() {
                        break;
                    }
                }
                _ => break,
            }
        }
    });
    rx
}

/// Le message d'erreur de `sftp.exe`, sans ce qui n'apprend rien à l'utilisateur.
fn tidy(message: &str) -> String {
    let m = message.trim();
    m.strip_prefix("Can't ls: ").or_else(|| m.strip_prefix("remote ")).unwrap_or(m).to_string()
}

pub struct Session {
    child: Child,
    stdin: ChildStdin,
    out: mpsc::Receiver<String>,
    err: mpsc::Receiver<String>,
    /// Le dossier où le serveur nous place à la connexion.
    pub home: String,
    _asker: Option<Asker>,
}

impl Session {
    /// Se connecte. `ask` répond aux questions de `ssh` (mot de passe, phrase d'une clé).
    /// L'erreur est le message de `ssh` : serveur introuvable, accès refusé…
    pub fn open(server: &Server, ask: Ask) -> Result<Session, String> {
        let exe = client().ok_or("sftp.exe")?;
        let mut cmd = Command::new(exe);
        let asker = Asker::start(ask);
        if let (Some(asker), Ok(me)) = (&asker, std::env::current_exe()) {
            // sans console, `ssh` pose ses questions par le programme que désigne SSH_ASKPASS : nous
            cmd.env("SSH_ASKPASS", me).env("SSH_ASKPASS_REQUIRE", "force").env("DISPLAY", "plume").env(ASKPASS_ENV, &asker.name);
        }
        // un serveur jamais vu est retenu ; un serveur connu dont la clé a changé est refusé
        cmd.args(["-o", "StrictHostKeyChecking=accept-new", "-o", "ConnectTimeout=20", "-o", "ServerAliveInterval=30"]);
        cmd.args(["-P", &server.port.to_string(), "--", &format!("{}@{}", server.user, server.host)]);
        Session::start(cmd, asker)
    }

    /// Parle directement à un programme serveur de ce PC, sans réseau ni mot de passe : pour les essais.
    #[cfg(any(test, feature = "test-hook"))]
    pub fn open_direct(server_program: &str) -> Result<Session, String> {
        let mut cmd = Command::new(client().ok_or("sftp.exe")?);
        cmd.args(["-D", server_program]);
        Session::start(cmd, None)
    }

    fn start(mut cmd: Command, asker: Option<Asker>) -> Result<Session, String> {
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).creation_flags(NO_WINDOW);
        let mut child = cmd.spawn().map_err(|e| e.to_string())?;
        let (Some(stdin), Some(out), Some(err)) = (child.stdin.take(), child.stdout.take(), child.stderr.take()) else { return Err(CLOSED.into()) };
        let mut session = Session { child, stdin, out: lines(out), err: lines(err), home: String::new(), _asker: asker };
        // la première réponse n'arrive qu'une fois connecté : le temps du réseau, et celui de taper un mot de passe
        let (answer, _) = session.run("pwd", Duration::from_secs(600))?;
        session.home = answer.iter().find_map(|l| l.strip_prefix("Remote working directory: ")).unwrap_or("/").to_string();
        Ok(session)
    }

    /// Le programme tourne-t-il encore ?
    pub fn alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Ce que le programme a dit avant de s'arrêter : la raison d'une connexion manquée ou perdue.
    fn last_words(&mut self) -> String {
        std::thread::sleep(Duration::from_millis(60));
        let said: Vec<String> = self.err.try_iter().filter(|l| !l.trim().is_empty()).collect();
        // la dernière ligne est souvent « Connection closed » : la précédente dit pourquoi
        let useful = said.iter().rev().find(|l| !l.starts_with("Connection closed") && !l.starts_with("Couldn't read packet")).or(said.last());
        useful.map_or(CLOSED.to_string(), |l| l.trim().to_string())
    }

    /// Envoie une commande ; renvoie ce qu'elle a écrit, et ce que le programme a dit d'elle sur sa sortie d'erreur.
    fn run(&mut self, command: &str, patience: Duration) -> Result<(Vec<String>, Vec<String>), String> {
        while self.err.try_recv().is_ok() {}
        let mut send = |text: &str| self.stdin.write_all(text.as_bytes()).and_then(|_| self.stdin.flush());
        if send(&format!("{command}\nversion\n{FILLER}")).is_err() {
            return Err(self.last_words());
        }
        let echo = format!("sftp> {command}");
        let (mut answer, mut inside) = (vec![], false);
        let (started, mut nudges) = (Instant::now(), 0u32);
        loop {
            match self.out.recv_timeout(Duration::from_millis(100)) {
                Ok(line) if !inside => inside = line == echo,
                Ok(line) if line == SENTINEL => break,
                Ok(line) => answer.push(line),
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    if !self.alive() {
                        return Err(self.last_words());
                    }
                    if started.elapsed() > patience {
                        return Err("The server took too long to answer.".into());
                    }
                    // un bloc de sortie plus gros que prévu : encore un peu de texte pour le pousser
                    if started.elapsed() > Duration::from_secs(3 * (nudges as u64 + 1)) && nudges < 8 {
                        nudges += 1;
                        let _ = self.stdin.write_all(FILLER.as_bytes()).and_then(|_| self.stdin.flush());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => return Err(self.last_words()),
            }
        }
        // sa sortie d'erreur, elle, part tout de suite : ce qu'il a dit de la commande est déjà écrit. On laisse
        // juste au fil qui la lit le temps de nous le passer.
        let mut errors = vec![];
        while let Ok(line) = self.err.recv_timeout(Duration::from_millis(if errors.is_empty() { 20 } else { 2 })) {
            if !line.trim().is_empty() {
                errors.push(line);
            }
        }
        Ok((answer, errors))
    }

    /// Une commande qui ne doit rien dire sur la sortie d'erreur.
    fn act(&mut self, command: &str, patience: Duration) -> Result<(), String> {
        match self.run(command, patience)?.1.first() {
            Some(error) => Err(tidy(error)),
            None => Ok(()),
        }
    }

    /// Le contenu d'un dossier du serveur.
    pub fn list(&mut self, dir: &str) -> Result<Vec<Entry>, String> {
        // la barre finale fait entrer dans un lien qui mène à un dossier
        let path = quote(&format!("{}/", dir.trim_end_matches('/'))).ok_or(BAD_NAME)?;
        let (answer, errors) = self.run(&format!("ls -lna {path}"), QUICK)?;
        let today = today();
        let entries: Vec<Entry> = answer.iter().filter_map(|l| parse_line(l, today)).collect();
        match errors.first() {
            Some(error) if entries.is_empty() => Err(tidy(error)),
            _ => Ok(entries),
        }
    }

    pub fn mkdir(&mut self, path: &str) -> Result<(), String> {
        self.act(&format!("mkdir {}", quote(path).ok_or(BAD_NAME)?), QUICK)
    }

    pub fn rename(&mut self, from: &str, to: &str) -> Result<(), String> {
        self.act(&format!("rename {} {}", quote(from).ok_or(BAD_NAME)?, quote(to).ok_or(BAD_NAME)?), QUICK)
    }

    /// Supprime un fichier, ou un dossier avec tout ce qu'il contient. Rien ne se récupère.
    pub fn remove(&mut self, path: &str, dir: bool) -> Result<(), String> {
        self.remove_at(path, dir, 0)
    }

    fn remove_at(&mut self, path: &str, dir: bool, depth: u32) -> Result<(), String> {
        let quoted = quote(path).ok_or(BAD_NAME)?;
        if !dir {
            return self.act(&format!("rm {quoted}"), QUICK);
        }
        if depth > 64 {
            return Err("Too many folders inside one another.".into());
        }
        // `sftp.exe` ne sait retirer qu'un dossier vide : on le vide d'abord (un lien est retiré, jamais suivi)
        for e in self.list(path)? {
            self.remove_at(&join(path, &e.name), e.dir, depth + 1)?;
        }
        self.act(&format!("rmdir {quoted}"), QUICK)
    }

    /// Télécharge un fichier sous le nom `to`, ou un dossier (avec son contenu) dans le dossier `to`, qui existe.
    pub fn get(&mut self, from: &str, to: &Path, dir: bool) -> Result<(), String> {
        let (from, to) = (quote(from).ok_or(BAD_NAME)?, local(to).ok_or(BAD_NAME)?);
        self.act(&format!("get {}{from} {to}", if dir { "-r " } else { "" }), LONG)
    }

    /// Envoie un fichier sous le chemin `to`, ou un dossier (avec son contenu) dans le dossier `to`, qui existe.
    pub fn put(&mut self, from: &Path, to: &str, dir: bool) -> Result<(), String> {
        let (from, to) = (local(from).ok_or(BAD_NAME)?, quote(to).ok_or(BAD_NAME)?);
        self.act(&format!("put {}{from} {to}", if dir { "-r " } else { "" }), LONG)
    }
}

/// Message quand un nom ne peut pas être écrit dans une commande.
pub const BAD_NAME: &str = "This name has a character that cannot be sent to the server from here (\" or \\).";

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.stdin.write_all(b"bye\n").and_then(|_| self.stdin.flush());
        // le temps de dire au revoir au serveur, puis on n'attend plus
        for _ in 0..20 {
            if !self.alive() {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ───────────────────────── les questions de ssh ─────────────────────────

/// Le tuyau nommé où le programme d'aide dépose les questions de `ssh`, et le fil qui y répond.
struct Asker {
    name: String,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Asker {
    fn start(mut ask: Ask) -> Option<Asker> {
        // un nom que personne ne peut deviner d'avance
        let nonce = std::collections::hash_map::RandomState::new().build_hasher().finish();
        let name = format!(r"\\.\pipe\plume-fex-ask-{}-{nonce:016x}", std::process::id());
        let stop = Arc::new(AtomicBool::new(false));
        let (pipe, stopping) = (name.clone(), stop.clone());
        let first = open_pipe(&pipe)?;
        let thread = std::thread::spawn(move || {
            let (mut next, mut last, mut repeats) = (Some(first), String::new(), 0u32);
            while let Some(mut file) = next.take().or_else(|| open_pipe(&pipe)) {
                let connected =
                    unsafe { ConnectNamedPipe(windows::Win32::Foundation::HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(&file) as _), None) };
                // 535 : le programme d'aide était déjà là avant qu'on l'attende
                let connected = connected.is_ok() || connected.is_err_and(|e| e.code().0 as u32 & 0xffff == 535);
                if stopping.load(Ordering::Relaxed) {
                    return;
                }
                if !connected {
                    continue;
                }
                // la question : un octet (s : la réponse est un secret), la longueur du texte, le texte
                let mut head = [0u8; 5];
                if file.read_exact(&mut head).is_err() {
                    continue;
                }
                let mut text = vec![0u8; (u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize).min(8192)];
                if file.read_exact(&mut text).is_err() {
                    continue;
                }
                let text = String::from_utf8_lossy(&text).into_owned();
                repeats = if text == last { repeats + 1 } else { 0 };
                last = text.clone();
                let answer = ask(Question { text, secret: head[0] == b's', again: repeats > 0 });
                // la réponse : un octet (1 : il y en a une), sa longueur, son texte
                let mut reply = vec![answer.is_some() as u8];
                let answer = answer.unwrap_or_default();
                reply.extend((answer.len() as u32).to_le_bytes());
                reply.extend(answer.as_bytes());
                let _ = file.write_all(&reply).and_then(|_| file.flush());
                // le programme d'aide lit, puis ferme son côté : on attend cela avant de fermer le nôtre
                let _ = file.read(&mut head);
            }
        });
        Some(Asker { name, stop, thread: Some(thread) })
    }
}

/// Ouvre notre côté du tuyau. Seul ce PC peut s'y brancher.
fn open_pipe(name: &str) -> Option<File> {
    let wide: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    let pipe = unsafe {
        CreateNamedPipeW(
            PCWSTR(wide.as_ptr()),
            PIPE_ACCESS_DUPLEX,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            1,
            4096,
            4096,
            0,
            None,
        )
    };
    (!pipe.is_invalid()).then(|| unsafe { File::from_raw_handle(pipe.0 as _) })
}

impl Drop for Asker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let Some(thread) = self.thread.take() else { return };
        // Le fil attend qu'on se branche sur le tuyau : on le fait nous-mêmes, pour qu'il voie qu'il faut s'arrêter.
        // Entre deux questions le tuyau n'existe pas un instant : on insiste jusqu'à ce que le fil ait fini.
        while !thread.is_finished() {
            let _ = OpenOptions::new().read(true).write(true).open(&self.name);
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = thread.join();
    }
}

/// Se branche sur le tuyau des questions. Entre deux questions, il n'existe pas pendant un instant : on insiste.
fn connect(pipe: &str) -> Option<File> {
    (0..100).find_map(|attempt| {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(20));
        }
        OpenOptions::new().read(true).write(true).open(pipe).ok()
    })
}

/// Le rôle de Plume FEx quand `ssh` le lance pour poser une question (mot de passe, phrase d'une clé) : il la
/// porte à la fenêtre par le tuyau `pipe`, et écrit la réponse sur sa sortie, où `ssh` la lit.
/// Renvoie le code de sortie du programme (autre que 0 : pas de réponse).
pub fn askpass(pipe: &str) -> i32 {
    let question = std::env::args().nth(1).unwrap_or_default();
    // SSH_ASKPASS_PROMPT=confirm ou none : une question à laquelle on répond en clair
    let kind = match std::env::var("SSH_ASKPASS_PROMPT").as_deref() {
        Ok("confirm") | Ok("none") => b'c',
        _ => b's',
    };
    let Some(mut file) = connect(pipe) else { return 1 };
    let mut message = vec![kind];
    message.extend((question.len() as u32).to_le_bytes());
    message.extend(question.as_bytes());
    let mut head = [0u8; 5];
    if file.write_all(&message).is_err() || file.read_exact(&mut head).is_err() || head[0] != 1 {
        return 1;
    }
    let mut answer = vec![0u8; (u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize).min(8192)];
    if file.read_exact(&mut answer).is_err() {
        return 1;
    }
    let mut out = std::io::stdout();
    let written = out.write_all(&answer).and_then(|_| out.write_all(b"\n")).and_then(|_| out.flush());
    written.is_err() as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Le serveur SFTP livré avec Git pour Windows, s'il est là : un vrai serveur, sans réseau ni mot de passe.
    fn test_server() -> Option<String> {
        let exe = r"C:\PROGRA~1\Git\usr\lib\ssh\sftp-server.exe";
        (Path::new(exe).is_file() && client().is_some()).then(|| exe.to_string())
    }

    /// Un dossier d'essai, et son chemin vu du serveur (« /c/Users/... »).
    fn scratch(tag: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("fex-sftp-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let text = dir.display().to_string().replace('\\', "/");
        let seen = format!("/{}{}", text[..1].to_lowercase(), &text[2..]);
        (dir, seen)
    }

    #[test]
    fn addresses() {
        assert_eq!(Server::parse("sam@example.com"), Some(Server { user: "sam".into(), host: "example.com".into(), port: 22 }));
        assert_eq!(Server::parse(" root@10.0.0.5:2222 ").map(|s| s.label()), Some("root@10.0.0.5:2222".into()));
        assert_eq!(Server::parse("sam@host").map(|s| s.label()), Some("sam@host".into()));
        // rien qui puisse passer pour une option ou une autre commande
        for bad in [
            "",
            "host",
            "@host",
            "sam@",
            "sam@-oProxyCommand=calc",
            "-l@host",
            "sam@host:0",
            "sam@host:port",
            "sam@ho st",
            "sa m@host",
            "sam@host;calc",
            "sam@host\"x",
        ] {
            assert_eq!(Server::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn listing_lines() {
        let today = (2026, 11, 10);
        let e = parse_line("-rw-******    ? 1001     1001        16660 Nov 10 05:10 /home/sam/docs/CHANGELOG.fr.md", today).unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.link, e.size), ("CHANGELOG.fr.md", false, false, 16660));
        assert_eq!(e.mtime, filetime(2026, 11, 10, 5, 10));
        let e = parse_line("drwxr-xr-x    2 0        0            4096 Dec 25 23:59 /srv/Sub folder", today).unwrap();
        // « 25 décembre » sans année, vu un 10 novembre : c'était l'an dernier
        assert_eq!((e.name.as_str(), e.dir, e.mtime), ("Sub folder", true, filetime(2025, 12, 25, 23, 59)));
        let e = parse_line("lrwxrwxrwx    1 0        0               7 Jan  5  2019 /bin", today).unwrap();
        assert_eq!((e.name.as_str(), e.dir, e.link, e.mtime), ("bin", false, true, filetime(2019, 1, 5, 0, 0)));
        let e = parse_line("-rw-r--r--    1 1000     1000            0 Nov  9 12:00 /x/ deux  espaces.txt", today).unwrap();
        assert_eq!(e.name, " deux  espaces.txt");
        for skipped in [
            "drwx******    ? 1001     1001            0 Nov 10 10:49 /x/.",
            "drwx******    ? 1001     1001            0 Nov 10 10:49 /x/..",
            "sftp> ls",
            "",
            "total 12",
        ] {
            assert_eq!(parse_line(skipped, today), None, "{skipped}");
        }
        assert_eq!(quote("/a b/c[1] é.txt").as_deref(), Some("\"/a b/c[1] é.txt\""));
        assert_eq!((quote("a\"b"), quote("a\\b"), quote("a\nb"), quote("")), (None, None, None, None));
        assert_eq!((join("/", "a"), join("/a/", "b c"), join("/a", "b")), ("/a".to_string(), "/a/b c".to_string(), "/a/b".to_string()));
    }

    #[test]
    fn a_real_session() {
        let Some(server) = test_server() else { return };
        let (dir, seen) = scratch("session");
        std::fs::create_dir_all(dir.join("remote").join("Sub folder")).unwrap();
        std::fs::create_dir_all(dir.join("local")).unwrap();
        std::fs::write(dir.join("remote").join("hello.txt"), "hello").unwrap();
        std::fs::write(dir.join("remote").join("we[i]rd été 'x' #1.txt"), "odd").unwrap();
        std::fs::write(dir.join("remote").join("Sub folder").join("inner file.txt"), "inner").unwrap();
        std::fs::write(dir.join("local").join("up load.txt"), "sent").unwrap();
        let remote = format!("{seen}/remote");
        let mut s = Session::open_direct(&server).unwrap();
        assert!(s.home.starts_with('/'), "{}", s.home);
        let names = |s: &mut Session, d: &str| -> Vec<(String, bool, u64)> {
            let mut v: Vec<_> = s.list(d).unwrap().into_iter().map(|e| (e.name, e.dir, e.size)).collect();
            v.sort();
            v
        };
        assert_eq!(names(&mut s, &remote), [("Sub folder".into(), true, 0), ("hello.txt".into(), false, 5), ("we[i]rd été 'x' #1.txt".into(), false, 3)]);
        assert!(s.list(&remote).unwrap().iter().all(|e| e.mtime > 0));
        assert!(s.list(&format!("{remote}/nope")).is_err());
        // dossier, renommage, envoi, téléchargement : avec des espaces, des crochets, des accents
        s.mkdir(&format!("{remote}/New folder")).unwrap();
        assert!(s.mkdir(&format!("{remote}/New folder")).is_err());
        s.rename(&format!("{remote}/New folder"), &format!("{remote}/Dossier é [2]")).unwrap();
        s.put(&dir.join("local").join("up load.txt"), &format!("{remote}/Dossier é [2]/up load.txt"), false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("remote").join("Dossier é [2]").join("up load.txt")).unwrap(), "sent");
        s.get(&format!("{remote}/we[i]rd été 'x' #1.txt"), &dir.join("local").join("copie.txt"), false).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("local").join("copie.txt")).unwrap(), "odd");
        assert!(s.get(&format!("{remote}/nope.txt"), &dir.join("local").join("x.txt"), false).is_err());
        // dossiers entiers, dans un sens et dans l'autre
        s.get(&format!("{remote}/Sub folder"), &dir.join("local"), true).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("local").join("Sub folder").join("inner file.txt")).unwrap(), "inner");
        s.put(&dir.join("local").join("Sub folder"), &format!("{remote}/Dossier é [2]"), true).unwrap();
        assert!(dir.join("remote").join("Dossier é [2]").join("Sub folder").join("inner file.txt").is_file());
        // suppression : un fichier, puis un dossier avec tout ce qu'il contient
        s.remove(&format!("{remote}/hello.txt"), false).unwrap();
        assert!(s.remove(&format!("{remote}/hello.txt"), false).is_err());
        s.remove(&format!("{remote}/Dossier é [2]"), true).unwrap();
        assert_eq!(names(&mut s, &remote), [("Sub folder".into(), true, 0), ("we[i]rd été 'x' #1.txt".into(), false, 3)]);
        assert!(s.alive());
        drop(s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn questions_travel_through_the_pipe() {
        // la fenêtre répond « s3cret » à la première question, puis renonce
        let (seen_tx, seen_rx) = mpsc::channel();
        let mut count = 0;
        let asker = Asker::start(Box::new(move |q: Question| {
            count += 1;
            let _ = seen_tx.send((q.text.clone(), q.secret, q.again));
            (count == 1).then(|| "s3cret é".to_string())
        }))
        .unwrap();
        // ce que fait le programme d'aide, sans passer par un autre programme
        let ask = |kind: u8, text: &str| -> Option<String> {
            let mut file = connect(&asker.name).unwrap();
            let mut message = vec![kind];
            message.extend((text.len() as u32).to_le_bytes());
            message.extend(text.as_bytes());
            file.write_all(&message).unwrap();
            let mut head = [0u8; 5];
            file.read_exact(&mut head).unwrap();
            let mut answer = vec![0u8; u32::from_le_bytes([head[1], head[2], head[3], head[4]]) as usize];
            file.read_exact(&mut answer).unwrap();
            (head[0] == 1).then(|| String::from_utf8(answer).unwrap())
        };
        assert_eq!(ask(b's', "sam@host's password: ").as_deref(), Some("s3cret é"));
        assert_eq!(seen_rx.recv().unwrap(), ("sam@host's password: ".to_string(), true, false));
        // le tuyau se rouvre pour la question suivante ; la même question qui revient est signalée
        assert_eq!(ask(b's', "sam@host's password: "), None);
        assert_eq!(seen_rx.recv().unwrap(), ("sam@host's password: ".to_string(), true, true));
        drop(asker);
    }
}
