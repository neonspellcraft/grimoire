//! Native right-click menus (NSMenu on macOS, a Win32 popup menu on Windows) via muda.
//!
//! Showing a context menu is modal on both platforms: `popup` returns once the menu closes, with the chosen
//! command, if any.

use muda::{CheckMenuItem, ContextMenu, Menu, MenuEvent, MenuId, MenuItem, PredefinedMenuItem};
use winit::window::Window;

/// One line of a context menu. `T` is whatever the caller wants back when it's chosen.
pub enum Entry<T> {
    Item(String, T),
    Check(String, bool, T),
    /// A greyed-out label that heads a group.
    Header(String),
    Separator,
}

/// Shows `entries` at the mouse pointer over `window` and returns the chosen command.
pub fn popup<T>(window: &Window, entries: Vec<Entry<T>>) -> Option<T> {
    let menu = Menu::new();
    let mut commands: Vec<(MenuId, T)> = Vec::new();
    for entry in entries {
        let appended = match entry {
            Entry::Item(label, command) => {
                let item = MenuItem::new(label, true, None);
                commands.push((item.id().clone(), command));
                menu.append(&item)
            }
            Entry::Check(label, checked, command) => {
                let item = CheckMenuItem::new(label, true, checked, None);
                commands.push((item.id().clone(), command));
                menu.append(&item)
            }
            Entry::Header(label) => menu.append(&MenuItem::new(label, false, None)),
            Entry::Separator => menu.append(&PredefinedMenuItem::separator()),
        };
        if let Err(e) = appended {
            eprintln!("grimoire: couldn't build a menu: {e}");
            return None;
        }
    }
    // Clear anything left over from earlier menus, show this one, then see what was picked.
    while MenuEvent::receiver().try_recv().is_ok() {}
    if !show(&menu, window) {
        return None;
    }
    let chosen = MenuEvent::receiver().try_recv().ok()?.id;
    commands.into_iter().find(|(id, _)| *id == chosen).map(|(_, command)| command)
}

#[cfg(target_os = "macos")]
fn show(menu: &Menu, window: &Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else { return false };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else { return false };
    // SAFETY: the view belongs to a live winit window on the main thread; None shows it at the pointer.
    unsafe { menu.show_context_menu_for_nsview(handle.ns_view.as_ptr() as *const _, None) }
}

#[cfg(target_os = "windows")]
fn show(menu: &Menu, window: &Window) -> bool {
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = window.window_handle() else { return false };
    let RawWindowHandle::Win32(handle) = handle.as_raw() else { return false };
    // SAFETY: the HWND belongs to a live winit window on this thread; None shows it at the pointer.
    unsafe { menu.show_context_menu_for_hwnd(handle.hwnd.get(), None) }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn show(_: &Menu, _: &Window) -> bool {
    false
}

/// Reveals a file in Finder or Explorer.
pub fn reveal(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let result = std::process::Command::new("open").arg("-R").arg(path).spawn();
    #[cfg(target_os = "windows")]
    let result = std::process::Command::new("explorer").arg(format!("/select,{}", path.display())).spawn();
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    let result = std::process::Command::new("xdg-open").arg(path.parent().unwrap_or(path)).spawn();
    if let Err(e) = result {
        eprintln!("grimoire: couldn't show {}: {e}", path.display());
    }
}
