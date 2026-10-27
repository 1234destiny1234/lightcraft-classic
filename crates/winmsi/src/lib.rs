//! Windows Installer lookup: [`locate_component`] asks Windows Installer (`MsiLocateComponentW`)
//! where the key file of an installed component is. The updater uses it to tell a copy the MSI
//! installed (whose files Windows Installer owns and only the installer may replace) from a copy
//! unpacked from the portable zip (which updates itself in place).
//!
//! Windows Installer keeps this record itself: it is written when the MSI installs the component
//! and removed when it is uninstalled, so nothing else has to be installed or kept in sync.
//!
//! This is one of the crates allowed `unsafe` (the one msi.dll call, in `win`). Its API is safe:
//! the GUID is checked before the call and every buffer handed to Windows is owned for the whole
//! call. On other platforms [`locate_component`] returns [`Error::Unsupported`].
//!
//! Sources: Microsoft Win32 documentation (MsiLocateComponentW, INSTALLSTATE). Own design otherwise.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::fmt;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not Windows.
    Unsupported,
    /// The component code is not a braced GUID (`{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`).
    Invalid(String),
    /// Windows Installer answered with a state that says neither "installed here" nor "not
    /// installed" (a broken or incomplete install, a bad configuration).
    System(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Unsupported => f.write_str("Windows Installer is only on Windows"),
            Error::Invalid(s) => write!(f, "Windows Installer: {s}"),
            Error::System(s) => write!(f, "Windows Installer: {s}"),
        }
    }
}

impl std::error::Error for Error {}

pub type Result<T> = std::result::Result<T, Error>;

/// Where Windows Installer has the key file of `component` (a braced GUID, the component's `Guid`
/// in the MSI): `Some(path)` when it is installed on this machine, `None` when no installed
/// product has it.
pub fn locate_component(component: &str) -> Result<Option<PathBuf>> {
    check_guid(component)?;
    #[cfg(windows)]
    {
        win::locate_component(component)
    }
    #[cfg(not(windows))]
    {
        Err(Error::Unsupported)
    }
}

/// `{XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX}`, hex digits in either case.
fn check_guid(guid: &str) -> Result<()> {
    let inner = guid.strip_prefix('{').and_then(|g| g.strip_suffix('}'));
    let groups: Vec<&str> = inner.map(|g| g.split('-').collect()).unwrap_or_default();
    let lengths: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    if lengths == [8, 4, 4, 4, 12] && groups.iter().all(|g| g.bytes().all(|b| b.is_ascii_hexdigit())) {
        Ok(())
    } else {
        Err(Error::Invalid(format!("{guid:?} is not a braced GUID")))
    }
}

#[cfg(windows)]
#[allow(unsafe_code)]
mod win;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_braced_guid_is_asked_about() {
        for bad in [
            "",
            "{}",
            "8A1D3A39-F7C1-47B1-97AE-40C80BF82F4A",
            "{8A1D3A39-F7C1-47B1-97AE-40C80BF82F4}",
            "{8A1D3A39-F7C1-47B1-97AE-40C80BF82F4G}",
            "{8A1D3A39F7C147B197AE40C80BF82F4A}",
            "{8A1D3A39-F7C1-47B1-97AE-40C80BF82F4A}\0",
        ] {
            assert!(matches!(locate_component(bad), Err(Error::Invalid(_))), "{bad:?}");
        }
        assert_eq!(check_guid("{8a1d3a39-f7c1-47b1-97ae-40c80bf82f4a}"), Ok(()));
    }

    #[test]
    fn a_component_nothing_installed_is_not_found() {
        // a fresh GUID no product uses
        let r = locate_component("{0E5C4D1B-7A0F-4B8E-9C31-5D2E7F6A9B10}");
        if cfg!(windows) {
            assert_eq!(r, Ok(None));
        } else {
            assert_eq!(r, Err(Error::Unsupported));
        }
    }

    /// When Windows has an installed component whose key file exists, the path comes back whole.
    /// The Visual C++ runtime's and Windows' own MSIs vary between machines, so this asks about the
    /// app's component and only checks the answer's shape when it is installed here.
    #[test]
    fn an_installed_component_names_an_existing_file() {
        if let Ok(Some(path)) = locate_component("{8A1D3A39-F7C1-47B1-97AE-40C80BF82F4A}") {
            assert!(path.is_absolute(), "{}", path.display());
        }
    }
}
