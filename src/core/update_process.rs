//! Bounded subprocesses used during update verification and installation.

use std::fmt;
use std::fs::File;
use std::io::{self, Read as _, Seek as _};
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const POLL: Duration = Duration::from_millis(25);
const STOP_TIMEOUT: Duration = Duration::from_secs(5);
// The Windows updater verifies in-process; the GUI and Unix utilities use it.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) const VERIFY_TIMEOUT: Duration = Duration::from_secs(2 * 60);

#[derive(Debug)]
pub(crate) struct Failure {
    pub(crate) detail: String,
    /// A surviving process means callers must preserve staging and must not
    /// relaunch an app over files the installer could still be changing.
    pub(crate) running_pid: Option<u32>,
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.detail.fmt(f)
    }
}

impl std::error::Error for Failure {}

impl From<String> for Failure {
    fn from(detail: String) -> Self {
        Self {
            detail,
            running_pid: None,
        }
    }
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        error.to_string().into()
    }
}

pub(crate) fn run(
    command: &mut Command,
    timeout: Duration,
    cancelled: &dyn Fn() -> bool,
) -> Result<Output, Failure> {
    if cancelled() {
        return Err(
            io::Error::new(io::ErrorKind::Interrupted, "update preparation cancelled").into(),
        );
    }
    // Files avoid both full pipe buffers and readers blocked on a descriptor
    // inherited by a grandchild. They disappear when the last handle closes.
    let mut stdout = tempfile::tempfile()?;
    let mut stderr = tempfile::tempfile()?;
    command
        .stdin(Stdio::null())
        .stdout(stdout.try_clone()?)
        .stderr(stderr.try_clone()?);
    let mut process = Process::spawn(command)?;
    let started = Instant::now();
    let result = loop {
        if cancelled() {
            break Err("update preparation cancelled".to_string());
        }
        match process.child.try_wait() {
            Ok(Some(status)) => match process.group.empty() {
                Ok(true) => break Ok(status),
                Ok(false) => {}
                Err(error) => break Err(format!("checking the command's processes: {error}")),
            },
            Ok(None) => {}
            Err(error) => break Err(format!("waiting for the command: {error}")),
        }
        if started.elapsed() >= timeout {
            break Err(format!(
                "{} did not finish within {timeout:?}",
                command.get_program().to_string_lossy()
            ));
        }
        thread::sleep(POLL);
    };
    match result {
        Ok(status) => {
            process.finished = true;
            Ok(Output {
                status,
                stdout: read_output(&mut stdout)?,
                stderr: read_output(&mut stderr)?,
            })
        }
        Err(mut detail) => {
            let running_pid = if process.stop() {
                None
            } else {
                detail.push_str("; process termination could not be confirmed; close the remaining updater before retrying");
                Some(process.group.running_pid(process.child.id()))
            };
            Err(Failure {
                detail,
                running_pid,
            })
        }
    }
}

fn read_output(file: &mut File) -> io::Result<Vec<u8>> {
    file.rewind()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

struct Process {
    child: Child,
    group: platform::Group,
    finished: bool,
}

impl Process {
    fn spawn(command: &mut Command) -> Result<Self, Failure> {
        let group = platform::Group::prepare(command)?;
        let child = command.spawn()?;
        let mut process = Self {
            child,
            group,
            finished: false,
        };
        if let Err(error) = process.group.attach(&process.child) {
            let stopped = process.stop();
            return Err(Failure {
                detail: format!("starting the update command: {error}"),
                running_pid: (!stopped).then_some(process.child.id()),
            });
        }
        Ok(process)
    }

    fn stop(&mut self) -> bool {
        // On Windows the child is assigned to a job before it starts, so even
        // an installer bootstrapper's descendants cannot escape this stop.
        let _ = self.group.kill();
        let _ = self.child.kill();
        let deadline = Instant::now() + STOP_TIMEOUT;
        loop {
            if self.child.try_wait().is_ok_and(|status| status.is_some())
                && self.group.empty().unwrap_or(false)
            {
                self.finished = true;
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            thread::sleep(POLL);
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        if !self.finished {
            let _ = self.group.kill();
            let _ = self.child.kill();
        }
    }
}

#[cfg(unix)]
mod platform {
    use super::*;
    use std::os::unix::process::CommandExt as _;

    pub(super) struct Group {
        pid: i32,
        owned: bool,
    }

    impl Group {
        pub(super) fn prepare(command: &mut Command) -> io::Result<Self> {
            const PARENT: &str = "TTY7_UPDATE_COMMAND_PARENT";
            // The GUI supervises the verifier, which supervises ditto,
            // codesign or AppImage extraction. Those nested commands must
            // stay in the outer group so cancelling it reaches every child.
            // A stale inherited marker cannot match a different parent.
            let inherited = std::env::var(PARENT)
                .ok()
                .and_then(|value| value.parse::<i32>().ok())
                .is_some_and(|pid| pid > 0 && pid == unsafe { libc::getppid() });
            let pid = if inherited {
                unsafe { libc::getpgrp() }
            } else {
                0
            };
            command
                .process_group(pid)
                .env(PARENT, std::process::id().to_string());
            Ok(Self {
                pid,
                owned: !inherited,
            })
        }
        pub(super) fn attach(&mut self, child: &Child) -> io::Result<()> {
            if self.owned {
                self.pid = child.id() as i32;
            }
            Ok(())
        }
        pub(super) fn kill(&self) -> io::Result<()> {
            if self.pid > 0 && unsafe { libc::kill(-self.pid, libc::SIGKILL) } != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return Err(error);
                }
            }
            Ok(())
        }
        pub(super) fn empty(&self) -> io::Result<bool> {
            // The outer supervisor waits for all members. A nested supervisor
            // can only wait for its own child, since it is itself a member.
            if !self.owned {
                return Ok(true);
            }
            if self.pid == 0 || unsafe { libc::kill(-self.pid, 0) } != 0 {
                let error = io::Error::last_os_error();
                if self.pid == 0 || error.raw_os_error() == Some(libc::ESRCH) {
                    return Ok(true);
                }
                return Err(error);
            }
            Ok(false)
        }
        pub(super) fn running_pid(&self, fallback: u32) -> u32 {
            fallback
        }
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::mem::{size_of, zeroed};
    use std::os::windows::io::AsRawHandle as _;
    use std::os::windows::process::CommandExt as _;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
    };
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JobObjectBasicAccountingInformation, JobObjectBasicProcessIdList,
        JobObjectExtendedLimitInformation, QueryInformationJobObject, SetInformationJobObject,
        TerminateJobObject,
    };
    use windows_sys::Win32::System::Threading::{
        CREATE_NO_WINDOW, CREATE_SUSPENDED, OpenThread, ResumeThread, THREAD_SUSPEND_RESUME,
    };

    struct Handle(HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }

    pub(super) struct Group {
        job: Handle,
    }

    impl Group {
        pub(super) fn prepare(command: &mut Command) -> io::Result<Self> {
            // All handles are private to this supervisor. The job is configured
            // before the suspended child can create any descendants.
            let job = unsafe { CreateJobObjectW(std::ptr::null(), std::ptr::null()) };
            if job.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Handle(job);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { zeroed() };
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if unsafe {
                SetInformationJobObject(
                    job.0,
                    JobObjectExtendedLimitInformation,
                    (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                    size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            command.creation_flags(CREATE_NO_WINDOW | CREATE_SUSPENDED);
            Ok(Self { job })
        }
        pub(super) fn attach(&mut self, child: &Child) -> io::Result<()> {
            if unsafe { AssignProcessToJobObject(self.job.0, child.as_raw_handle()) } == 0 {
                return Err(io::Error::last_os_error());
            }
            // std::process does not expose the primary thread handle. Toolhelp
            // finds it while the child is still suspended and owns one thread.
            let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
            if snapshot == INVALID_HANDLE_VALUE {
                return Err(io::Error::last_os_error());
            }
            let snapshot = Handle(snapshot);
            let mut entry: THREADENTRY32 = unsafe { zeroed() };
            entry.dwSize = size_of::<THREADENTRY32>() as u32;
            let mut found = unsafe { Thread32First(snapshot.0, &mut entry) };
            while found != 0 {
                if entry.th32OwnerProcessID == child.id() {
                    let thread =
                        unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                    if thread.is_null() {
                        return Err(io::Error::last_os_error());
                    }
                    let thread = Handle(thread);
                    if unsafe { ResumeThread(thread.0) } == u32::MAX {
                        return Err(io::Error::last_os_error());
                    }
                    return Ok(());
                }
                found = unsafe { Thread32Next(snapshot.0, &mut entry) };
            }
            Err(io::Error::other(
                "the updater command's primary thread was not found",
            ))
        }
        pub(super) fn kill(&self) -> io::Result<()> {
            if unsafe { TerminateJobObject(self.job.0, 1) } == 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
        pub(super) fn empty(&self) -> io::Result<bool> {
            let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    std::ptr::null_mut(),
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(info.ActiveProcesses == 0)
        }
        pub(super) fn running_pid(&self, fallback: u32) -> u32 {
            #[repr(C)]
            struct Pids {
                assigned: u32,
                listed: u32,
                ids: [usize; 256],
            }
            let mut pids: Pids = unsafe { zeroed() };
            if unsafe {
                QueryInformationJobObject(
                    self.job.0,
                    JobObjectBasicProcessIdList,
                    (&mut pids as *mut Pids).cast(),
                    size_of::<Pids>() as u32,
                    std::ptr::null_mut(),
                )
            } != 0
                && pids.listed > 0
            {
                return pids.ids[0] as u32;
            }
            fallback
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn fixture(mode: &str, pid_file: &Path) -> Command {
        let test_name = concat!(module_path!(), "::process_fixture");
        let test_name = test_name.split_once("::").unwrap().1;
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args(["--exact", test_name, "--ignored", "--nocapture"])
            .env("TTY7_UPDATE_PROCESS_FIXTURE", mode)
            .env("TTY7_UPDATE_PROCESS_PID_FILE", pid_file);
        command
    }

    #[test]
    #[ignore = "child process fixture, invoked by the other tests"]
    fn process_fixture() {
        let path = std::env::var_os("TTY7_UPDATE_PROCESS_PID_FILE").unwrap();
        let path = Path::new(&path);
        match std::env::var("TTY7_UPDATE_PROCESS_FIXTURE")
            .unwrap()
            .as_str()
        {
            "tree" => {
                let mut child = fixture("sleep", path).spawn().unwrap();
                child.wait().unwrap();
            }
            "nested" => {
                run(&mut fixture("sleep", path), Duration::from_secs(5), &|| {
                    false
                })
                .unwrap();
            }
            "sleep" => {
                std::fs::write(path, std::process::id().to_string()).unwrap();
                thread::sleep(Duration::from_secs(3));
            }
            "output" => {
                std::fs::write(path, b"ran").unwrap();
                print!("{}", "x".repeat(200_000));
                eprint!("{}", "e".repeat(200_000));
            }
            mode => panic!("unknown fixture {mode}"),
        }
    }

    #[test]
    fn cancelled_commands_never_start() {
        let root = tempfile::tempdir().unwrap();
        let marker = root.path().join("started");
        let error = run(
            &mut fixture("output", &marker),
            Duration::from_secs(5),
            &|| true,
        )
        .unwrap_err();
        assert!(error.running_pid.is_none());
        assert!(!marker.exists());
    }

    #[test]
    fn cancellation_stops_the_entire_command_tree() {
        let root = tempfile::tempdir().unwrap();
        let pid_file = root.path().join("pid");
        let started = Instant::now();
        let error = run(
            &mut fixture("tree", &pid_file),
            Duration::from_secs(5),
            &|| pid_file.exists(),
        )
        .unwrap_err();
        assert!(error.detail.contains("cancelled"), "{error}");
        assert!(error.running_pid.is_none(), "{error}");
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

    #[test]
    fn cancellation_stops_nested_command_supervisors() {
        let root = tempfile::tempdir().unwrap();
        let pid_file = root.path().join("pid");
        let error = run(
            &mut fixture("nested", &pid_file),
            Duration::from_secs(5),
            &|| pid_file.exists(),
        )
        .unwrap_err();
        assert!(error.running_pid.is_none(), "{error}");
        let pid: u32 = std::fs::read_to_string(pid_file).unwrap().parse().unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while process_alive(pid) && Instant::now() < deadline {
            thread::sleep(POLL);
        }
        assert!(
            !process_alive(pid),
            "nested command {pid} survived cancellation"
        );
    }

    fn process_alive(pid: u32) -> bool {
        #[cfg(unix)]
        {
            unsafe { libc::kill(pid as i32, 0) == 0 }
        }
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{CloseHandle, WAIT_TIMEOUT};
            use windows_sys::Win32::System::Threading::{
                OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject,
            };
            let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
            if handle.is_null() {
                return false;
            }
            let result = unsafe { WaitForSingleObject(handle, 0) };
            unsafe {
                CloseHandle(handle);
            }
            result == WAIT_TIMEOUT
        }
    }

    #[test]
    fn a_hung_command_tree_times_out() {
        let root = tempfile::tempdir().unwrap();
        let started = Instant::now();
        let error = run(
            &mut fixture("tree", &root.path().join("pid")),
            Duration::from_millis(500),
            &|| false,
        )
        .unwrap_err();
        assert!(error.detail.contains("did not finish"), "{error}");
        assert!(error.running_pid.is_none(), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn large_output_does_not_block_command_completion() {
        let root = tempfile::tempdir().unwrap();
        let output = run(
            &mut fixture("output", &root.path().join("pid")),
            Duration::from_secs(5),
            &|| false,
        )
        .unwrap();
        assert!(output.status.success());
        assert!(output.stdout.len() >= 200_000);
        assert_eq!(output.stderr.len(), 200_000);
    }
}
