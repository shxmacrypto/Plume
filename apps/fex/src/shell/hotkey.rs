//! Win+E ouvre Plume FEx. Depuis Windows 11 25H2, l'Explorateur garde le raccourci pour lui (la commande de Win+E
//! inscrite dans le registre n'est plus consultée) : quand Plume FEx est l'explorateur par défaut, un petit veilleur
//! (`plume-fex.exe --hotkey`, lancé avec Windows) attrape Win+E au clavier et ouvre Plume FEx.
//!
//! - Rien n'est changé dans l'Explorateur : le veilleur arrêté (option décochée, Plume FEx retiré), Win+E redevient
//!   celui de Windows aussitôt, sans redémarrage.
//! - Le veilleur n'a pas de fenêtre visible ni d'interface : une fenêtre de messages, un crochet clavier, un minuteur
//!   qui vérifie toutes les 30 s que Plume FEx est toujours l'explorateur par défaut (sinon il s'arrête).
//! - Pour que la touche Windows relâchée n'ouvre pas le menu Démarrer (Windows croit qu'elle a été pressée seule),
//!   une touche sans fonction (0xE8) est envoyée juste avant d'avaler le E.

use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use windows::{
    Win32::{
        Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND, LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::CreateMutexW},
        UI::{
            Input::KeyboardAndMouse::{
                GetAsyncKeyState, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LWIN,
                VK_MENU, VK_RWIN, VK_SHIFT,
            },
            WindowsAndMessaging::{
                CallNextHookEx, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowExW, GetMessageW, HC_ACTION, HWND_MESSAGE,
                IsWindow, KBDLLHOOKSTRUCT, LLKHF_INJECTED, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SetTimer, SetWindowsHookExW, TranslateMessage,
                UnhookWindowsHookEx, WH_KEYBOARD_LL, WINDOW_EX_STYLE, WINDOW_STYLE, WM_APP, WM_CLOSE, WM_DESTROY, WM_KEYDOWN, WM_SYSKEYDOWN, WM_TIMER,
                WNDCLASSW,
            },
        },
    },
    core::{PCWSTR, w},
};

/// La classe de la fenêtre de messages du veilleur (pour le retrouver et l'arrêter).
const CLASS: PCWSTR = w!("PlumeFExWinE");
const VK_E: u32 = 0x45;
/// Une touche sans fonction : la touche Windows a « servi », le menu Démarrer ne s'ouvre pas à son relâchement.
const MASK: u16 = 0xE8;
const WM_OPEN: u32 = WM_APP + 1;
const CHECK: usize = 1;

static WINDOW: AtomicIsize = AtomicIsize::new(0);
/// Le E de Win+E est encore enfoncé : ses répétitions et son relâchement sont avalés aussi.
static HELD: AtomicBool = AtomicBool::new(false);

fn pressed(vk: VIRTUAL_KEY) -> bool {
    unsafe { (GetAsyncKeyState(vk.0 as i32) as u16) & 0x8000 != 0 }
}

fn mask_key() {
    let key = |flags: KEYBD_EVENT_FLAGS| INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 { ki: KEYBDINPUT { wVk: VIRTUAL_KEY(MASK), wScan: 0, dwFlags: flags, time: 0, dwExtraInfo: 0 } },
    };
    let inputs = [key(KEYBD_EVENT_FLAGS(0)), key(KEYEVENTF_KEYUP)];
    unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) };
}

unsafe extern "system" fn keyboard(code: i32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        let k = unsafe { &*(lp.0 as *const KBDLLHOOKSTRUCT) };
        if k.vkCode == VK_E && k.flags.0 & LLKHF_INJECTED.0 == 0 {
            let down = matches!(wp.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN);
            if down && HELD.load(Ordering::SeqCst) {
                return LRESULT(1); // répétition : déjà ouvert
            }
            if down && (pressed(VK_LWIN) || pressed(VK_RWIN)) && !pressed(VK_CONTROL) && !pressed(VK_MENU) && !pressed(VK_SHIFT) {
                mask_key();
                HELD.store(true, Ordering::SeqCst);
                // rien de lent ici (Windows retire un crochet trop lent) : la fenêtre ouvre Plume FEx
                let hwnd = HWND(WINDOW.load(Ordering::SeqCst) as *mut _);
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_OPEN, WPARAM(0), LPARAM(0));
                }
                return LRESULT(1);
            }
            if !down && HELD.swap(false, Ordering::SeqCst) {
                return LRESULT(1);
            }
        }
    }
    unsafe { CallNextHookEx(None, code, wp, lp) }
}

/// Plume FEx est-il toujours l'explorateur par défaut ? (Sinon le veilleur n'a plus de raison d'être.)
fn still_default() -> bool {
    match (super::integration::Classes::user(), std::env::current_exe()) {
        (Some(classes), Ok(exe)) => classes.is_default(&exe),
        // essais : pas de registre réel, le veilleur reste jusqu'à ce qu'on le ferme
        _ => true,
    }
}

unsafe extern "system" fn proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_OPEN => {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
            LRESULT(0)
        }
        WM_TIMER if wp.0 == CHECK => {
            if !still_default() {
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

/// `plume-fex.exe --hotkey` : le veilleur. Un seul à la fois ; rend 0 en s'arrêtant.
pub fn run() -> i32 {
    unsafe {
        let _mutex = CreateMutexW(None, true, w!("Local\\PlumeFExWinE"));
        if GetLastError() == ERROR_ALREADY_EXISTS {
            return 0;
        }
        let Ok(module) = GetModuleHandleW(None) else { return 1 };
        let wc = WNDCLASSW { lpfnWndProc: Some(proc), hInstance: module.into(), lpszClassName: CLASS, ..Default::default() };
        RegisterClassW(&wc);
        let Ok(hwnd) = CreateWindowExW(WINDOW_EX_STYLE(0), CLASS, w!(""), WINDOW_STYLE(0), 0, 0, 0, 0, Some(HWND_MESSAGE), None, Some(module.into()), None)
        else {
            return 1;
        };
        WINDOW.store(hwnd.0 as isize, Ordering::SeqCst);
        // essais : PLUME_FEX_NO_HOOK garde le veilleur sans toucher au clavier
        let hook =
            if std::env::var_os("PLUME_FEX_NO_HOOK").is_none() { SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard), Some(module.into()), 0).ok() } else { None };
        SetTimer(Some(hwnd), CHECK, 30_000, None);
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        if let Some(h) = hook {
            let _ = UnhookWindowsHookEx(h);
        }
    }
    0
}

/// Le veilleur qui tourne, s'il y en a un.
fn running() -> Option<HWND> {
    unsafe { FindWindowExW(Some(HWND_MESSAGE), None, CLASS, PCWSTR::null()).ok().filter(|h| !h.0.is_null()) }
}

/// Lance le veilleur (s'il ne tourne pas déjà).
pub fn start(exe: &std::path::Path) {
    if running().is_none() {
        let _ = std::process::Command::new(exe).arg("--hotkey").spawn();
    }
}

/// Arrête le veilleur et attend qu'il soit parti (au plus 3 s), pour que son programme puisse être effacé.
pub fn stop() {
    let Some(hwnd) = running() else { return };
    unsafe {
        let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
    }
    for _ in 0..30 {
        if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    // le processus sort juste après sa fenêtre
    std::thread::sleep(std::time::Duration::from_millis(300));
}
