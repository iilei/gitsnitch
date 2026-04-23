use std::io::{self, IsTerminal};

use windows_sys::Win32::System::Console::{
    ENABLE_VIRTUAL_TERMINAL_PROCESSING, GetConsoleMode, GetStdHandle, STD_ERROR_HANDLE,
    STD_OUTPUT_HANDLE, SetConsoleMode,
};

fn enable_vt_processing_for_handle(std_handle: u32, is_terminal: bool) -> bool {
    if !is_terminal {
        return false;
    }

    // SAFETY: Win32 console mode APIs are called with handles returned by GetStdHandle,
    // and all fallible calls are checked for success and converted to `false`.
    let handle = unsafe { GetStdHandle(std_handle) };
    if handle == 0 || handle == -1 {
        return false;
    }

    let mut mode = 0;
    // SAFETY: `mode` points to valid, writable memory for GetConsoleMode.
    let get_mode_ok = unsafe { GetConsoleMode(handle, &mut mode) } != 0;
    if !get_mode_ok {
        return false;
    }

    if (mode & ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0 {
        return true;
    }

    // SAFETY: `handle` is a console handle and `mode` is derived from GetConsoleMode.
    unsafe { SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0 }
}

pub(crate) fn enable_vt_processing_stdout() -> bool {
    enable_vt_processing_for_handle(STD_OUTPUT_HANDLE, io::stdout().is_terminal())
}

pub(crate) fn enable_vt_processing_stderr() -> bool {
    enable_vt_processing_for_handle(STD_ERROR_HANDLE, io::stderr().is_terminal())
}
