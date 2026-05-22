//! Seccomp-BPF syscall filter for PHP process sandboxing.
//!
//! Skills applied:
//! - `domain-cloud-native`: Syscall whitelist for minimal attack surface
//! - `m15-anti-pattern`: Security enforced before server start

/// Compile the Seccomp-BPF program without applying it to the current thread.
///
/// Use this from tests; call [`apply_seccomp_filter`] only in child processes or at
/// process entry before other threads start.
#[cfg(target_os = "linux")]
pub fn build_seccomp_filter() -> anyhow::Result<seccompiler::BpfProgram> {
    use seccompiler::{SeccompAction, SeccompFilter, SeccompRule};

    tracing::debug!("Building Seccomp-BPF syscall filter");

    // Define allowed syscalls — essential for a PHP runtime
    let allowed_syscalls: Vec<(i64, Vec<SeccompRule>)> = vec![
        // I/O
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
        // Memory
        (libc::SYS_mmap, vec![]),
        (libc::SYS_mprotect, vec![]),
        (libc::SYS_munmap, vec![]),
        (libc::SYS_brk, vec![]),
        (libc::SYS_madvise, vec![]),
        (libc::SYS_mremap, vec![]),
        // File operations
        (libc::SYS_open, vec![]),
        (libc::SYS_openat, vec![]),
        (libc::SYS_openat2, vec![]),
        (libc::SYS_creat, vec![]),
        (libc::SYS_readlink, vec![]),
        (libc::SYS_readlinkat, vec![]),
        (libc::SYS_getcwd, vec![]),
        (libc::SYS_chdir, vec![]),
        (libc::SYS_fchdir, vec![]),
        (libc::SYS_rename, vec![]),
        (libc::SYS_renameat, vec![]),
        (libc::SYS_renameat2, vec![]),
        (libc::SYS_unlink, vec![]),
        (libc::SYS_unlinkat, vec![]),
        (libc::SYS_link, vec![]),
        (libc::SYS_linkat, vec![]),
        (libc::SYS_symlink, vec![]),
        (libc::SYS_symlinkat, vec![]),
        (libc::SYS_ftruncate, vec![]),
        (libc::SYS_truncate, vec![]),
        (libc::SYS_fcntl, vec![]),
        (libc::SYS_flock, vec![]),
        (libc::SYS_fchmod, vec![]),
        (libc::SYS_fchmodat, vec![]),
        (libc::SYS_fchown, vec![]),
        (libc::SYS_fchownat, vec![]),
        (libc::SYS_lseek, vec![]),
        (libc::SYS_dup, vec![]),
        (libc::SYS_dup2, vec![]),
        (libc::SYS_dup3, vec![]),
        (libc::SYS_pipe, vec![]),
        (libc::SYS_pipe2, vec![]),
        // Network
        (libc::SYS_socket, vec![]),
        (libc::SYS_connect, vec![]),
        (libc::SYS_accept, vec![]),
        (libc::SYS_accept4, vec![]),
        (libc::SYS_bind, vec![]),
        (libc::SYS_listen, vec![]),
        (libc::SYS_sendto, vec![]),
        (libc::SYS_recvfrom, vec![]),
        (libc::SYS_getsockopt, vec![]),
        (libc::SYS_setsockopt, vec![]),
        (libc::SYS_shutdown, vec![]),
        (libc::SYS_getpeername, vec![]),
        (libc::SYS_getsockname, vec![]),
        (libc::SYS_sethostname, vec![]),
        // epoll / eventfd
        (libc::SYS_epoll_create, vec![]),
        (libc::SYS_epoll_create1, vec![]),
        (libc::SYS_epoll_ctl, vec![]),
        (libc::SYS_epoll_wait, vec![]),
        (libc::SYS_epoll_pwait, vec![]),
        (libc::SYS_eventfd, vec![]),
        (libc::SYS_eventfd2, vec![]),
        // Time
        (libc::SYS_clock_gettime, vec![]),
        (libc::SYS_clock_getres, vec![]),
        (libc::SYS_gettimeofday, vec![]),
        (libc::SYS_nanosleep, vec![]),
        (libc::SYS_clock_nanosleep, vec![]),
        (libc::SYS_timerfd_create, vec![]),
        (libc::SYS_timerfd_gettime, vec![]),
        (libc::SYS_timerfd_settime, vec![]),
        // Threading / process
        (libc::SYS_clone, vec![]),
        (libc::SYS_clone3, vec![]),
        (libc::SYS_futex, vec![]),
        (libc::SYS_exit, vec![]),
        (libc::SYS_exit_group, vec![]),
        (libc::SYS_set_tid_address, vec![]),
        (libc::SYS_gettid, vec![]),
        (libc::SYS_sched_yield, vec![]),
        (libc::SYS_sched_getaffinity, vec![]),
        (libc::SYS_sched_setaffinity, vec![]),
        (libc::SYS_set_robust_list, vec![]),
        (libc::SYS_get_robust_list, vec![]),
        (libc::SYS_rt_sigaction, vec![]),
        (libc::SYS_rt_sigprocmask, vec![]),
        (libc::SYS_rt_sigreturn, vec![]),
        (libc::SYS_rt_sigtimedwait, vec![]),
        // Misc
        (libc::SYS_getpid, vec![]),
        (libc::SYS_getuid, vec![]),
        (libc::SYS_getgid, vec![]),
        (libc::SYS_geteuid, vec![]),
        (libc::SYS_getegid, vec![]),
        (libc::SYS_getrandom, vec![]),
        (libc::SYS_uname, vec![]),
        (libc::SYS_sysinfo, vec![]),
        (libc::SYS_prctl, vec![]),
        (libc::SYS_arch_prctl, vec![]),
        (libc::SYS_set_thread_area, vec![]),
        (libc::SYS_get_thread_area, vec![]),
        (libc::SYS_rseq, vec![]),
        (libc::SYS_getdents64, vec![]),
        (libc::SYS_newfstatat, vec![]),
        (libc::SYS_statfs, vec![]),
        (libc::SYS_fstatfs, vec![]),
        (libc::SYS_memfd_create, vec![]),
        (libc::SYS_seccomp, vec![]),
        (libc::SYS_execve, vec![]),
        (libc::SYS_wait4, vec![]),
    ];

    // Build the seccomp filter: allow listed syscalls, kill thread on violation
    let filter = SeccompFilter::new(
        allowed_syscalls.into_iter().collect(),
        SeccompAction::Allow,
        SeccompAction::KillThread,
        seccompiler::TargetArch::x86_64,
    )
    .map_err(|e| anyhow::anyhow!("Failed to build seccomp filter: {}", e))?;

    filter
        .try_into()
        .map_err(|e| anyhow::anyhow!("Failed to compile seccomp BPF: {}", e))
}

/// Apply Seccomp-BPF syscall filter to the **current** process.
///
/// Whitelist: read, write, mmap, mprotect, brk, close, fstat, ioctl, epoll_*,
///            eventfd, accept4, connect, getsockopt, setsockopt, sendto, recvfrom,
///            clone3, exit, exit_group, futex
///
/// Block: ptrace, mount, umount2, reboot, kexec, keyctl, bpf, unshare, pivot_root
#[cfg(target_os = "linux")]
pub fn apply_seccomp_filter() -> anyhow::Result<()> {
    tracing::info!("Applying Seccomp-BPF syscall filter");
    let bpf_filter = build_seccomp_filter()?;
    seccompiler::apply_filter(&bpf_filter)
        .map_err(|e| anyhow::anyhow!("Failed to apply seccomp filter: {}", e))?;
    tracing::info!("Seccomp-BPF syscall filter applied successfully");
    Ok(())
}

/// Verify the filter builds on this platform without installing it (safe for unit tests).
#[cfg(target_os = "linux")]
pub fn verify_seccomp_filter() -> anyhow::Result<()> {
    let _ = build_seccomp_filter()?;
    Ok(())
}

/// Stub for non-Linux platforms.
#[cfg(not(target_os = "linux"))]
pub fn build_seccomp_filter() -> anyhow::Result<()> {
    tracing::debug!("Seccomp not available on this platform, skipping");
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub fn apply_seccomp_filter() -> anyhow::Result<()> {
    build_seccomp_filter()
}

#[cfg(not(target_os = "linux"))]
pub fn verify_seccomp_filter() -> anyhow::Result<()> {
    build_seccomp_filter()
}
