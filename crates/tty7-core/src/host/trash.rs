//! Recoverable deletion is separate from the host's permanent `remove`.

use std::io;
use std::path::Path;

#[cfg(target_os = "linux")]
pub(super) fn move_to_trash(path: &Path) -> io::Result<()> {
    // GIO implements the desktop trash specification, including the mount's
    // own trash and original-location metadata. It refuses unsupported mounts
    // instead of falling back to unlink. No shell interprets the path.
    let path = std::path::absolute(path)?;
    let output = std::process::Command::new("gio")
        .arg("trash")
        .arg("--")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                io::Error::new(
                    io::ErrorKind::Unsupported,
                    "the gio trash command is unavailable",
                )
            } else {
                e
            }
        })?;
    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

#[cfg(windows)]
pub(super) fn move_to_trash(path: &Path) -> io::Result<()> {
    let path = std::path::absolute(path)?;
    if path.file_name().is_none() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "cannot trash a root",
        ));
    }

    // A fresh STA owns every COM object below. In particular, do not use the
    // Shell delete flags that answer confirmation dialogs with Yes: those can
    // permanently delete items when the bin is unavailable.
    std::thread::Builder::new()
        .name("tty7-recycle".into())
        .spawn(move || {
            use windows::Win32::System::Com::{
                COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize,
            };
            unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
                .ok()
                .map_err(io::Error::other)?;
            struct ComApartment;
            impl Drop for ComApartment {
                fn drop(&mut self) {
                    unsafe { CoUninitialize() }
                }
            }
            let _apartment = ComApartment;
            recycle_windows_item(&path).map_err(io::Error::other)
        })?
        .join()
        .map_err(|_| io::Error::other("trash operation interrupted; check the recycle bin"))?
}

#[cfg(windows)]
fn recycle_windows_item(path: &Path) -> windows::core::Result<()> {
    use windows::{
        Win32::{
            Foundation::HANDLE,
            UI::Shell::{
                BHID_Transfer, FOLDERID_RecycleBinFolder, IShellItem, ITransferSource,
                KF_FLAG_DEFAULT, SHCreateItemFromParsingName, SHGetKnownFolderItem,
            },
        },
        core::HSTRING,
    };

    // RecycleItem is the dedicated Shell recycle operation; unlike
    // IFileOperation::DeleteItem it does not fall back to permanent removal.
    unsafe {
        let source: IShellItem =
            SHCreateItemFromParsingName(&HSTRING::from(path.as_os_str()), None)?;
        let parent = source.GetParent()?;
        let transfer: ITransferSource = parent.BindToHandler(None, &BHID_Transfer)?;
        let trash: IShellItem = SHGetKnownFolderItem(
            &FOLDERID_RecycleBinFolder,
            KF_FLAG_DEFAULT,
            HANDLE::default(),
        )?;
        transfer.RecycleItem(&source, &trash, 0)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub(super) fn move_to_trash(path: &Path) -> io::Result<()> {
    use objc2_foundation::{NSFileManager, NSString, NSURL};

    let path = std::path::absolute(path)?;
    let path = path.to_str().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "trash path is not valid UTF-8")
    })?;
    objc2::rc::autoreleasepool(|_| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        NSFileManager::defaultManager()
            .trashItemAtURL_resultingItemURL_error(&url, None)
            .map_err(|error| io::Error::other(error.to_string()))
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
pub(super) fn move_to_trash(_path: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "recoverable trash is not available on this host",
    ))
}
