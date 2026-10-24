//! How much of the current thread's stack is left, so a runtime can size
//! QuickJS's stack limit from the thread it is created on.

/// Bytes of this thread's stack below the caller's frame, when the OS can
/// tell. `None` on platforms or threads where it cannot.
#[inline(never)]
pub(crate) fn remaining() -> Option<usize> {
    let here = 0u8;
    let here = std::hint::black_box(&here) as *const u8 as usize;
    let low = stack_low()?;
    here.checked_sub(low)
}

/// Above this the limit stops being one: an unlimited stack rlimit can report
/// terabytes, and runaway recursion should still end in a `RangeError`.
const MAX_LIMIT: usize = 256 * 1024 * 1024;

/// QuickJS's stack limit for a runtime created with `remaining` bytes of stack
/// below it: half, capped. `None` keeps QuickJS's default, since a limit of 0
/// would turn its check off.
pub(crate) fn limit_for(remaining: usize) -> Option<usize> {
    let limit = (remaining / 2).min(MAX_LIMIT);
    (limit > 0).then_some(limit)
}

/// Lowest address of this thread's stack.
#[cfg(any(target_os = "linux", target_os = "android"))]
fn stack_low() -> Option<usize> {
    unsafe {
        // musl (and OHOS's libc, derived from it) reports only the part of
        // the main thread's stack mapped so far, which can be a few hundred
        // KiB of a stack that grows on demand.
        if cfg!(any(target_env = "musl", target_env = "ohos"))
            && libc::syscall(libc::SYS_gettid) == libc::getpid() as libc::c_long
        {
            return None;
        }
        let mut attr: libc::pthread_attr_t = std::mem::zeroed();
        if libc::pthread_getattr_np(libc::pthread_self(), &mut attr) != 0 {
            return None;
        }
        let mut addr: *mut libc::c_void = std::ptr::null_mut();
        let mut size: libc::size_t = 0;
        let ok = libc::pthread_attr_getstack(&attr, &mut addr, &mut size) == 0;
        libc::pthread_attr_destroy(&mut attr);
        (ok && !addr.is_null()).then_some(addr as usize)
    }
}

/// Lowest address of this thread's stack.
#[cfg(target_vendor = "apple")]
fn stack_low() -> Option<usize> {
    unsafe {
        let this = libc::pthread_self();
        // The address is the top of the stack; it grows down `size` bytes.
        let top = libc::pthread_get_stackaddr_np(this) as usize;
        let size = libc::pthread_get_stacksize_np(this);
        top.checked_sub(size)
    }
}

/// Lowest address of this thread's stack.
#[cfg(windows)]
fn stack_low() -> Option<usize> {
    unsafe extern "system" {
        fn GetCurrentThreadStackLimits(low: *mut usize, high: *mut usize);
    }
    let (mut low, mut high) = (0usize, 0usize);
    unsafe { GetCurrentThreadStackLimits(&mut low, &mut high) };
    (low != 0).then_some(low)
}

#[cfg(not(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    windows
)))]
fn stack_low() -> Option<usize> {
    None
}

#[cfg(test)]
mod tests {
    use super::{MAX_LIMIT, limit_for};

    #[test]
    fn the_limit_is_half_the_stack_left_never_zero_and_capped() {
        assert_eq!(limit_for(8 << 20), Some(4 << 20));
        assert_eq!(limit_for(1), None);
        assert_eq!(limit_for(0), None);
        assert_eq!(limit_for(usize::MAX), Some(MAX_LIMIT));
    }

    #[test]
    fn a_thread_reports_about_the_stack_it_was_given() {
        const SIZE: usize = 4 * 1024 * 1024;
        let remaining = std::thread::Builder::new()
            .stack_size(SIZE)
            .spawn(super::remaining)
            .unwrap()
            .join()
            .unwrap()
            .expect("stack bounds on this platform");
        // Some was spent getting here, and the OS may round the size up.
        assert!(
            remaining > SIZE / 2 && remaining <= SIZE + 64 * 1024,
            "{remaining}"
        );
    }
}
