//! A handle scope around each backend operation, so the handles an operation
//! creates are freed when it returns instead of piling up in the context's
//! base scope. Values leaving an operation survive it: each holds a
//! persistent reference or keeps its primitive by value.

use crate::arkjs;
use std::cell::RefCell;
use std::marker::PhantomData;
use std::ptr;

// Operations in progress per env. Per env, not per thread: an operation on one
// env that re-enters another (through a native callback) must still give the
// second env its own scope, which may belong to another VM.
thread_local! {
    // Already const; clippy reports it as not on the OHOS target.
    #[allow(clippy::missing_const_for_thread_local)]
    static DEPTH: RefCell<Vec<(usize, u32)>> = const { RefCell::new(Vec::new()) };
}

/// Opens a handle scope for the outermost operation on its env; nested
/// operations add to it. Inside a native callback driven from Rust, every
/// operation is nested, so its handles stay until the callback returns:
/// [`OpScope::enter_new`] bounds a long batch there. Lives on the stack of one synchronous backend
/// function: not `Send`, never held across an await.
pub(crate) struct OpScope {
    env: arkjs::JSVM_Env,
    scope: arkjs::JSVM_HandleScope,
    _not_send: PhantomData<*const ()>,
}

impl OpScope {
    pub(crate) fn enter(env: arkjs::JSVM_Env) -> Self {
        Self::open(env, false)
    }

    /// Opens a scope even inside an operation in progress, so an embedder can
    /// bound the handles of each step of a long batch within one native call.
    pub(crate) fn enter_new(env: arkjs::JSVM_Env) -> Self {
        Self::open(env, true)
    }

    fn open(env: arkjs::JSVM_Env, always: bool) -> Self {
        let key = env as usize;
        let outermost = DEPTH.with(|depth| {
            let mut depth = depth.borrow_mut();
            match depth.iter_mut().find(|(e, _)| *e == key) {
                Some((_, n)) => {
                    *n += 1;
                    *n == 1
                }
                None => {
                    depth.push((key, 1));
                    true
                }
            }
        });
        let mut scope: arkjs::JSVM_HandleScope = ptr::null_mut();
        if (outermost || always) && !env.is_null() {
            let status = unsafe { arkjs::OH_JSVM_OpenHandleScope(env, &mut scope) };
            if status != arkjs::JSVM_Status_JSVM_OK {
                scope = ptr::null_mut();
            }
        }
        Self {
            env,
            scope,
            _not_send: PhantomData,
        }
    }
}

impl Drop for OpScope {
    fn drop(&mut self) {
        if !self.scope.is_null() {
            // A pending exception lives on the VM, not in the scope; callers
            // read it into a kept value before the scope closes.
            unsafe {
                arkjs::OH_JSVM_CloseHandleScope(self.env, self.scope);
            }
        }
        DEPTH.with(|depth| {
            let mut depth = depth.borrow_mut();
            let key = self.env as usize;
            if let Some(i) = depth.iter().position(|(e, _)| *e == key) {
                depth[i].1 -= 1;
                if depth[i].1 == 0 {
                    depth.swap_remove(i);
                }
            }
        });
    }
}
