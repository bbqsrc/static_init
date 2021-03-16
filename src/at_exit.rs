#![cfg(any(elf, mach_o, coff))]

mod exit_manager {
    use crate::Finaly;
    use crate::GlobalManager as SubManager;
    use crate::{Manager, ManagerBase, OnceManager, Phase, Static};

    use core::cell::Cell;
    use core::ptr::NonNull;

    trait OnExit {
        fn get_next(&self) -> Option<NonNull<Node>>;
        fn execute(&self);
    }

    type Node = dyn 'static + OnExit + Sync;

    /// Opaque type used for registration management
    /// To be used with GlobalRegister
    pub struct ExitManager<const G:bool> {
        sub:  SubManager<G>,
        next: Cell<Option<NonNull<Node>>>,
    }

    mod reg {

        use super::{ExitManager, Node};
        use crate::{destructor, Finaly, Static};

        use parking_lot::{lock_api::RawMutex, Mutex};

        use core::ptr::NonNull;

        struct Wrap(NonNull<Node>);

        unsafe impl Send for Wrap{}

        static REGISTER: Mutex<(Option<Wrap>, bool)> =
            Mutex::const_new(RawMutex::INIT, (None, true));

        #[destructor(0)]
        extern "C" fn execute_at_exit() {
            let mut l = REGISTER.lock();
            let mut list: Option<NonNull<Node>> = l.0.take().map(|w| w.0);
            drop(l);
            while let Some(mut on_exit) = list {
                let r = unsafe { on_exit.as_mut() };
                r.execute();
                list = r.get_next().or_else(|| {
                    let mut l = REGISTER.lock();
                    if l.0.is_none() {
                        l.1 = true;
                    }
                    l.0.take().map(|w| w.0)
                });
            }
        }

        pub fn register<T: Static<Manager = ExitManager<G>> + Sync,const G:bool>(st: &T) -> bool
        where
            T::Data: Finaly,
        {
            let mut l = REGISTER.lock();
            if l.1 {
                Static::manager(st).next.set(l.0.take().map(|w| w.0));
                *l = (Some(Wrap((st as &Node).into())), true);
                true
            } else {
                false
            }
        }
    }

    const GLOBAL_INIT: ExitManager<true> = ExitManager {
        sub:  SubManager::new(),
        next: Cell::new(None),
    };
    const GLOBAL_INIT_LAZY: ExitManager<false> = ExitManager {
        sub:  SubManager::new_lazy(),
        next: Cell::new(None),
    };

    impl ExitManager<true> {
        pub const unsafe fn new() -> Self {
            GLOBAL_INIT
        }
    }
    impl ExitManager<false> {
        pub const unsafe fn new_lazy() -> Self {
            GLOBAL_INIT_LAZY
        }
    }

    impl<const G:bool> AsRef<SubManager<G>> for ExitManager<G> {
        fn as_ref(&self) -> &SubManager<G> {
            &self.sub
        }
    }

    //All access of next are done when REGISTER is locked
    //or when the access is exclusive in execute_at_exit
    unsafe impl<const G:bool> Sync for ExitManager<G> {}

    impl<const G:bool> ManagerBase for ExitManager<G> {
        fn phase(&self) -> Phase {
            self.sub.phase()
        }
    }

    unsafe impl<T: Static<Manager = Self>,const G:bool> Manager<T> for ExitManager<G>
    where
        T: Sync,
        T::Data: Finaly,
    {
        #[inline(always)]

        fn register(
            st: &T,
            on_uninited: impl Fn(Phase) -> bool,
            init: impl FnOnce(&<T as Static>::Data) -> bool,
            on_registration_failure: impl FnOnce(&<T as Static>::Data),
        ) {
            <SubManager<G> as OnceManager<T>>::register(
                st,
                on_uninited,
                init,
                reg::register,
                on_registration_failure,
            )
        }
    }

    impl<T: Static<Manager = ExitManager<G>>, const G:bool> OnExit for T
    where
        T::Data: Finaly,
    {
        fn get_next(&self) -> Option<NonNull<Node>> {
            Static::manager(self).next.get()
        }
        fn execute(&self) {
            <SubManager<G> as OnceManager<T>>::finalize(self, Finaly::finaly);
        }
    }
}
pub use exit_manager::ExitManager;

pub use local_manager::ThreadExitManager;
mod local_manager {

    use crate::LocalManager as SubManager;
    use crate::{Finaly, Manager, ManagerBase, OnceManager, Phase, Static};

    use core::cell::Cell;
    use core::ptr::NonNull;

    trait OnExit {
        fn get_next(&self) -> Option<NonNull<Node>>;
        fn execute(&self);
    }

    type Node = dyn 'static + OnExit;

    pub struct ThreadExitManager {
        sub:  SubManager,
        next: Cell<Option<NonNull<Node>>>,
    }

    const LOCAL_INIT: ThreadExitManager = ThreadExitManager {
        sub:  SubManager::new(),
        next: Cell::new(None),
    };

    impl ThreadExitManager {
        pub const unsafe fn new() -> Self {
            LOCAL_INIT
        }
    }

    impl AsRef<SubManager> for ThreadExitManager {
        fn as_ref(&self) -> &SubManager {
            &self.sub
        }
    }

    impl ManagerBase for ThreadExitManager {
        fn phase(&self) -> Phase {
            self.sub.phase()
        }
    }

    unsafe impl<T: Static<Manager = Self>> Manager<T> for ThreadExitManager
    where
        T::Data: Finaly,
    {
        #[inline(always)]

        fn register(
            st: &T,
            on_uninited: impl Fn(Phase) -> bool,
            init: impl FnOnce(&<T as Static>::Data) -> bool,
            on_registration_failure: impl FnOnce(&<T as Static>::Data),
        ) {
            <SubManager as OnceManager<T>>::register(
                st,
                on_uninited,
                init,
                register,
                on_registration_failure,
            )
        }
    }

    impl<T: Static<Manager = ThreadExitManager>> OnExit for T
    where
        T::Data: Finaly,
    {
        fn get_next(&self) -> Option<NonNull<Node>> {
            Static::manager(self).next.get()
        }
        fn execute(&self) {
            <SubManager as OnceManager<T>>::finalize(self, Finaly::finaly);
        }
    }

    #[cfg(coff_thread_at_exit)]
    mod windows {
        use super::{Node, ThreadExitManager};
        use crate::{Finaly, Static};
        use core::cell::Cell;
        use core::ptr::NonNull;

        #[cfg(target_arch = "x86_64")]
        type Reason = u64;
        #[cfg(target_arch = "i686")]
        type Reason = u32;
        //On thread exit
        //non nul pointers between .CRT$XLA and .CRT$XLZ will be
        //run... => So we could implement thread_local drop without
        //registration...
        #[link_section = ".CRT$XLAZ"] //do this after the standard library
        #[used]
        pub static AT_THEAD_EXIT: extern "system" fn(*mut u8, Reason, *mut u8) = destroy;

        extern "system" fn destroy(_: *mut u8, reason: Reason, _: *mut u8) {
            const DLL_THREAD_DETACH: Reason = 3;
            const DLL_PROCESS_DETACH: Reason = 0;
            if reason == DLL_THREAD_DETACH || reason == DLL_PROCESS_DETACH {
                let mut o_ptr = REGISTER.take();
                while let Some(ptr) = o_ptr {
                    let r = unsafe { ptr.as_ref() };
                    r.execute();
                    o_ptr = r.get_next();
                    o_ptr.or_else(|| REGISTER.take());
                }
                DONE.set(true)
            }

            // Copy pasted from: std/src/sys/windows/thread_local_key.rs
            //
            // See comments above for what this is doing. Note that we don't need this
            // trickery on GNU windows, just on MSVC.
            //
            // TODO: better implement it as in libstdc++ implementation of __cxa_thread_atexit?
            unsafe { reference_tls_used() };
            #[cfg(target_env = "msvc")]
            unsafe fn reference_tls_used() {
                extern "C" {
                    static _tls_used: u8;
                }
                core::ptr::read_volatile(&_tls_used);
            }
            #[cfg(not(target_env = "msvc"))]
            unsafe fn reference_tls_used() {}
        }

        #[thread_local]
        static REGISTER: Cell<Option<NonNull<Node>>> = Cell::new(None);

        #[thread_local]
        static DONE: Cell<bool> = Cell::new(false);

        pub(super) fn register<T: Static<Manager = ThreadExitManager>>(st: &T) -> bool
        where
            T::Data: Finaly,
        {
            if DONE.get() {
                false
            } else {
                unsafe { Static::manager(st).next.set(REGISTER.take()) };
                REGISTER.set(Some((st as &Node).into()));
                true
            }
        }
    }
    #[cfg(coff_thread_at_exit)]
    use windows::register;

    #[cfg(cxa_thread_at_exit)]
    mod cxa {
        use super::{Node, ThreadExitManager};
        use crate::{Finaly, Static};
        use core::cell::Cell;
        use core::ptr::{null_mut, NonNull};

        extern "C" {
            #[linkage = "extern_weak"]
            static __dso_handle: *mut u8;
            #[linkage = "extern_weak"]
            static __cxa_thread_atexit_impl: *const core::ffi::c_void;
        }

        /// Register a function along with a pointer.
        ///
        /// When the thread exit, functions register with this
        /// function will be called in reverse order of their addition
        /// and will take as argument the `data`.
        fn at_thread_exit(f: extern "C" fn(*mut u8), data: *mut u8) {
            type CxaThreadAtExit =
                extern "C" fn(f: extern "C" fn(*mut u8), data: *mut u8, dso_handle: *mut u8);

            unsafe {
                assert!(!__cxa_thread_atexit_impl.is_null()); //
                let at_thread_exit_impl: CxaThreadAtExit =
                    core::mem::transmute(__cxa_thread_atexit_impl);
                at_thread_exit_impl(f, data, __dso_handle);
            }
        }

        #[thread_local]
        static REGISTER: Cell<Option<NonNull<Node>>> = Cell::new(None);

        #[thread_local]
        static DESTROYING: Cell<bool> = Cell::new(false);

        extern "C" fn execute_destroy(_: *mut u8) {
            DESTROYING.set(true);
            let mut o_ptr = REGISTER.take();
            while let Some(ptr) = o_ptr {
                let r = unsafe { ptr.as_ref() };
                r.execute();
                o_ptr = r.get_next().or_else(|| REGISTER.take());
            }
            DESTROYING.set(false);
        }
        pub(super) fn register<T: Static<Manager = ThreadExitManager>>(st: &T) -> bool
        where
            T::Data: Finaly,
        {
            let old = REGISTER.take();
            if let Some(old) = old {
                Static::manager(st).next.set(Some(old));
            } else if !DESTROYING.get() {
                at_thread_exit(execute_destroy, null_mut())
            }
            REGISTER.set(Some((st as &Node).into()));
            true
        }
    }
    #[cfg(cxa_thread_at_exit)]
    use cxa::register;

    #[cfg(pthread_thread_at_exit)]
    mod pthread {
        use super::{Node, ThreadExitManager};
        use crate::{Finaly, Static};

        use core::cell::Cell;
        use core::ffi::c_void;
        use core::ptr::NonNull;
        use core::sync::atomic::{AtomicUsize, Ordering};

        use libc::{
            pthread_getspecific, pthread_key_create, pthread_key_delete, pthread_key_t,
            pthread_setspecific,
        };

        //minimum number of time a destructor key may be registered while destructors are run
        const _POSIX_THREAD_DESTRUCTOR_ITERATIONS: usize = 4;

        static DESTRUCTOR_KEY: AtomicUsize = AtomicUsize::new(usize::MAX);

        #[thread_local]
        static ITERATION_COUNT: Cell<usize> = Cell::new(0);

        #[thread_local]
        static REGISTER: Cell<Option<NonNull<Node>>> = Cell::new(None);

        extern "C" fn execute_destroy(_: *mut c_void) {
            let mut opt_head = REGISTER.take();
            while let Some(ptr) = opt_head {
                let r = unsafe { ptr.as_ref() };
                r.execute();
                opt_head = r.get_next().or_else(|| REGISTER.take());
            }
        }

        /// Here panics are prefered so that we are sure
        /// that if it returns false, no memory allocation
        /// has been done, which avoid recursions.
        ///
        /// To do => an init()
        fn get_key() -> Option<pthread_key_t> {
            //TODO a revoir
            let mut key = DESTRUCTOR_KEY.load(Ordering::Acquire);
            let mut lk = 0;
            while key == usize::MAX {
                //The minimum number of key is 128, we require only one contrarily to
                //what happen in standard library (one per thread local on some targets)
                //on glibc the limit is 1024. So this could definitively fail.
                if unsafe {
                    pthread_key_create(&mut lk as *mut pthread_key_t, Some(execute_destroy)) != 0
                } {
                    key = DESTRUCTOR_KEY.load(Ordering::Acquire);
                    if key != usize::MAX {
                        break;
                    } else {
                        return None;
                    }
                }
                if lk as usize == usize::MAX {
                    unsafe { pthread_key_delete(lk) };
                } else {
                    key = match DESTRUCTOR_KEY.compare_exchange(
                        usize::MAX,
                        lk as usize,
                        Ordering::Release,
                        Ordering::Acquire,
                    ) {
                        Ok(k) => k,
                        Err(k) => {
                            unsafe { pthread_key_delete(lk) };
                            k
                        }
                    };
                }
            }
            Some(key as pthread_key_t)
        }
        pub(super) fn register_on_thread_exit<T: Static<Manager = ThreadExitManager>>(
            st: &T,
            key: pthread_key_t,
        ) -> bool
        where
            T::Data: Finaly,
        {
            let specific = unsafe { pthread_getspecific(key) };

            if specific.is_null() {
                if ITERATION_COUNT.get() < _POSIX_THREAD_DESTRUCTOR_ITERATIONS {
                    if unsafe { pthread_setspecific(key, NonNull::dangling().as_ptr()) } != 0 {
                        return false;
                    }

                    ITERATION_COUNT.set(ITERATION_COUNT.get() + 1);
                } else {
                    return false;
                }
            }

            unsafe { Static::manager(st).next.set(REGISTER.take()) };

            REGISTER.set(Some((st as &Node).into()));
            true
        }

        pub struct LocalRegister;

        pub(super) fn register<T: Static<Manager = ThreadExitManager>>(st: &T) -> bool
        where
            T::Data: Finaly,
        {
            match get_key() {
                Some(key) => register_on_thread_exit(st, key),
                None => false,
            }
        }
    }
    #[cfg(pthread_thread_at_exit)]
    use pthread::LocalRegister;
}
