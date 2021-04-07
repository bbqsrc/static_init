#![cfg(any(elf, mach_o, coff))]

mod exit_manager {
    use crate::mutex::{LockNature, LockResult, SyncPhaseGuard, SyncReadPhaseGuard};
    use crate::mutex::{Mutex, SyncPhasedLocker};
    use crate::splited_sequentializer::SyncSequentializer as SubSequentializer;
    use crate::Finaly;
    use crate::{
        LazySequentializer, Phase, Phased, Sequential, Sequentializer, SplitedLazySequentializer,
    };

    trait OnExit {
        fn take_next(&self) -> Option<&'static Node>;
        fn execute(&self);
    }

    type Node = dyn 'static + OnExit + Sync;

    pub struct ExitSequentializerBase {
        sub:  SubSequentializer,
        next: Mutex<Option<&'static Node>>,
    }

    /// A sequentializer that store finalize_callback  
    /// for execution at program exit
    pub struct ExitSequentializer<const INIT_ON_REF_FAILURE: bool>(ExitSequentializerBase);

    mod reg {

        use super::{ExitSequentializer, Node};
        use crate::{destructor, Finaly, Sequential};

        use crate::mutex::Mutex;

        static REGISTER: Mutex<(Option<&'static Node>, bool)> = Mutex::new((None, true));

        #[destructor(0)]
        extern "C" fn execute_at_exit() {
            let mut l = REGISTER.lock();
            let mut list: Option<&'static Node> = l.0.take();
            drop(l);
            while let Some(on_exit) = list {
                // SAFETY:
                // the reference created mut point to an object:
                //   - this is full-filled by the requirement that the ExitSequentializer object
                //     must be static.
                //   - there should not have any mutable reference to the object: this is
                //   a requirement of the ExitSequentializer object new method
                on_exit.execute();
                list = on_exit.take_next().or_else(|| {
                    let mut l = REGISTER.lock();
                    if l.0.is_none() {
                        l.1 = true;
                    }
                    l.0.take()
                });
            }
        }

        /// Store a reference of the static for execution of the
        /// finalize call back at program exit
        pub fn finalize_at_exit<
            T: 'static + Sequential<Sequentializer = ExitSequentializer<IRF>> + Sync,
            const IRF: bool,
        >(
            st: &'static T,
        ) -> bool
        where
            T::Data: 'static + Finaly,
        {
            let mut l = REGISTER.lock();
            if l.1 {
                let mut next = Sequential::sequentializer(st).0.next.lock();
                assert!(
                    next.is_none(),
                    "Double registration of an ExitSequentializer for finalization at program exit"
                );
                *next = l.0.take();
                *l = (Some(st as &Node), true);
                true
            } else {
                false
            }
        }
    }
    pub use reg::finalize_at_exit;

    #[allow(clippy::declare_interior_mutable_const)]
    /// This object is only used to for const initialization
    const MUTEX_INIT: Mutex<Option<&'static Node>> = Mutex::new(None);

    impl<const IRF: bool> ExitSequentializer<IRF> {
        /// Create a new ExitSequentializer
        ///
        /// Useless if the target object is not 'static
        pub const fn new(l: SyncPhasedLocker) -> Self {
            //Self(GLOBAL_INIT)
            Self(ExitSequentializerBase {
                sub:  SubSequentializer::new(l),
                next: MUTEX_INIT,
            })
        }
    }

    impl<const IRF: bool> AsRef<SubSequentializer> for ExitSequentializer<IRF> {
        fn as_ref(&self) -> &SubSequentializer {
            &self.0.sub
        }
    }
    impl<const IRF: bool> AsMut<SubSequentializer> for ExitSequentializer<IRF> {
        fn as_mut(&mut self) -> &mut SubSequentializer {
            &mut self.0.sub
        }
    }

    impl<const IRF: bool> Phased for ExitSequentializer<IRF> {
        fn phase(this: &Self) -> Phase {
            Phased::phase(&this.0.sub)
        }
    }
    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<'a, T: 'a + Sequential<Sequentializer = Self>, const IRF: bool>
        Sequentializer<'a, T> for ExitSequentializer<IRF>
    where
        T: 'static + Sync,
        T::Data: 'static + Finaly,
    {
        type ReadGuard = SyncReadPhaseGuard<'a, T::Data>;
        type WriteGuard = SyncPhaseGuard<'a, T::Data>;
        fn lock(
            st: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> LockResult<SyncReadPhaseGuard<'a, T::Data>, SyncPhaseGuard<'a, T::Data>> {
            <SubSequentializer as Sequentializer<T>>::lock(st, lock_nature)
        }
        fn try_lock(
            st: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> Option<LockResult<Self::ReadGuard, Self::WriteGuard>> {
            <SubSequentializer as Sequentializer<T>>::try_lock(st, lock_nature)
        }
        fn lock_mut(st: &'a mut T) -> SyncPhaseGuard<'a, T::Data> {
            <SubSequentializer as Sequentializer<T>>::lock_mut(st)
        }
    }

    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<T: 'static + Sequential<Sequentializer = Self>, const IRF: bool>
        LazySequentializer<'static, T> for ExitSequentializer<IRF>
    where
        T: 'static + Sync,
        T::Data: 'static + Finaly,
    {
        #[inline(always)]
        fn init(
            st: &'static T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Phase {
            <SubSequentializer as SplitedLazySequentializer<T>>::init(
                st,
                shall_init,
                init,
                finalize_at_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn init_unique(
            st: &'static mut T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Phase {
            <SubSequentializer as LazySequentializer<T>>::init_unique(st, shall_init, init)
        }
        #[inline(always)]
        fn init_then_read_guard(
            st: &'static T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Self::ReadGuard {
            <SubSequentializer as SplitedLazySequentializer<T>>::init_then_read_guard(
                st,
                shall_init,
                init,
                finalize_at_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn init_then_write_guard(
            st: &'static T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Self::WriteGuard {
            <SubSequentializer as SplitedLazySequentializer<T>>::init_then_write_guard(
                st,
                shall_init,
                init,
                finalize_at_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn try_init_then_read_guard(
            st: &'static T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Option<Self::ReadGuard> {
            <SubSequentializer as SplitedLazySequentializer<T>>::try_init_then_read_guard(
                st,
                shall_init,
                init,
                finalize_at_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn try_init_then_write_guard(
            st: &'static T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Option<Self::WriteGuard> {
            <SubSequentializer as SplitedLazySequentializer<T>>::try_init_then_write_guard(
                st,
                shall_init,
                init,
                finalize_at_exit,
                IRF,
            )
        }
    }

    impl<T: Sequential<Sequentializer = ExitSequentializer<IRF>>, const IRF: bool> OnExit for T
    where
        T::Data: 'static + Finaly,
    {
        fn take_next(&self) -> Option<&'static Node> {
            Sequential::sequentializer(self).0.next.lock().take()
        }
        fn execute(&self) {
            <SubSequentializer as SplitedLazySequentializer<T>>::finalize_callback(
                self,
                Finaly::finaly,
            );
        }
    }
}
pub use exit_manager::{finalize_at_exit, ExitSequentializer};

#[cfg(feature = "thread_local")]
pub use local_manager::{finalize_at_thread_exit, ThreadExitSequentializer};

#[cfg(feature = "thread_local")]
mod local_manager {

    use crate::splited_sequentializer::UnSyncSequentializer as SubSequentializer;
    use crate::{
        Finaly, LazySequentializer, Phase, Phased, Sequential, Sequentializer,
        SplitedLazySequentializer,
    };

    use core::cell::Cell;

    use crate::mutex::{
        LockNature, LockResult, UnSyncPhaseGuard, UnSyncPhaseLocker, UnSyncReadPhaseGuard,
    };

    trait OnExit {
        fn take_next(&self) -> Option<&'static Node>;
        fn execute(&self);
    }

    type Node = dyn 'static + OnExit;

    /// A sequentializer that store finalize_callback  
    /// for execution at thread exit
    struct ThreadExitSequentializerBase {
        sub:  SubSequentializer,
        next: Cell<Option<&'static Node>>,
    }

    #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
    /// A sequentializer that store finalize_callback  
    /// for execution at thread exit
    pub struct ThreadExitSequentializer<const INIT_ON_REF_FAILURE: bool>(
        ThreadExitSequentializerBase,
    );

    #[allow(clippy::declare_interior_mutable_const)]
    /// This object is only used to be copied
    const CELL_INIT: Cell<Option<&'static Node>> = Cell::new(None);

    impl<const IRF: bool> ThreadExitSequentializer<IRF> {
        /// Useless if the target object is not a static thread_local
        pub const fn new(l: UnSyncPhaseLocker) -> Self {
            //Self(GLOBAL_INIT)
            Self(ThreadExitSequentializerBase {
                sub:  SubSequentializer::new(l),
                next: CELL_INIT,
            })
        }
    }

    impl<const IRF: bool> AsRef<SubSequentializer> for ThreadExitSequentializer<IRF> {
        fn as_ref(&self) -> &SubSequentializer {
            &self.0.sub
        }
    }
    impl<const IRF: bool> AsMut<SubSequentializer> for ThreadExitSequentializer<IRF> {
        fn as_mut(&mut self) -> &mut SubSequentializer {
            &mut self.0.sub
        }
    }

    impl<const IRF: bool> Phased for ThreadExitSequentializer<IRF> {
        fn phase(this: &Self) -> Phase {
            Phased::phase(&this.0.sub)
        }
    }
    // SAFETY: it is safe because it does implement locking panic
    unsafe impl<'a, T: 'static + Sequential<Sequentializer = Self>, const IRF: bool>
        Sequentializer<'a, T> for ThreadExitSequentializer<IRF>
    where
        T::Data: 'static + Finaly,
    {
        type ReadGuard = UnSyncReadPhaseGuard<'a, T::Data>;
        type WriteGuard = UnSyncPhaseGuard<'a, T::Data>;

        #[inline(always)]
        fn lock(
            st: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> LockResult<UnSyncReadPhaseGuard<'a, T::Data>, UnSyncPhaseGuard<'a, T::Data>> {
            <SubSequentializer as Sequentializer<T>>::lock(st, lock_nature)
        }
        #[inline(always)]
        fn try_lock(
            st: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> Option<LockResult<Self::ReadGuard, Self::WriteGuard>> {
            <SubSequentializer as Sequentializer<T>>::try_lock(st, lock_nature)
        }
        #[inline(always)]
        fn lock_mut(st: &'a mut T) -> UnSyncPhaseGuard<'a, T::Data> {
            <SubSequentializer as Sequentializer<T>>::lock_mut(st)
        }
    }

    // SAFETY: it is safe because it does implement circular initialization panic
    unsafe impl<T: 'static + Sequential<Sequentializer = Self>, const IRF: bool>
        LazySequentializer<'static, T> for ThreadExitSequentializer<IRF>
    where
        T::Data: 'static + Finaly,
    {
        #[inline(always)]
        fn init(
            st: &'static T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Phase {
            <SubSequentializer as SplitedLazySequentializer<T>>::init(
                st,
                shall_proceed,
                init,
                finalize_at_thread_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn init_unique(
            st: &'static mut T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Phase {
            <SubSequentializer as LazySequentializer<T>>::init_unique(st, shall_proceed, init)
        }
        #[inline(always)]
        fn init_then_read_guard(
            st: &'static T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Self::ReadGuard {
            <SubSequentializer as SplitedLazySequentializer<T>>::init_then_read_guard(
                st,
                shall_proceed,
                init,
                finalize_at_thread_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn init_then_write_guard(
            st: &'static T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Self::WriteGuard {
            <SubSequentializer as SplitedLazySequentializer<T>>::init_then_write_guard(
                st,
                shall_proceed,
                init,
                finalize_at_thread_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn try_init_then_read_guard(
            st: &'static T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Option<Self::ReadGuard> {
            <SubSequentializer as SplitedLazySequentializer<T>>::try_init_then_read_guard(
                st,
                shall_proceed,
                init,
                finalize_at_thread_exit,
                IRF,
            )
        }
        #[inline(always)]
        fn try_init_then_write_guard(
            st: &'static T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'static <T as Sequential>::Data),
        ) -> Option<Self::WriteGuard> {
            <SubSequentializer as SplitedLazySequentializer<T>>::try_init_then_write_guard(
                st,
                shall_proceed,
                init,
                finalize_at_thread_exit,
                IRF,
            )
        }
    }

    impl<
            T: 'static + Sequential<Sequentializer = ThreadExitSequentializer<IRF>>,
            const IRF: bool,
        > OnExit for T
    where
        T::Data: 'static + Finaly,
    {
        fn take_next(&self) -> Option<&'static Node> {
            Sequential::sequentializer(self).0.next.take()
        }
        fn execute(&self) {
            <SubSequentializer as SplitedLazySequentializer<T>>::finalize_callback(
                self,
                Finaly::finaly,
            );
        }
    }

    #[cfg(coff_thread_at_exit)]
    mod windows {
        use super::{Node, ThreadExitSequentializer};
        use crate::{Finaly, Sequential};
        use core::cell::Cell;
        use core::ptr::NonNull;

        use winapi::shared::minwindef::{DWORD, LPVOID};
        use winapi::um::winnt::{DLL_PROCESS_DETACH, DLL_THREAD_DETACH};

        //On thread exit
        //non nul pointers between .CRT$XLA and .CRT$XLZ will be
        //run... => So we could implement thread_local drop without
        //registration...
        #[link_section = ".CRT$XLAZ"] //do this after the standard library
        #[used]
        pub static AT_THEAD_EXIT: extern "system" fn(LPVOID, DWORD, LPVOID) = destroy;

        extern "system" fn destroy(_: LPVOID, reason: DWORD, _: LPVOID) {
            if reason == DLL_THREAD_DETACH || reason == DLL_PROCESS_DETACH {
                let mut o_ptr = REGISTER.take();
                while let Some(r) = o_ptr {
                    // SAFETY ptr must refer to a thread_local static
                    // this is required by ThreadExitSequentializer::new
                    r.execute();
                    o_ptr = r.take_next();
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
        static REGISTER: Cell<Option<&'static Node>> = Cell::new(None);

        #[thread_local]
        static DONE: Cell<bool> = Cell::new(false);

        #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
        /// Store a reference of the thread local static for execution of the
        /// finalize call back at thread exit
        pub fn finalize_at_thread_exit<
            T: Sequential<Sequentializer = ThreadExitSequentializer<IRF>>,
            const IRF: bool,
        >(
            st: &'static T,
        ) -> bool
        where
            T::Data: 'static + Finaly,
        {
            if DONE.get() {
                false
            } else {
                Sequential::sequentializer(st).0.next.set(REGISTER.take());
                REGISTER.set(Some(st as &Node));
                true
            }
        }
    }
    #[cfg(coff_thread_at_exit)]
    pub use windows::finalize_at_thread_exit;

    #[cfg(cxa_thread_at_exit)]
    mod cxa {
        use super::{Node, ThreadExitSequentializer};
        use crate::{Finaly, Sequential};
        use core::cell::Cell;
        use core::ptr::null_mut;

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
        static REGISTER: Cell<Option<&'static Node>> = Cell::new(None);

        #[thread_local]
        static DESTROYING: Cell<bool> = Cell::new(false);

        extern "C" fn execute_destroy(_: *mut u8) {
            DESTROYING.set(true);
            let mut o_r = REGISTER.take();
            while let Some(r) = o_r {
                r.execute();
                o_r = r.take_next().or_else(|| REGISTER.take());
            }
            DESTROYING.set(false);
        }
        #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
        /// Store a reference of the thread local static for execution of the
        /// finalize call back at thread exit
        pub fn finalize_at_thread_exit<
            T: 'static + Sequential<Sequentializer = ThreadExitSequentializer<IRF>>,
            const IRF: bool,
        >(
            st: &'static T,
        ) -> bool
        where
            T::Data: 'static + Finaly,
        {
            let old = REGISTER.take();
            if let Some(old) = old {
                Sequential::sequentializer(st).0.next.set(Some(old));
            } else if !DESTROYING.get() {
                at_thread_exit(execute_destroy, null_mut())
            }
            REGISTER.set(Some(st as &Node));
            true
        }
    }
    #[cfg(cxa_thread_at_exit)]
    pub use cxa::finalize_at_thread_exit;

    #[cfg(pthread_thread_at_exit)]
    mod pthread {
        use super::{Node, ThreadExitSequentializer};
        use crate::{Finaly, Sequential};

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
        static REGISTER: Cell<Option<&'static Node>> = Cell::new(None);

        extern "C" fn execute_destroy(_: *mut c_void) {
            let mut opt_head = REGISTER.take();
            while let Some(r) = opt_head {
                r.execute();
                opt_head = r.take_next().or_else(|| REGISTER.take());
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
        fn register_on_thread_exit<
            T: Sequential<Sequentializer = ThreadExitSequentializer<IRF>>,
            const IRF: bool,
        >(
            st: &'static T,
            key: pthread_key_t,
        ) -> bool
        where
            T::Data: 'static + Finaly,
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

            Sequential::sequentializer(st).0.next.set(REGISTER.take());

            REGISTER.set(Some(st as &Node));
            true
        }

        #[cfg_attr(docsrs, doc(cfg(feature = "thread_local")))]
        /// Store a reference of the thread local static for execution of the
        /// finalize call back at thread exit
        pub fn finalize_at_thread_exit<
            T: Sequential<Sequentializer = ThreadExitSequentializer<IRF>>,
            const IRF: bool,
        >(
            st: &'static T,
        ) -> bool
        where
            T::Data: 'static + Finaly,
        {
            match get_key() {
                Some(key) => register_on_thread_exit(st, key),
                None => false,
            }
        }
    }
    #[cfg(pthread_thread_at_exit)]
    pub use pthread::finalize_at_thread_exit;
}
