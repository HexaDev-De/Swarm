//! Owned Windows process trees. Closing a job also terminates its descendants.
#![cfg(windows)]

use std::{
    collections::{hash_map::Entry, HashMap},
    mem::size_of,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    sync::{Mutex, OnceLock},
};
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::HANDLE,
        System::{
            JobObjects::{
                AssignProcessToJobObject, CreateJobObjectW, JobObjectBasicAccountingInformation,
                JobObjectExtendedLimitInformation, QueryInformationJobObject,
                SetInformationJobObject, JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
                JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
            },
            Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
        },
    },
};

pub(crate) struct Job(OwnedHandle);

impl Job {
    /// Call immediately after spawning. Failure leaves the caller's fallback intact.
    pub(crate) fn attach(pid: u32) -> Option<Self> {
        // SAFETY: successful Win32 constructors return owned, valid handles. Each is
        // immediately wrapped for cleanup, including every early return below.
        unsafe {
            let job = Self(OwnedHandle::from_raw_handle(
                CreateJobObjectW(None, PCWSTR::null()).ok()?.0,
            ));
            let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
            .ok()?;
            let process = OwnedHandle::from_raw_handle(
                OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, pid)
                    .ok()?
                    .0,
            );
            AssignProcessToJobObject(job.handle(), HANDLE(process.as_raw_handle())).ok()?;
            Some(job)
        }
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.0.as_raw_handle())
    }

    fn active(&self) -> bool {
        let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
        // SAFETY: the handle remains owned and the output buffer has the declared size.
        let result = unsafe {
            QueryInformationJobObject(
                Some(self.handle()),
                JobObjectBasicAccountingInformation,
                (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                None,
            )
        };
        result.is_err() || info.ActiveProcesses > 0
    }
}

fn registry() -> &'static Mutex<HashMap<u32, Job>> {
    static JOBS: OnceLock<Mutex<HashMap<u32, Job>>> = OnceLock::new();
    JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

pub(crate) fn register(pid: u32) {
    let mut jobs = registry().lock().unwrap_or_else(|error| error.into_inner());
    // Retain orphaned descendants, but never accumulate completed process trees.
    jobs.retain(|_, job| job.active());
    if let Entry::Vacant(entry) = jobs.entry(pid) {
        if let Some(job) = Job::attach(pid) {
            entry.insert(job);
        }
    }
}

pub(crate) fn terminate(pid: u32) -> bool {
    let job = registry()
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .remove(&pid);
    let handled = job.is_some();
    drop(job);
    handled
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    use windows::Win32::{
        Foundation::{WAIT_OBJECT_0, WAIT_TIMEOUT},
        System::Threading::{WaitForSingleObject, PROCESS_SYNCHRONIZE},
    };

    #[test]
    fn descendant_fixture() {
        let Some(path) = std::env::var_os("PANDACODE_WINDOWS_JOB_FIXTURE") else {
            return;
        };
        // The parent assigns this process before allowing its descendant to spawn.
        std::io::stdin().read_exact(&mut [0]).unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "providers::windows_job::tests::leaf_fixture"])
            .env("PANDACODE_WINDOWS_JOB_LEAF", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        std::fs::write(path, format!("{}\n", child.id())).unwrap();
        let _ = child.wait();
    }

    #[test]
    fn leaf_fixture() {
        if std::env::var_os("PANDACODE_WINDOWS_JOB_LEAF").is_some() {
            loop {
                std::thread::park();
            }
        }
    }

    #[test]
    fn dropping_job_terminates_child_and_descendant() {
        let path = std::env::temp_dir().join(format!("pandacode-job-{}", uuid::Uuid::new_v4()));
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "providers::windows_job::tests::descendant_fixture",
            ])
            .env("PANDACODE_WINDOWS_JOB_FIXTURE", &path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let job = match Job::attach(child.id()) {
            Some(job) => job,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("Could not assign the fixture process to a Windows Job Object");
            }
        };
        child.stdin.take().unwrap().write_all(b"\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let pid = loop {
            if let Some(pid) = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| text.strip_suffix('\n')?.parse::<u32>().ok())
            {
                break pid;
            }
            assert!(
                Instant::now() < deadline,
                "Descendant fixture did not start"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        std::fs::remove_file(path).unwrap();
        // SAFETY: OpenProcess transfers a valid handle; both process handles stay
        // owned until all bounded waits finish.
        unsafe {
            let descendant = OwnedHandle::from_raw_handle(
                OpenProcess(PROCESS_SYNCHRONIZE, false, pid).unwrap().0,
            );
            assert_eq!(
                WaitForSingleObject(HANDLE(descendant.as_raw_handle()), 0),
                WAIT_TIMEOUT
            );
            drop(job);
            assert_eq!(
                WaitForSingleObject(HANDLE(child.as_raw_handle()), 5000),
                WAIT_OBJECT_0
            );
            assert_eq!(
                WaitForSingleObject(HANDLE(descendant.as_raw_handle()), 5000),
                WAIT_OBJECT_0
            );
        }
        let _ = child.wait().unwrap();
    }
}
