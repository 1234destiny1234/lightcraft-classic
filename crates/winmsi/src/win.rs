//! The msi.dll call. Every pointer handed to Windows points into a buffer this module owns for the
//! whole call.

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::PathBuf;

use windows_sys::Win32::System::ApplicationInstallationAndServicing::{
    INSTALLSTATE, INSTALLSTATE_ABSENT, INSTALLSTATE_ADVERTISED, INSTALLSTATE_LOCAL, INSTALLSTATE_MOREDATA, INSTALLSTATE_NOTUSED, INSTALLSTATE_SOURCE,
    INSTALLSTATE_UNKNOWN, MsiLocateComponentW,
};

use crate::{Error, Result};

/// The first buffer, in UTF-16 units; a longer path is asked for again at its length.
const FIRST: u32 = 520;
/// The longest path accepted (Windows' extended-length limit).
const LONGEST: u32 = 32_768;

fn wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn locate_component(component: &str) -> Result<Option<PathBuf>> {
    let code = wide(component);
    let mut size = FIRST;
    // at most twice: the second call has the length the first one reported
    for _ in 0..2 {
        let mut buf = vec![0u16; size as usize];
        let mut len = size;
        // SAFETY: `code` is NUL-terminated and lives for the call; `buf` holds `len` units, as
        // `len` says, and Windows writes at most that many (it reports MOREDATA otherwise).
        let state: INSTALLSTATE = unsafe { MsiLocateComponentW(code.as_ptr(), buf.as_mut_ptr(), &mut len) };
        match state {
            INSTALLSTATE_LOCAL | INSTALLSTATE_SOURCE => {
                let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len()).min(len as usize);
                let path = String::from_utf16_lossy(buf.get(..end).unwrap_or(&[]));
                return Ok((!path.is_empty()).then(|| PathBuf::from(path)));
            }
            INSTALLSTATE_MOREDATA => {
                // `len` is the path's length without its NUL
                size = len.saturating_add(1);
                if size > LONGEST {
                    return Err(Error::System(format!("the component's path is {len} characters long")));
                }
            }
            INSTALLSTATE_UNKNOWN | INSTALLSTATE_ABSENT | INSTALLSTATE_ADVERTISED | INSTALLSTATE_NOTUSED => return Ok(None),
            other => return Err(Error::System(format!("the component's install state is {other}"))),
        }
    }
    Err(Error::System("the component's path kept growing".into()))
}
