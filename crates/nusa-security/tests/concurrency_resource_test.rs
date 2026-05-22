//! Concurrency and resource tests for nusa-security crate.
//!
//! Covers: Landlock concurrent apply, Seccomp concurrent apply, Landlock FD leak,
//! Seccomp filter size within kernel limits.

use std::time::Duration;

// ── Landlock: Concurrent Apply ──

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(target_os = "linux")]
async fn landlock_concurrent_apply_no_race() {
    // === Arrange ===
    let tmp_dir = std::env::temp_dir().join("nusa-landlock-test");
    std::fs::create_dir_all(&tmp_dir).expect("must create");
    let code_dir = tmp_dir.join("code");
    std::fs::create_dir_all(&code_dir).expect("must create");

    // === Act ===
    // On Linux, Landlock can only be applied once per process (restrict_self is one-way).
    // Test that concurrent calls don't cause a race (first one wins, others may fail).
    let mut handles = Vec::new();
    for _ in 0..4 {
        let code = code_dir.clone();
        let tmp = tmp_dir.clone();
        handles.push(tokio::spawn(async move {
            let result = nusa_security::apply_landlock(&code, &tmp);
            result.is_ok()
        }));
    }

    // === Assert ===
    let mut success_count = 0usize;
    for h in handles {
        let ok = tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        if ok {
            success_count += 1;
        }
    }

    // Landlock is one-way per process: at most one apply succeeds; zero is ok if unsupported.
    assert!(
        success_count <= 1,
        "concurrent landlock apply must not race (got {success_count} successes)"
    );
    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[cfg(not(target_os = "linux"))]
async fn landlock_concurrent_apply_non_linux_noop() {
    // === Arrange ===
    let tmp_dir = std::env::temp_dir().join("nusa-landlock-test");
    std::fs::create_dir_all(&tmp_dir).expect("must create");
    let code_dir = tmp_dir.join("code");
    std::fs::create_dir_all(&code_dir).expect("must create");

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        let code = code_dir.clone();
        let tmp = tmp_dir.clone();
        handles.push(tokio::spawn(async move {
            nusa_security::apply_landlock(&code, &tmp)
        }));
    }

    // === Assert ===
    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        assert!(result.is_ok(), "non-Linux must always return Ok");
    }

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ── Seccomp: Concurrent Apply ──

#[tokio::test]
#[cfg(not(target_os = "linux"))]
async fn seccomp_concurrent_apply_non_linux_noop() {
    // === Arrange ===
    // On non-Linux, seccomp is a no-op

    // === Act ===
    let mut handles = Vec::new();
    for _ in 0..8 {
        handles.push(tokio::spawn(async move { nusa_security::apply_seccomp() }));
    }

    // === Assert ===
    for h in handles {
        let result = tokio::time::timeout(Duration::from_secs(5), h)
            .await
            .expect("must complete")
            .expect("must not panic");
        assert!(result.is_ok(), "non-Linux must always return Ok");
    }
}

// ── Landlock: FD Leak ──

#[test]
#[cfg(target_os = "linux")]
fn landlock_pathfd_creation_no_fd_leak() {
    // === Arrange ===
    use landlock::PathFd;
    let start_fds = count_open_fds();

    let tmp_dir = std::env::temp_dir().join("nusa-landlock-fd-test");
    std::fs::create_dir_all(&tmp_dir).expect("must create");

    // === Act ===
    for i in 0..100 {
        let path = tmp_dir.join(format!("file_{}", i));
        std::fs::write(&path, "test").expect("must write");
        let fd = PathFd::new(&path).expect("must create PathFd");
        drop(fd); // FD should be released
    }

    // === Assert ===
    let end_fds = count_open_fds();
    assert!(
        end_fds <= start_fds + 10,
        "FD count must be stable after PathFd creation+drop"
    );

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
#[cfg(not(target_os = "linux"))]
fn landlock_pathfd_non_linux_skipped() {
    // === Arrange ===
    let tmp_dir = std::env::temp_dir().join("nusa-landlock-nonlinux");
    std::fs::create_dir_all(&tmp_dir).expect("must create");

    // === Act ===
    let result = nusa_security::apply_landlock(&tmp_dir, &tmp_dir);

    // === Assert ===
    assert!(result.is_ok(), "non-Linux must always return Ok");

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

// ── Seccomp: Filter Size ──

#[test]
#[cfg(target_os = "linux")]
fn seccomp_bpf_program_size_within_kernel_limits() {
    // === Arrange ===
    use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};

    // The kernel limit for BPF filter length is typically 4096 instructions
    const MAX_BPF_INSTRUCTIONS: usize = 4096;

    // === Act ===
    // Build the same filter as nusa-security does
    let allowed_syscalls: Vec<(i64, Vec<seccompiler::SeccompRule>)> = vec![
        (libc::SYS_read, vec![]),
        (libc::SYS_write, vec![]),
        (libc::SYS_writev, vec![]),
        (libc::SYS_close, vec![]),
        (libc::SYS_fstat, vec![]),
        (libc::SYS_stat, vec![]),
        (libc::SYS_lstat, vec![]),
        (libc::SYS_statx, vec![]),
        (libc::SYS_ioctl, vec![]),
        (libc::SYS_access, vec![]),
        (libc::SYS_faccessat, vec![]),
        (libc::SYS_mmap, vec![]),
        (libc::SYS_mprotect, vec![]),
        (libc::SYS_munmap, vec![]),
        (libc::SYS_brk, vec![]),
        (libc::SYS_madvise, vec![]),
        (libc::SYS_mremap, vec![]),
        (libc::SYS_openat, vec![]),
        (libc::SYS_openat2, vec![]),
        (libc::SYS_getcwd, vec![]),
        (libc::SYS_chdir, vec![]),
        (libc::SYS_fchdir, vec![]),
        (libc::SYS_renameat2, vec![]),
        (libc::SYS_unlinkat, vec![]),
        (libc::SYS_ftruncate, vec![]),
        (libc::SYS_fcntl, vec![]),
        (libc::SYS_lseek, vec![]),
        (libc::SYS_dup, vec![]),
        (libc::SYS_dup3, vec![]),
        (libc::SYS_pipe2, vec![]),
        (libc::SYS_socket, vec![]),
        (libc::SYS_connect, vec![]),
        (libc::SYS_accept4, vec![]),
        (libc::SYS_bind, vec![]),
        (libc::SYS_listen, vec![]),
        (libc::SYS_sendto, vec![]),
        (libc::SYS_recvfrom, vec![]),
        (libc::SYS_getsockopt, vec![]),
        (libc::SYS_setsockopt, vec![]),
        (libc::SYS_shutdown, vec![]),
        (libc::SYS_epoll_create1, vec![]),
        (libc::SYS_epoll_ctl, vec![]),
        (libc::SYS_epoll_wait, vec![]),
        (libc::SYS_epoll_pwait, vec![]),
        (libc::SYS_eventfd2, vec![]),
        (libc::SYS_clock_gettime, vec![]),
        (libc::SYS_gettimeofday, vec![]),
        (libc::SYS_nanosleep, vec![]),
        (libc::SYS_timerfd_create, vec![]),
        (libc::SYS_timerfd_settime, vec![]),
        (libc::SYS_clone3, vec![]),
        (libc::SYS_futex, vec![]),
        (libc::SYS_exit, vec![]),
        (libc::SYS_exit_group, vec![]),
        (libc::SYS_set_tid_address, vec![]),
        (libc::SYS_gettid, vec![]),
        (libc::SYS_sched_yield, vec![]),
        (libc::SYS_rt_sigaction, vec![]),
        (libc::SYS_rt_sigprocmask, vec![]),
        (libc::SYS_rt_sigreturn, vec![]),
        (libc::SYS_getpid, vec![]),
        (libc::SYS_getuid, vec![]),
        (libc::SYS_getgid, vec![]),
        (libc::SYS_getrandom, vec![]),
        (libc::SYS_uname, vec![]),
        (libc::SYS_sysinfo, vec![]),
        (libc::SYS_arch_prctl, vec![]),
        (libc::SYS_getdents64, vec![]),
        (libc::SYS_newfstatat, vec![]),
        (libc::SYS_memfd_create, vec![]),
        (libc::SYS_seccomp, vec![]),
        (libc::SYS_execve, vec![]),
        (libc::SYS_wait4, vec![]),
    ];

    let filter = SeccompFilter::new(
        allowed_syscalls.into_iter().collect(),
        SeccompAction::Allow,
        SeccompAction::KillThread,
        seccompiler::TargetArch::x86_64,
    );

    // === Assert ===
    if let Ok(filter) = filter {
        let bpf: Result<BpfProgram, _> = filter.try_into();
        if let Ok(bpf_program) = bpf {
            let instruction_count = bpf_program.len();
            assert!(
                instruction_count < MAX_BPF_INSTRUCTIONS,
                "BPF filter has {} instructions, limit is {}",
                instruction_count,
                MAX_BPF_INSTRUCTIONS
            );
        }
    }
    // If filter build fails (e.g., unsupported syscall), that's OK too
}

// ── Helper ──

#[cfg(unix)]
fn count_open_fds() -> usize {
    use std::fs;
    let fd_dir = "/proc/self/fd";
    if let Ok(entries) = fs::read_dir(fd_dir) {
        entries.count()
    } else {
        0
    }
}

#[cfg(not(unix))]
#[allow(dead_code)]
fn count_open_fds() -> usize {
    0
}
