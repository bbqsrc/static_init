#![cfg(any(elf, mach_o, coff))]

use super::static_lazy::Once as OnceTrait;
use super::{destructor, ConstDrop};
use core::cell::Cell;
use core::marker::PhantomData;
use core::mem::{forget, transmute};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU8, Ordering};
use parking_lot::{lock_api::RawMutex as _, Mutex, Once, OnceState, RawMutex};

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    /// drop_const call has not yet been registered
    NonRegistered = 0,
    /// drop_const registration is running     
    Registrating  = 1,
    /// drop_const is registered but execution not yet done     
    Registered    = 2,
    /// drop_const is being executed     
    Executing     = 3,
    /// drop_const has been executed     
    Executed      = 4,
    /// drop_const could not been registered and will never
    /// be registered but the data is still valid for use
    Unregistrable = 5,
    /// drop_const executed and paniced
    Poisoned      = 6,
}

pub type SyncNode = dyn 'static + OnExitSync + Sync;

pub type UnSyncNode = dyn 'static + OnExitUnSync;

pub trait OnExit {
    fn execute(&self);
    fn set_status(&self, _: Status);
    fn get_status(&self) -> Status;
}

pub trait OnExitSync: OnExit {
    fn set_next(&'static self, _: Option<&'static SyncNode>);
    fn get_next(&'static self) -> Option<&'static SyncNode>;
}
pub trait OnExitUnSync: OnExit {
    /// # Safety
    /// set_next must have thread_local storage duration
    unsafe fn set_next(&self, _: Option<NonNull<UnSyncNode>>);
    fn get_next(&self) -> Option<NonNull<UnSyncNode>>;
}

pub trait ExitRegisterSync {
    fn register(_: &'static SyncNode) -> bool;
}
pub trait ExitRegisterUnSync {
    /// # Safety
    /// the variable passed as argument must be a thread_local
    unsafe fn register(_: &UnSyncNode) -> bool;
}

pub trait StatusHolder {
    const INIT: Self;
    fn set_status(&self, _: Status);
    fn get_status(&self) -> Status;
}

pub trait NodeSync {
    fn set_next(&'static self, _: Option<&'static SyncNode>);
    fn get_next(&'static self) -> Option<&'static SyncNode>;
}
pub trait NodeUnSync {
    /// # Safety
    /// set_next must have thread_local storage duration
    unsafe fn set_next(&self, _: Option<NonNull<UnSyncNode>>);
    fn get_next(&self) -> Option<NonNull<UnSyncNode>>;
}

pub struct AtExit<Data, StH, ER> {
    data:    Data,
    manager: StH,
    phantom: PhantomData<ER>,
}
struct OnPanic<'a, StH: StatusHolder>(&'a StH, Status);
impl<'a, StH: StatusHolder> Drop for OnPanic<'a, StH> {
    fn drop(&mut self) {
        self.0.set_status(self.1)
    }
}

impl<Data: ConstDrop, StH: StatusHolder, ER> OnExit for AtExit<Data, StH, ER> {
    fn execute(&self) {
        self.manager.set_status(Status::Executing);

        let guard = OnPanic(&self.manager, Status::Poisoned);

        self.data.const_drop();

        forget(guard);

        self.manager.set_status(Status::Executed);
    }

    fn set_status(&self, s: Status) {
        self.manager.set_status(s);
    }
    fn get_status(&self) -> Status {
        self.manager.get_status()
    }
}

impl<Data: ConstDrop, StH: NodeSync + StatusHolder, ER> OnExitSync for AtExit<Data, StH, ER> {
    fn set_next(&'static self, node: Option<&'static SyncNode>) {
        self.manager.set_next(node)
    }
    fn get_next(&'static self) -> Option<&'static SyncNode> {
        self.manager.get_next()
    }
}

impl<Data: ConstDrop, StH: NodeUnSync + StatusHolder, ER> OnExitUnSync for AtExit<Data, StH, ER> {
    unsafe fn set_next(&self, node: Option<NonNull<UnSyncNode>>) {
        self.manager.set_next(node)
    }
    fn get_next(&self) -> Option<NonNull<UnSyncNode>> {
        self.manager.get_next()
    }
}

impl<Data, StH: StatusHolder, ER> AtExit<Data, StH, ER> {
    pub fn status(&self) -> Status {
        self.manager.get_status()
    }
}

impl<
        Data: ConstDrop + Sync,
        StH: Sync + NodeSync + StatusHolder + OnceTrait,
        ER: Sync + ExitRegisterSync,
    > AtExit<Data, StH, ER>
{
    pub fn register_sync(&'static self) -> Result<(), Status> {
        match self.manager.state() {
            OnceState::New => {
                self.manager.call_once(|| {
                    let guard = OnPanic(&self.manager, Status::Unregistrable);

                    if ER::register(self) {
                        self.manager.set_status(Status::Registered);
                        forget(guard)
                    }
                });
                Ok(())
            }
            OnceState::InProgress => Err(Status::Registrating),
            OnceState::Done => Err(self.status()),
            OnceState::Poisoned => Err(Status::Unregistrable),
        }
    }
}
impl<
        Data: 'static + ConstDrop,
        StH: 'static + NodeUnSync + StatusHolder,
        ER: 'static + ExitRegisterUnSync,
    > AtExit<Data, StH, ER>
{
    pub unsafe fn register_unsync(&self) -> Result<(), Status> {
        let status = self.manager.get_status();
        if status == Status::NonRegistered {
            let guard = OnPanic(&self.manager, Status::Unregistrable);

            if ER::register(self) {
                self.manager.set_status(Status::Registered);
                forget(guard)
            }
            Ok(())
        } else {
            Err(status)
        }
    }
}

mod global_register {
    use super::*;

    #[destructor(0)]
    extern "C" fn execute_at_exit2() {
        let mut l = REGISTER.lock();
        let mut list: Option<&SyncNode> = l.0.take();
        drop(l);
        while let Some(on_exit) = list {
            on_exit.execute();
            list = on_exit.get_next().or_else(|| {
                let mut l = REGISTER.lock();
                if l.0.is_none() {
                    l.1 = true;
                }
                l.0.take()
            });
        }
    }

    static REGISTER: Mutex<(Option<&'static SyncNode>, bool)> =
        Mutex::const_new(RawMutex::INIT, (None, true));

    pub struct GlobalRegister;

    /// Opaque type used for registration management
    /// To be used with GlobalRegister
    pub struct AtExitManaged {
        next:   Cell<Option<&'static SyncNode>>,
        once:   Once,
        status: AtomicU8,
    }

    impl ExitRegisterSync for GlobalRegister {
        fn register(node: &'static SyncNode) -> bool {
            node.set_status(Status::Registrating);
            let mut l = REGISTER.lock();
            if l.1 {
                node.set_next(l.0.take());
                *l = (Some(node), true);
                true
            } else {
                false
            }
        }
    }

    //All access of next are done when REGISTER2 is locked
    unsafe impl Sync for AtExitManaged {}

    impl StatusHolder for AtExitManaged {
        const INIT: Self = Self {
            status: AtomicU8::new(Status::NonRegistered as u8),
            once:   Once::new(),
            next:   Cell::new(None),
        };
        fn set_status(&self, s: Status) {
            self.status.store(s as u8, Ordering::Release);
        }
        fn get_status(&self) -> Status {
            unsafe { transmute(self.status.load(Ordering::Acquire)) }
        }
    }
    impl NodeSync for AtExitManaged {
        fn set_next(&'static self, node: Option<&'static SyncNode>) {
            assert!(REGISTER.is_locked());
            self.next.set(node)
        }
        fn get_next(&'static self) -> Option<&'static SyncNode> {
            assert!(REGISTER.is_locked());
            self.next.get()
        }
    }
    impl OnceTrait for AtExitManaged {
        fn call_once<F: FnOnce()>(&self, f: F) {
            self.once.call_once(f)
        }
        fn state(&self) -> OnceState {
            self.once.state()
        }
    }
}
pub use global_register::{AtExitManaged, GlobalRegister};

/// An opagque type used to managed "at thread exit" registration
pub struct LocalExitManager {
    status: Cell<Status>,
    next:   Cell<Option<NonNull<UnSyncNode>>>,
}

impl StatusHolder for LocalExitManager {
    const INIT: Self = Self {
        status: Cell::new(Status::NonRegistered),
        next:   Cell::new(None),
    };
    fn set_status(&self, s: Status) {
        self.status.set(s);
    }
    fn get_status(&self) -> Status {
        self.status.get()
    }
}
impl NodeUnSync for LocalExitManager {
    /// #Safety
    /// Node must last long enough
    unsafe fn set_next(&self, node: Option<NonNull<UnSyncNode>>) {
        self.next.set(node)
    }
    fn get_next(&self) -> Option<NonNull<UnSyncNode>> {
        self.next.get()
    }
}

//#[cfg(coff_thread_at_exit)]
mod windows {
    use super::{ExitRegisterUnSync, OnExitUnSync, UnSyncNode,Status};
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
    static REGISTER: Cell<Option<NonNull<dyn OnExitUnSync>>> = Cell::new(None);

    #[thread_local]
    static DONE: Cell<bool> = Cell::new(false);

    fn register(r: &UnSyncNode) -> bool {
        if DONE.get() {
            false
        } else {
            unsafe { r.set_next(REGISTER.take()) };
            REGISTER.set(Some(r.into()));
            true
        }
    }

    pub struct LocalRegister;
    impl ExitRegisterUnSync for LocalRegister {
        unsafe fn register(r: &UnSyncNode) -> bool {
            r.set_status(Status::Registrating);
            register(r)
        }
    }
}

//#[cfg(cxa_thread_at_exit)]
mod cxa {
    use super::{ExitRegisterUnSync, UnSyncNode,Status};
    use core::cell::Cell;
    use core::ptr::{self, NonNull};
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
    static REGISTER: Cell<Option<NonNull<UnSyncNode>>> = Cell::new(None);

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
    fn register(r: &UnSyncNode) -> bool {
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
    impl ExitRegisterUnSync for LocalRegister {
        unsafe fn register(r: &UnSyncNode) -> bool {
            r.set_status(Status::Registrating);
            register(r)
        }
    }
}

//#[cfg(pthread_thread_at_exit)]
mod pthread {
    use super::{ExitRegisterUnSync, UnSyncNode,Status};
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
    static REGISTER: Cell<Option<NonNull<UnSyncNode>>> = Cell::new(None);

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
    fn init() -> bool {
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
                    return false;
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
        return true;
    }
    fn register_on_thread_exit(r: &UnSyncNode) -> bool {
        let key = {
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
                        panic!(
                            "Unable to allocate a pthread_key for thread local destructor \
                             registration"
                        );
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
            key as pthread_key_t
        };

        let specific = unsafe { pthread_getspecific(key) };

        if specific.is_null() {
            if ITERATION_COUNT.get() < _POSIX_THREAD_DESTRUCTOR_ITERATIONS {
                assert_eq!(
                    unsafe { pthread_setspecific(key, NonNull::dangling().as_ptr()) },
                    0
                );
                ITERATION_COUNT.set(ITERATION_COUNT.get() + 1);
            } else {
                //it is not guaranted by posix that destructor will be run
                //so refuse registration
                return false;
            }
        }

        unsafe { r.set_next(REGISTER.take()) };

        REGISTER.set(Some(r.into()));

        true
    }
    pub struct LocalRegister;
    impl ExitRegisterUnSync for LocalRegister {
        unsafe fn register(r: &UnSyncNode) -> bool {
            if init() {
                r.set_status(Status::Registrating);
                register_on_thread_exit(r)
            } else {
                false
            }
        }
    }
}

///// At type to store a handler that is called with `data` as argument if the function register has
///// been called before main exit.
/////
///// The handler is called just after main exit.
/////
///// This is equivalent to a local variable declared in main body but that can be declared as
///// a static variable.
/////
///// Contrarily to `libc::at_exit` registration of functions does not involve any memory allocation.
//pub struct AtExit<T: 'static> {
//    pub data:    T,
//    pub managed: AtExitManaged,
//}
//
///// As AtExit but needs an external mechanism to ensure
///// registration is not performed more that once and in a
///// thread safe maner.
//pub struct UnguardedAtExit<T: 'static> {
//    pub data:    T,
//    pub managed: UnguardedAtExitManaged,
//}
//
///// Opaque type used for registration management
//pub struct AtExitManaged {
//    next:   Cell<Option<OnExitRef>>,
//    once:   Once,
//    status: AtomicU8,
//}
//
///// As AtExitManaged but for UnguardedAtExit
//pub struct UnguardedAtExitManaged {
//    next:   Cell<Option<OnExitRef>>,
//    status: AtomicU8,
//}
//
//type OnExitRef = &'static (dyn OnExit + Sync);
//
///// Used to const init `managed` field of `AtExit`
//pub const COMPLETE_INIT: AtExitManaged = AtExitManaged {
//    next:   Cell::new(None),
//    once:   Once::new(),
//    status: AtomicU8::new(0),
//};
//
///// Used to const init `managed` field of `AtExit`
//pub const UNGUARDED_COMPLETE_INIT: UnguardedAtExitManaged = UnguardedAtExitManaged {
//    next:   Cell::new(None),
//    status: AtomicU8::new(0),
//};
//
//trait OnExit {
//    fn execute(&'static self);
//    fn get_next(&'static self) -> Option<OnExitRef>;
//}
//
//impl<T: 'static + ConstDrop> OnExit for AtExit<T> {
//    fn execute(&'static self) {
//        //debug_assert!(self.status.get() == Status::Registered);
//        self.managed
//            .status
//            .store(Status::Executing as u8, Ordering::Relaxed);
//        struct OnPanic<'a>(&'a AtomicU8);
//        impl<'a> Drop for OnPanic<'a> {
//            fn drop(&mut self) {
//                self.0.store(Status::Poisoned as u8, Ordering::Release)
//            }
//        }
//        let guard = OnPanic(&self.managed.status);
//        self.data.const_drop();
//        forget(guard);
//        self.managed
//            .status
//            .store(Status::Executed as u8, Ordering::Release);
//    }
//    fn get_next(&'static self) -> Option<OnExitRef> {
//        //protected by REGISTER mutex.
//        unsafe { *self.managed.next.as_ptr() }
//    }
//}
//
//impl<T: 'static + ConstDrop> OnExit for UnguardedAtExit<T> {
//    fn execute(&'static self) {
//        //debug_assert!(self.status.get() == Status::Registered);
//        self.managed
//            .status
//            .store(Status::Executing as u8, Ordering::Relaxed);
//        struct OnPanic<'a>(&'a AtomicU8);
//        impl<'a> Drop for OnPanic<'a> {
//            fn drop(&mut self) {
//                self.0.store(Status::Poisoned as u8, Ordering::Release)
//            }
//        }
//        let guard = OnPanic(&self.managed.status);
//        self.data.const_drop();
//        forget(guard);
//        self.managed
//            .status
//            .store(Status::Executed as u8, Ordering::Release);
//    }
//    fn get_next(&'static self) -> Option<OnExitRef> {
//        //protected by REGISTER mutex.
//        unsafe { *self.managed.next.as_ptr() }
//    }
//}
//
//static REGISTER: Mutex<Option<OnExitRef>> = Mutex::const_new(RawMutex::INIT, None);
//
//#[destructor(0)]
//extern "C" fn execute_at_exit() {
//    let mut l = REGISTER.lock();
//    let mut list: Option<OnExitRef> = l.take();
//    drop(l);
//    while let Some(on_exit) = list {
//        on_exit.execute();
//        list = on_exit.get_next().or_else(|| {
//            let mut l = REGISTER.lock();
//            l.take()
//        });
//    }
//}
//impl<T: Sync + ConstDrop> AtExit<T> {
//    /// Return the current status
//    ///
//    /// Ordering may be Relaxed or Acquire. If the
//    /// status is Executed, this method call will synchronize
//    /// with the end of the execution of const_drop.
//    pub fn status(&self, order: Ordering) -> Status {
//        unsafe { core::mem::transmute(self.managed.status.load(order)) }
//    }
//
//    /// Register the current object for call of on_thread_exit at
//    /// thread destruction.
//    ///
//    /// Return an error if it is already registered or if registration
//    /// on progress in an other thread.
//    ///
//    /// If the error returned is Registered or Executing or Executed or if
//    /// the result if Ok(()) this call will synchronize with the registration
//    /// (which even if the result is Ok(()) could actualy have happen in an other thread.
//    ///
//    /// If the result is Err(Status::Executed) this call also synchronize with the en
//    /// of the execution of drop_const.
//    pub fn register(&'static self) -> Result<(), Status> {
//        match self.managed.once.state() {
//            OnceState::New => {
//                self.managed.once.call_once(|| {
//                    let mut reg = REGISTER.lock();
//                    self.managed.next.set(reg.take());
//                    *reg = Some(self as &_);
//                    self.managed
//                        .status
//                        .store(Status::Registered as u8, Ordering::Relaxed);
//                });
//                Ok(())
//            }
//            OnceState::InProgress => Err(Status::Registrating),
//            OnceState::Done => Err(self.status(Ordering::Acquire)),
//            OnceState::Poisoned => Err(Status::Unregistrable),
//        }
//    }
//}
//unsafe impl<T: Sync> Sync for AtExit<T> {}
//
//impl<T: ConstDrop + Sync + 'static> UnguardedAtExit<T> {
//    /// Return the current status
//    ///
//    /// Ordering may be Relaxed or Acquire. If the
//    /// status is Executed, this method call will synchronize
//    /// with the end of the execution of const_drop.
//    pub fn status(&self, order: Ordering) -> Status {
//        unsafe { core::mem::transmute(self.managed.status.load(order)) }
//    }
//
//    /// # Safety
//    /// As for AtExit but not guarded for once execution
//    /// and user must ensure it is a static!
//    pub unsafe fn register(&mut self) -> Result<(), Status> {
//        let mut reg = REGISTER.lock();
//        self.managed.next.set(reg.take());
//        *reg = Some(&*(self as *const _) as OnExitRef);
//        drop(reg);
//        self.managed
//            .status
//            .store(Status::Registered as u8, Ordering::Relaxed);
//        Ok(())
//    }
//}
//unsafe impl<T: Sync> Sync for UnguardedAtExit<T> {}
