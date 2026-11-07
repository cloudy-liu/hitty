//! Compose window attention and deliver shortcut-backed taskbar reminders to Explorer.
use super::{TraySnapshot, icon};
use crate::core::config::AttentionBadgeSize;
use anyhow::Context as _;
use gpui::App;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::collections::HashMap;
use windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
    UI::{
        Shell::{ITaskbarList3, TaskbarList},
        WindowsAndMessaging::{
            CreateIconFromResourceEx, DestroyIcon, GCLP_HICON, GCLP_HICONSM, GetClassLongPtrW,
            HICON, ICON_BIG, ICON_SMALL, IsWindow, LR_DEFAULTCOLOR, SendMessageW, WM_GETICON,
            WM_SETICON,
        },
    },
};
use windows::core::w;

const KINDS: [u32; 2] = [ICON_SMALL, ICON_BIG];

struct Icon(HICON);

impl Icon {
    fn new(size: u32, badge_size: AttentionBadgeSize) -> anyhow::Result<Self> {
        let image =
            icon::render_taskbar(size, badge_size).context("could not render attention icon")?;
        Self::from_image(image)
    }

    fn from_image(image: icon::RgbaImage) -> anyhow::Result<Self> {
        let size = image.width;
        let mut png = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_raw(image.width, image.height, image.data)
            .context("invalid attention icon")?
            .write_to(&mut png, image::ImageFormat::Png)?;
        // Windows accepts PNG-compressed icon resources. The input buffer only
        // needs to live for this call; the returned HICON owns its pixels.
        let icon = unsafe {
            CreateIconFromResourceEx(
                png.get_ref(),
                true,
                0x30000,
                size as i32,
                size as i32,
                LR_DEFAULTCOLOR,
            )?
        };
        Ok(Self(icon))
    }
}

impl Drop for Icon {
    fn drop(&mut self) {
        let _ = unsafe { DestroyIcon(self.0) };
    }
}

/// Explorer's overlay channel with the standalone mark it will place on the
/// installed identity's taskbar button. GPUI initializes COM on its Windows
/// UI thread; callers elsewhere must have initialized it themselves.
fn open_overlay(badge_size: AttentionBadgeSize) -> anyhow::Result<(ITaskbarList3, Icon)> {
    let list: ITaskbarList3 =
        unsafe { CoCreateInstance(&TaskbarList, None, CLSCTX_INPROC_SERVER)? };
    unsafe { list.HrInit()? };
    let image = icon::attention_mark(32, badge_size).context("could not render taskbar overlay")?;
    Ok((list, Icon::from_image(image)?))
}

pub(super) struct Taskbar {
    badge_size: AttentionBadgeSize,
    icon: Icon,
    overlay: Option<(ITaskbarList3, Icon)>,
    originals: HashMap<isize, [HICON; 2]>,
}

impl Taskbar {
    fn new(badge_size: AttentionBadgeSize) -> anyhow::Result<Self> {
        // Explorer uses the registered shortcut's icon for the installed
        // AppUserModelID, ignoring WM_SETICON on that grouped taskbar button,
        // so the installed identity delivers the mark through the shell's
        // overlay. Cargo builds, and installs whose overlay channel fails,
        // compose it into the window artwork instead. Never both: Explorer
        // honors the window artwork whenever it does not substitute a
        // shortcut's icon, and the button then shows two marks.
        let overlay = if std::env::current_exe()
            .is_ok_and(|exe| crate::core::aumid::taskbar_app_id(&exe) == crate::core::aumid::AUMID)
        {
            open_overlay(badge_size)
                .map_err(|error| log::debug!("taskbar overlay unavailable: {error:#}"))
                .ok()
        } else {
            None
        };
        Ok(Self {
            badge_size,
            // Explorer may ignore a small bitmap for ICON_SMALL at high DPI.
            // Let Windows scale the full-resolution artwork for both slots.
            icon: Icon::new(256, badge_size)?,
            overlay,
            originals: HashMap::new(),
        })
    }

    fn apply(&mut self, hwnd: HWND, attention: bool) {
        if let Some((list, icon)) = &self.overlay {
            let mark = if attention { icon.0 } else { HICON::default() };
            // Reapply even when nothing changed, including after Explorer has
            // recreated the group's taskbar button.
            if let Err(error) = unsafe { list.SetOverlayIcon(hwnd, mark, w!("Agent attention")) } {
                log::debug!("could not update taskbar attention overlay: {error}");
            }
            // The overlay carries the mark; the window icon stays plain.
            return;
        }
        if attention {
            let current = KINDS.map(|kind| unsafe {
                let icon = SendMessageW(hwnd, WM_GETICON, WPARAM(kind as usize), LPARAM(0)).0;
                if icon != 0 {
                    return HICON(icon as *mut _);
                }
                // Null selects a class icon, but sending null later does not
                // refresh Explorer. Preserve the borrowed class handle itself.
                let index = if kind == ICON_SMALL {
                    GCLP_HICONSM
                } else {
                    GCLP_HICON
                };
                let icon = GetClassLongPtrW(hwnd, index);
                HICON(if icon == 0 {
                    GetClassLongPtrW(hwnd, GCLP_HICON)
                } else {
                    icon
                } as *mut _)
            });
            if current == [self.icon.0; 2] {
                return;
            }
            // Preserve GPUI's effective icons, including its class fallback.
            // Repeated polls must never save our own artwork as the original.
            let original = self.originals.entry(hwnd.0 as isize).or_insert(current);
            for (original, current) in original.iter_mut().zip(current) {
                if current != self.icon.0 {
                    *original = current;
                }
            }
            for kind in KINDS {
                unsafe {
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        WPARAM(kind as usize),
                        LPARAM(self.icon.0.0 as isize),
                    );
                }
            }
        } else if let Some(original) = self.originals.remove(&(hwnd.0 as isize)) {
            self.restore(hwnd, original);
        }
    }

    fn restore(&self, hwnd: HWND, original: [HICON; 2]) {
        for (kind, original) in KINDS.into_iter().zip(original) {
            unsafe {
                // A closed/reused HWND or another icon owner must be left alone.
                if SendMessageW(hwnd, WM_GETICON, WPARAM(kind as usize), LPARAM(0)).0
                    == self.icon.0.0 as isize
                {
                    SendMessageW(
                        hwnd,
                        WM_SETICON,
                        WPARAM(kind as usize),
                        LPARAM(original.0 as isize),
                    );
                }
            }
        }
    }
}

impl Drop for Taskbar {
    fn drop(&mut self) {
        // WM_SETICON borrows the HICON. Detach it before freeing its pixels.
        for (&hwnd, &original) in &self.originals {
            if let Some((list, _)) = &self.overlay {
                let _ =
                    unsafe { list.SetOverlayIcon(HWND(hwnd as *mut _), HICON::default(), w!("")) };
            }
            self.restore(HWND(hwnd as *mut _), original);
        }
    }
}

pub(super) fn update(state: &mut Option<Taskbar>, snap: &TraySnapshot, cx: &mut App) {
    if state
        .as_ref()
        .is_some_and(|taskbar| taskbar.badge_size != snap.attention_badge_size)
    {
        // Restore borrowed window icons before retiring the old bitmap handles.
        *state = None;
    }
    if state.is_none() {
        match Taskbar::new(snap.attention_badge_size) {
            Ok(taskbar) => *state = Some(taskbar),
            Err(error) => {
                log::debug!("taskbar attention unavailable: {error:#}");
                return;
            }
        }
    }
    let taskbar = state.as_mut().unwrap();
    taskbar
        .originals
        .retain(|&hwnd, _| unsafe { IsWindow(HWND(hwnd as *mut _)).as_bool() });
    for (workspace, _) in crate::ui::windows::WindowRegistry::open_windows(cx) {
        let Some(handle) = crate::ui::windows::WindowRegistry::window_for(cx, workspace) else {
            continue;
        };
        let _ = handle.update(cx, |_, window, _| {
            let Ok(handle) = HasWindowHandle::window_handle(window) else {
                return;
            };
            let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                return;
            };
            // Every window in the group carries the same aggregate reminder.
            // Explorer can recreate its buttons directly from these icons.
            taskbar.apply(HWND(handle.hwnd.get() as *mut _), snap.attention());
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::{
        Win32::{
            Foundation::{LPARAM, WPARAM},
            System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx},
            UI::WindowsAndMessaging::{
                CreateWindowExW, DefWindowProcW, DestroyWindow, GCLP_HICON, GCLP_HICONSM,
                GetClassLongPtrW, ICON_BIG, ICON_SMALL, IDI_APPLICATION, LoadIconW, RegisterClassW,
                SendMessageW, UnregisterClassW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_GETICON,
                WM_SETICON, WNDCLASSW,
            },
        },
        core::w,
    };

    struct TestWindow(HWND);

    impl TestWindow {
        fn new() -> Self {
            Self::for_class(w!("STATIC"))
        }

        fn for_class(class: windows::core::PCWSTR) -> Self {
            Self(unsafe {
                CreateWindowExW(
                    WINDOW_EX_STYLE::default(),
                    class,
                    w!("attention test"),
                    WINDOW_STYLE::default(),
                    0,
                    0,
                    32,
                    32,
                    None,
                    None,
                    None,
                    None,
                )
                .unwrap()
            })
        }

        fn icons(&self) -> [isize; 2] {
            [ICON_SMALL, ICON_BIG].map(|kind| unsafe {
                SendMessageW(self.0, WM_GETICON, WPARAM(kind as usize), LPARAM(0)).0
            })
        }
    }

    #[test]
    fn clearing_attention_supplies_the_class_icon_to_explorer() {
        unsafe extern "system" fn window_proc(
            hwnd: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        let class = w!("ctty7-taskbar-class-icon-test");
        let icon = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap() };
        assert_ne!(
            unsafe {
                RegisterClassW(&WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hIcon: icon,
                    lpszClassName: class,
                    ..Default::default()
                })
            },
            0
        );
        {
            let window = TestWindow::for_class(class);
            assert_eq!(
                window.icons(),
                [0, 0],
                "class icons are not window overrides"
            );
            let expected = [GCLP_HICONSM, GCLP_HICON]
                .map(|kind| unsafe { GetClassLongPtrW(window.0, kind) as isize });
            assert!(expected.iter().all(|icon| *icon != 0));
            let mut taskbar = Taskbar::new(Default::default()).unwrap();
            for _ in 0..3 {
                taskbar.apply(window.0, true);
                taskbar.apply(window.0, false);
                assert_eq!(
                    window.icons(),
                    expected,
                    "Explorer keeps the attention artwork when WM_SETICON restores a null handle"
                );
            }
            taskbar.apply(window.0, true);
            drop(taskbar);
            assert_eq!(
                window.icons(),
                expected,
                "the restored class icons remain borrowed"
            );
        }
        unsafe { UnregisterClassW(class, None).unwrap() };
    }

    impl Drop for TestWindow {
        fn drop(&mut self) {
            let _ = unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn the_overlay_path_leaves_the_window_icon_plain() {
        // Explorer may honor WM_SETICON even for the installed identity, on a
        // button it does not back with a registered shortcut. Painting both
        // paths put two marks on that button.
        let _ = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
        let Ok(overlay) = open_overlay(Default::default()) else {
            return; // the shell's TaskbarList class is not available here
        };
        let window = TestWindow::new();
        let original = window.icons();
        let mut taskbar = Taskbar {
            badge_size: Default::default(),
            icon: Icon::new(256, Default::default()).unwrap(),
            overlay: Some(overlay),
            originals: HashMap::new(),
        };
        taskbar.apply(window.0, true);
        assert_eq!(
            window.icons(),
            original,
            "the shell overlay carries the mark; the window artwork must stay plain"
        );
        taskbar.apply(window.0, false);
        assert_eq!(window.icons(), original);
        drop(taskbar);
        assert_eq!(window.icons(), original);
    }

    #[test]
    fn attention_uses_window_artwork_and_restores_icons() {
        let first = TestWindow::new();
        let second = TestWindow::new();
        let original = first.icons();
        let mut taskbar = Taskbar::new(Default::default()).unwrap();
        taskbar.apply(first.0, true);
        let marked = first.icons();
        assert!(
            marked.iter().all(|icon| *icon != 0),
            "the upper-right badge must be part of the window artwork, not a shell-positioned overlay"
        );
        assert_ne!(marked, original);
        taskbar.apply(first.0, true);
        assert_eq!(first.icons(), marked);
        taskbar.apply(second.0, true);
        assert_eq!(second.icons(), marked);
        taskbar.apply(first.0, false);
        assert_eq!(first.icons(), original);
        assert_eq!(second.icons(), marked);
        // Preserve a real per-window icon as well as the null class fallback.
        let custom = unsafe { LoadIconW(None, IDI_APPLICATION).unwrap() };
        unsafe {
            SendMessageW(
                first.0,
                WM_SETICON,
                WPARAM(ICON_BIG as usize),
                LPARAM(custom.0 as isize),
            );
        }
        let custom_original = first.icons();
        taskbar.apply(first.0, true);
        taskbar.apply(first.0, true);
        drop(taskbar);
        assert_eq!(first.icons(), custom_original);
        assert_eq!(second.icons(), original);
    }
}
