//! Wait for the existing GUI without terminating it or reusing its PID.

use std::thread;
use std::time::{Duration, Instant};

pub(super) const EXIT_TIMEOUT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(100);

pub(super) fn wait_for_exit(pid: u32) -> Result<(), String> {
    wait_within(pid, EXIT_TIMEOUT)
}

fn expired(pid: u32, timeout: Duration) -> String {
    format!(
        "parent process {pid} was still running after {timeout:?}; the update was not installed"
    )
}

fn poll_until_exit(
    pid: u32,
    timeout: Duration,
    alive: impl Fn() -> Result<bool, String>,
) -> Result<(), String> {
    let started = Instant::now();
    while alive()? {
        if started.elapsed() >= timeout {
            return Err(expired(pid, timeout));
        }
        thread::sleep(POLL.min(timeout.saturating_sub(started.elapsed())));
    }
    Ok(())
}

#[cfg(unix)]
fn wait_within(pid: u32, timeout: Duration) -> Result<(), String> {
    let pid = i32::try_from(pid)
        .ok()
        .filter(|pid| *pid > 0)
        .ok_or_else(|| "invalid parent pid".to_string())?;
    // When the GUI is our parent, reparenting avoids mistaking a recycled PID
    // for a GUI that has not exited. Hand-run helpers use a bounded fallback.
    let is_parent = unsafe { libc::getppid() } == pid;
    poll_until_exit(pid as u32, timeout, || {
        if is_parent {
            return Ok(unsafe { libc::getppid() } == pid);
        }
        if unsafe { libc::kill(pid, 0) } == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::ESRCH) => Ok(false),
            Some(libc::EPERM) => Ok(true),
            _ => Err(format!("checking parent process {pid}: {error}")),
        }
    })
}

#[cfg(windows)]
fn wait_within(pid: u32, timeout: Duration) -> Result<(), String> {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER, GetLastError, WAIT_OBJECT_0,
        WAIT_TIMEOUT,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
    };

    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    if handle.is_null() {
        let error = unsafe { GetLastError() };
        if error == ERROR_INVALID_PARAMETER {
            return Ok(());
        }
        if error != ERROR_ACCESS_DENIED {
            return Err(format!("opening parent process {pid}: OS error {error}"));
        }
        // The elevated chain may lack a handle across accounts. Each probe
        // is a zero-duration wait, and the same overall deadline still applies.
        return poll_until_exit(pid, timeout, || {
            let probe = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if probe.is_null() {
                return match unsafe { GetLastError() } {
                    ERROR_INVALID_PARAMETER => Ok(false),
                    ERROR_ACCESS_DENIED => Ok(true),
                    error => Err(format!("checking parent process {pid}: OS error {error}")),
                };
            }
            let result = unsafe { WaitForSingleObject(probe, 0) };
            let error = std::io::Error::last_os_error();
            unsafe {
                CloseHandle(probe);
            }
            match result {
                WAIT_OBJECT_0 => Ok(false),
                WAIT_TIMEOUT => Ok(true),
                _ => Err(format!("checking parent process {pid}: {error}")),
            }
        });
    }
    // The held process object continues to identify the original GUI.
    let millis = timeout.as_millis().min((u32::MAX - 1) as u128) as u32;
    let result = unsafe { WaitForSingleObject(handle, millis) };
    let error = std::io::Error::last_os_error();
    unsafe {
        CloseHandle(handle);
    }
    match result {
        WAIT_OBJECT_0 => Ok(()),
        WAIT_TIMEOUT => Err(expired(pid, timeout)),
        _ => Err(format!("waiting for parent process {pid}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_running_gui_is_not_killed_or_waited_on_forever() {
        let started = Instant::now();
        let error = wait_within(std::process::id(), Duration::from_millis(50)).unwrap_err();
        assert!(error.contains("was still running"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn observation_fallback_uses_the_same_deadline() {
        let started = Instant::now();
        assert!(poll_until_exit(123, Duration::from_millis(20), || Ok(true)).is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
        assert!(poll_until_exit(123, Duration::ZERO, || Ok(false)).is_ok());
    }
}
