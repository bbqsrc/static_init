#![cfg(any(elf, mach_o, coff))]

use super::once::{Once, GlobalOnce};
use super::{destructor, Finaly, Manager, Phase, Static};
use core::cell::Cell;
use core::marker::PhantomData;
use core::mem::{forget, transmute};
use core::ptr::NonNull;
use parking_lot::{lock_api::RawMutex as _, Mutex, Once as PkOnce, OnceState, RawMutex};

pub type AtExit = GenericAtExit<GlobalManager<PkOnce>, GlobalRegister>;
pub type AtGlobalExit = GenericAtExit<GlobalManager<GlobalOnce>, GlobalRegister>;
pub type AtThreadExit = GenericAtExit<LocalManager, LocalRegister>;

pub struct GenericAtExit<StH, ER> {
    manager: StH,
    phantom: PhantomData<ER>,
}

type Node = NonNull<dyn 'static + OnExit>;

pub trait OnExit {
    fn execute(&self);
    /// #Safety
    /// shall only be called by the unique instance of ExitRegister.
    unsafe fn set_next(&self, _: Option<Node>);
    fn get_next(&self) -> Option<Node>;
}

pub trait ExitRegister {
    fn register(_: &(dyn 'static + OnExit)) -> bool;
}

pub trait StatusHolder {
    fn set_status(&self, _: Phase);
    fn get_status(&self) -> Phase;
    fn set_next(&self, _: Option<Node>);
    fn get_next(&self) -> Option<Node>;
}

struct OnPanic<'a, StH: StatusHolder>(&'a StH, Phase);
impl<'a, StH: StatusHolder> Drop for OnPanic<'a, StH> {
    fn drop(&mut self) {
        self.0.set_status(self.1)
    }
}

impl<StH, ER> GenericAtExit<StH, ER> {
    pub const unsafe fn new(sth: StH) -> Self {
        Self {
            manager: sth,
            phantom: PhantomData,
        }
    }
}

impl<
        Data: Finaly + 'static,
        StH: StatusHolder + 'static,
        ER: 'static,
        T: Static<Data = Data, Manager = GenericAtExit<StH, ER>>,
    > OnExit for T
{
    fn execute(&self) {
        let at_exit = Static::manager(self);

        let guard = OnPanic(&at_exit.manager, Phase::PostFinalyExecutionPanic);
        at_exit.manager.set_status(Phase::FinalyExecution);
        Finaly::finaly(Static::data(self));
        forget(guard);

        at_exit.manager.set_status(Phase::Finalized);
    }
    unsafe fn set_next(&self, node: Option<Node>) {
        Static::manager(self).manager.set_next(node)
    }
    fn get_next(&self) -> Option<Node> {
        Static::manager(self).manager.get_next()
    }
}

impl<
        Data: Finaly + 'static,
        S: Static<Data = Data, Manager = Self>,
        StH: StatusHolder + 'static + Once,
        ER: 'static + ExitRegister,
    > Manager<S> for GenericAtExit<StH, ER>
{
    fn phase(&self) -> Phase {
        self.manager.get_status()
    }

    fn register(st: &S, init: impl FnOnce(&Data) -> bool, on_reg_failure: impl FnOnce(&Data)) {
        let this = Static::manager(st);
        this.manager.call_once(|| {
            let guard = OnPanic(&this.manager, Phase::PostInitializationPanic);
            this.manager.set_status(Phase::Initialization);
            let register = init(Static::data(st));
            forget(guard);

            if register {
                this.manager.set_status(Phase::FinalyRegistration);
                let guard = OnPanic(&this.manager, Phase::PostFinalyRegistrationFailure);
                let r = st as &(dyn 'static + OnExit);
                let cond = ER::register(r);
                forget(guard);
                if cond {
                    this.manager.set_status(Phase::Initialized);
                } else {
                    this.manager
                        .set_status(Phase::PostFinalyRegistrationFailure);
                    let guard = OnPanic(&this.manager, Phase::OnRegistrationFailurePanic);
                    on_reg_failure(Static::data(st));
                    forget(guard);
                    this.manager
                        .set_status(Phase::InitializedPostOnRegistrationFailure)
                }
            } else {
                this.manager.set_status(Phase::InitializedWithoutFinaly);
            }
        });
    }
}

mod global_register {
    use super::*;
    use crate::AtomicPhase;

    #[destructor(0)]
    extern "C" fn execute_at_exit2() {
        let mut l = REGISTER.lock();
        let mut list: Option<Node> = l.0.take().map(|n| n.0);
        drop(l);
        while let Some(mut on_exit) = list {
            let r = unsafe { on_exit.as_mut() };
            r.execute();
            list = r.get_next().or_else(|| {
                let mut l = REGISTER.lock();
                if l.0.is_none() {
                    l.1 = true;
                }
                l.0.take().map(|n| n.0)
            });
        }
    }
    struct NodeWrapper(Node);
    unsafe impl Send for NodeWrapper {}

    static REGISTER: Mutex<(Option<NodeWrapper>, bool)> =
        Mutex::const_new(RawMutex::INIT, (None, true));

    pub struct GlobalRegister;

    /// Opaque type used for registration management
    /// To be used with GlobalRegister
    pub struct GlobalManager<O> {
        next:   Cell<Option<Node>>,
        once:   O,
        status: AtomicPhase,
    }

    const GLOBAL_PK_INIT: GlobalManager<PkOnce> = GlobalManager {
        status: AtomicPhase::new(),
        once:   PkOnce::new(),
        next:   Cell::new(None),
    };
    const GLOBAL_INIT: GlobalManager<GlobalOnce> = GlobalManager {
        status: AtomicPhase::new(),
        once:   unsafe{GlobalOnce::new()},
        next:   Cell::new(None),
    };
    impl GlobalManager<GlobalOnce> {
        pub const unsafe fn new() -> Self {
            GLOBAL_INIT
        }
    }
    impl GlobalManager<PkOnce> {
        pub const unsafe fn new_pk() -> Self {
            GLOBAL_PK_INIT
        }
    }

    impl ExitRegister for GlobalRegister {
        fn register(node: &(dyn 'static + OnExit)) -> bool {
            let mut l = REGISTER.lock();
            if l.1 {
                unsafe { node.set_next(l.0.take().map(|n| n.0)) };
                *l = (Some(NodeWrapper(node.into())), true);
                true
            } else {
                false
            }
        }
    }

    //All access of next are done when REGISTER2 is locked
    unsafe impl<O:Sync> Sync for GlobalManager<O> {}

    impl<O> StatusHolder for GlobalManager<O> {
        fn set_status(&self, s: Phase) {
            self.status.set(s);
        }
        fn get_status(&self) -> Phase {
            unsafe { transmute(self.status.get()) }
        }
        fn set_next(&self, node: Option<Node>) {
            assert!(REGISTER.is_locked());
            self.next.set(node)
        }
        fn get_next(&self) -> Option<Node> {
            assert!(REGISTER.is_locked());
            self.next.get()
        }
    }
    impl<O:Once> Once for GlobalManager<O> {
        fn call_once<F: FnOnce()>(&self, f: F) {
            self.once.call_once(f)
        }
        fn state(&self) -> OnceState {
            self.once.state()
        }
    }
}
pub use global_register::GlobalManager;
use global_register::GlobalRegister;

/// An opagque type used to managed "at thread exit" registration
pub struct LocalManager {
    status: Cell<Phase>,
    next:   Cell<Option<Node>>,
}

const LOCAL_INIT: LocalManager = LocalManager {
    status: Cell::new(Phase::New),
    next:   Cell::new(None),
};

impl LocalManager {
    pub const unsafe fn new() -> Self {
        LOCAL_INIT
    }
}
impl Once for LocalManager {
    fn state(&self) -> OnceState {
        match self.status.get() {
            Phase::New => OnceState::New,
            Phase::Initialization | Phase::FinalyRegistration => OnceState::InProgress,
            Phase::Initialized
            | Phase::FinalyExecution
            | Phase::Finalized
            | Phase::InitializedWithoutFinaly
            | Phase::InitializedPostOnRegistrationFailure
            | Phase::PostFinalyRegistrationFailure => OnceState::Done,
            Phase::PostInitializationPanic
            | Phase::OnRegistrationFailurePanic
            | Phase::PostFinalyExecutionPanic => OnceState::Poisoned,
        }
    }
    fn call_once<F: FnOnce()>(&self, f: F) {
        if self.status.get() == Phase::New {
            f();
            assert_ne!(self.status.get(), Phase::New);
        }
    }
}

impl StatusHolder for LocalManager {
    fn set_status(&self, s: Phase) {
        self.status.set(s);
    }
    fn get_status(&self) -> Phase {
        self.status.get()
    }
    /// #Safety
    /// Node must last long enough
    fn set_next(&self, node: Option<Node>) {
        self.next.set(node)
    }
    fn get_next(&self) -> Option<Node> {
        self.next.get()
    }
}

//#[cfg(coff_thread_at_exit)]
mod windows {
    use super::{ExitRegister, Node, OnExit};
    use core::cell::Cell;

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
    static REGISTER: Cell<Option<Node>> = Cell::new(None);

    #[thread_local]
    static DONE: Cell<bool> = Cell::new(false);

    fn register(r: &(dyn 'static + OnExit)) -> bool {
        if DONE.get() {
            false
        } else {
            unsafe { r.set_next(REGISTER.take()) };
            REGISTER.set(Some(r.into()));
            true
        }
    }

    pub struct LocalRegister;
    impl ExitRegister for LocalRegister {
        fn register(r: &(dyn 'static + OnExit)) -> bool {
            register(r)
        }
    }
}
#[cfg(coff_thread_at_exit)]
use windows::LocalRegister;

#[cfg(cxa_thread_at_exit)]
mod cxa {
    use super::{ExitRegister, Node, OnExit};
    use core::cell::Cell;
    use core::ptr;
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
    static REGISTER: Cell<Option<Node>> = Cell::new(None);

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
    fn register(r: &(dyn 'static + OnExit)) -> bool {
        let old = REGISTER.take();
        if let Some(old) = old {
            unsafe { r.set_next(Some(old)) };
        } else if !DESTROYING.get() {
            at_thread_exit(execute_destroy, ptr::null_mut())
        }
        REGISTER.set(Some(r.into()));
        true
    }

    pub struct LocalRegister;
    impl ExitRegister for LocalRegister {
        fn register(r: &(dyn 'static + OnExit)) -> bool {
            register(r)
        }
    }
}
#[cfg(cxa_thread_at_exit)]
use cxa::LocalRegister;

//#[cfg(pthread_thread_at_exit)]
mod pthread {
    use super::{ExitRegister, Node, OnExit};
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
    static REGISTER: Cell<Option<Node>> = Cell::new(None);

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
    fn register_on_thread_exit(r: &(dyn 'static + OnExit), key: pthread_key_t) -> bool {
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

        unsafe { r.set_next(REGISTER.take()) };

        REGISTER.set(Some(r.into()));
        true
    }

    pub struct LocalRegister;
    impl ExitRegister for LocalRegister {
        fn register(r: &(dyn 'static + OnExit)) -> bool {
            match get_key() {
                Some(key) => register_on_thread_exit(r, key),
                None => false,
            }
        }
    }
}
#[cfg(pthread_thread_at_exit)]
use pthread::LocalRegister;
