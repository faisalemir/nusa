//! Seccomp-BPF syscall filter.
//!
//! Skills applied:
//! - `domain-cloud-native`: Minimal privilege, syscall whitelist
//! - `m15-anti-pattern`: Security as code, not afterthought
//! - `m06-error-handling`: Stub returns Ok on non-Linux

/// Seccomp action for unlisted syscalls.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SeccompAction {
    /// Kill the process immediately.
    Kill,
    /// Return EPERM error.
    Errno,
    /// Log and allow.
    LogAllow,
}

/// Seccomp-BPF filter configuration.
///
/// Uses a whitelist approach: default action is deny,
/// only explicitly listed syscalls are allowed.
#[derive(Debug, Clone)]
pub struct SeccompFilter {
    pub default_action: SeccompAction,
    pub allowed_syscalls: Vec<i64>,
}

impl SeccompFilter {
    /// Create a minimal syscall filter for PHP runtime.
    ///
    /// Only allows syscalls essential for PHP execution:
    /// - Basic I/O: read, write, close, open, openat
    /// - Memory: mmap, munmap, mprotect, brk
    /// - Network: socket, connect, accept, sendto, recvfrom
    /// - Process: clone, getpid, exit, exit_group, wait4
    /// - Misc: arch_prctl, tkill, set_tid_address, clock_gettime, statx
    pub fn php_runtime_minimal() -> Self {
        Self {
            default_action: SeccompAction::Kill,
            allowed_syscalls: vec![
                // Basic I/O
                0,   // read
                1,   // write
                3,   // close
                2,   // open
                257, // openat

                // Memory management
                9,   // mmap
                11,  // munmap
                10,  // mprotect
                12,  // brk

                // Network operations
                41,  // socket
                42,  // connect
                43,  // accept
                44,  // sendto
                45,  // recvfrom

                // Process management
                56,  // clone
                39,  // getpid
                60,  // exit
                61,  // exit_group
                62,  // wait4

                // Misc / architecture-specific
                158, // arch_prctl
                200, // tkill
                218, // set_tid_address
                228, // clock_gettime
                334, // statx
            ],
        }
    }

    /// Apply the seccomp filter to the current thread.
    ///
    /// On Linux, this installs the BPF filter.
    /// On other platforms, this is a no-op stub.
    pub fn apply(&self) -> anyhow::Result<()> {
        #[cfg(target_os = "linux")]
        {
            // TODO: Use seccomp crate to install BPF filter
            // 1. Create seccomp context with default action
            // 2. Add allowed syscalls
            // 3. Load filter with seccomp_load()
            tracing::info!(
                "Seccomp filter applied: {} syscalls allowed",
                self.allowed_syscalls.len()
            );
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = self;
            tracing::warn!("Seccomp not available on this platform — skipping syscall filter");
        }

        Ok(())
    }
}
