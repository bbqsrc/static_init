/// The implementation bellow differ considerably
/// of what can be found else where in that the
/// registration is done throw an intrusive list within
/// the static. This avoid to do allocation when the thread_local
/// is first accessed. There are really no drawbacks.
///
use core::cell::Cell;
use core::ptr::NonNull;

/// Object of this type will behave as state machine which state is reported by the `status`
/// method:
///  - `NonRegistered`,
///  - `Registrating`,
///  - `Registered`,
///  - `Executing`,
///  - `Executed`,
///
///  The on_thread_exit will get access to the owned data throw a non `'static` reference
///  which prevents turning a thread local into a static.
///
///  The registration happens only once and Err(status) is returned on trial to register
///  already registered objects.
///
///  The type all similar behavior as `libc::at_exit` except that the registered function
///  is run at thread exit, the data owned and accessed by the handler is necessarily thread
///  local and registration is unlikely to cause memory allocation.
pub struct AtThreadLocalExit<T> {
    pub data:     T,
    pub on_thread_exit: fn(&T),
    pub managed:  AtThreadLocalExitManaged,
}

trait OnThreadExit {
    fn execute(&self);
    fn set_next(&self, _:NonNull<dyn OnThreadExit>);
    fn take_next(&self) -> Option<NonNull<dyn OnThreadExit>>;
}

#[cfg(coff)]
mod windows {
    use super::OnThreadExit;
    use core::cell::Cell;
    use core::ptr::NonNull;
    //On thread exit 
    //non nul pointers between .CRT$XLA and .CRT$XLZ will be
    //run... => So we could implement thread_local drop without
    //registration...
    #[link_section = ".CRT$XLAZ"]//do this after the standard library
    #[used]
    pub static AT_THEAD_EXIT: extern "system" fn(*mut u8,u64,*mut u8)  = destroy;

    extern "system" fn destroy(_:*mut u8,reason: u64,_: *mut u8) 
    {
        const DLL_THREAD_DETACH: u64 = 3;
        const DLL_PROCESS_DETACH: u64 = 0;
        if reason == DLL_THREAD_DETACH || reason == DLL_PROCESS_DETACH {
            let mut o_ptr = REGISTER.take();
            while let Some(ptr) = o_ptr {
                let r = unsafe{ptr.as_ref()};
                r.execute();
                o_ptr = r.take_next(); 
                o_ptr.or_else(|| REGISTER.take());
            }
        }

    // Copy pasted from: std/src/sys/windows/thread_local_key.rs
    //
    // See comments above for what this is doing. Note that we don't need this
    // trickery on GNU windows, just on MSVC.
    unsafe {reference_tls_used()};
    #[cfg(target_env = "msvc")]
    unsafe fn reference_tls_used() {
        extern "C" {
            static _tls_used: u8;
        }
        crate::intrinsics::volatile_load(&_tls_used);
    }
    #[cfg(not(target_env = "msvc"))]
    unsafe fn reference_tls_used() {}


    }

    #[thread_local]
    static REGISTER: Cell<Option<NonNull<dyn OnThreadExit>>> = Cell::new(None);

    pub(super) unsafe fn register_on_thread_exit(r: &(dyn OnThreadExit+'static)) {
        let old = REGISTER.take();
        if let Some(old) = old {
            r.set_next(old);
        }
        REGISTER.set(Some(NonNull::new_unchecked(r as *const _ as *mut _)));
    }

}

#[cfg(elf)]
mod elf {
    use super::OnThreadExit;
    use core::cell::Cell;
    use core::ptr::{self, NonNull};
    extern "C" {
        #[linkage = "extern_weak"]
        static __dso_handle: *mut u8;
        #[linkage = "extern_weak"]
        static __cxa_thread_atexit_impl: *const core::ffi::c_void;
    }

    type CxaThreadAtExit =
        extern "C" fn(f: extern "C" fn(*mut u8), data: *mut u8, dso_handle: *mut u8);

    /// Register a function along with a pointer.
    ///
    /// When the thread exit, functions register with this
    /// function will be called in reverse order of their addition
    /// and will take as argument the `data`.
    fn at_thread_exit(f: extern "C" fn(*mut u8), data: *mut u8) {
        unsafe {
            assert!(!__cxa_thread_atexit_impl.is_null()); //
            let at_thread_exit_impl: CxaThreadAtExit =
                core::mem::transmute(__cxa_thread_atexit_impl);
            at_thread_exit_impl(f, data, __dso_handle);
        }
    }

    #[thread_local]
    static REGISTER: Cell<Option<NonNull<dyn OnThreadExit>>> = Cell::new(None);
    
    extern "C" fn execute_destroy(_: *mut u8) {
        let mut o_ptr = REGISTER.take();
        while let Some(ptr) = o_ptr {
            let r = unsafe{ptr.as_ref()};
            r.execute();
            o_ptr = r.take_next(); 
        }
    }
    /// #Safety
    /// r must refer to a (thread local) static 
    pub(super) unsafe fn register_on_thread_exit(r: &(dyn OnThreadExit + 'static)) {
        let old = REGISTER.take();
        if let Some(old) = old {
            r.set_next(old);
        } else {
            at_thread_exit(execute_destroy, ptr::null_mut())
        }
        REGISTER.set(Some(NonNull::new_unchecked(r as *const _ as *mut _)));
    }
}

#[cfg(all(mach_o))]
mod mach_o {
    use super::OnThreadExit;
    use core::cell::Cell;
    use core::ptr::{self, NonNull};
    extern "C" {
        fn _tlv_atexit(dtor: extern "C" fn(*mut u8), arg: *mut u8);
    }

    #[thread_local]
    static REGISTER: Cell<Option<NonNull<dyn OnThreadExit>>> = Cell::new(None);
    
    extern "C" fn execute_destroy(_: *mut u8) {
        let mut o_ptr = REGISTER.take();
        while let Some(ptr) = o_ptr {
            let r = unsafe{ptr.as_ref()};
            r.execute();
            o_ptr = r.take_next().or_else(|| REGISTER.take()); 
        }
    }
    /// #Safety
    /// r must refer to a (thread local) static 
    pub(super) unsafe fn register_on_thread_exit(r: &(dyn OnThreadExit + 'static)) {
        // the same bug as in the standard library, unfixable, but probably
        // nobody will catch this.
        #[thread_local]
        static mut REGISTERED :bool = false;

        let old = REGISTER.take();
        if let Some(old) = old {
            r.set_next(old);
        } else if !REGISTERED {
                //hopefully at least on thread local was used before start of destruction
                REGISTERED = true;
                _tlv_atexit(execute_destroy, ptr::null_mut())
        }
        REGISTER.set(Some(NonNull::new_unchecked(r as *const _ as *mut _)));
    }
}

#[cfg(elf)]
use elf::register_on_thread_exit;
#[cfg(mach_o)]
use mach_o::register_on_thread_exit;
#[cfg(windows)]
use windows::register_on_thread_exit;


#[derive(Copy, Clone, Eq, PartialEq)]
pub enum Status {
    NonRegistered,
    Registrating,
    Registered,
    Executing,
    Executed,
}
pub struct AtThreadLocalExitManaged {
    status: Cell<Status>,
    next:   Cell<Option<NonNull<dyn OnThreadExit>>>,
}

pub const COMPLETE_INIT: AtThreadLocalExitManaged = AtThreadLocalExitManaged {
    status: Cell::new(Status::NonRegistered),
    next:   Cell::new(None),
};


impl<T:'static> AtThreadLocalExit<T> {
    /// Return the current status
    pub fn status(&self) -> Status {
        self.managed.status.get()
    }
    /// Register the current object for call of on_thread_exit at 
    /// thread destruction
    ///
    /// # Safety 
    ///  self must be a thread_local.
    pub unsafe fn register(&self) -> Result<(),Status> {
        let status = self.managed.status.get();
        if status == Status::NonRegistered {
            self.managed.status.set(Status::Registrating);
            register_on_thread_exit((self as &dyn OnThreadExit).into());
            self.managed.status.set(Status::Registered);
            Ok(())
        } else {
            Err(status)
        }
    }
}
impl<T> OnThreadExit for AtThreadLocalExit<T> {
    fn execute(&self) {
        //debug_assert!(self.status.get() == Status::Registered);
        self.managed.status.set(Status::Executing);
        (self.on_thread_exit)(&self.data);
        self.managed.status.set(Status::Executed);
        }
    fn set_next(&self,ptr: NonNull<dyn OnThreadExit>) {
        self.managed.next.set(Some(ptr));
    }
    fn take_next(&self) -> Option<NonNull<dyn OnThreadExit>> {
        self.managed.next.take()
    }
}
