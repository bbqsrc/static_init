use super::{ManagerBase, OnceManager, Static};
use core::cell::Cell;
use core::mem::forget;

#[cfg(debug_mode)]
use super::CyclicPanic;

mod phase {
    pub(super) const INITED_BIT: u32 = 1;
    pub(super) const INITIALIZING_BIT: u32 = 2 * INITED_BIT;
    pub(super) const INIT_SKIPED_BIT: u32 = 2 * INITIALIZING_BIT;
    pub(super) const LOCKED_BIT: u32 = 2 * INIT_SKIPED_BIT;
    pub(super) const PARKED_BIT: u32 = 2 * LOCKED_BIT;
    pub(super) const REGISTRATING_BIT: u32 = 2 * PARKED_BIT;
    pub(super) const REGISTERED_BIT: u32 = 2 * REGISTRATING_BIT;
    pub(super) const FINALIZING_BIT: u32 = 2 * REGISTERED_BIT;
    pub(super) const FINALIZED_BIT: u32 = 2 * FINALIZING_BIT;
    pub(super) const FINALIZATION_PANIC_BIT: u32 = 2 * FINALIZED_BIT;

    #[derive(Debug, Copy, Clone, Eq, PartialEq)]
    pub struct Phase(pub(super) u32);

    impl Phase {
        pub const fn new() -> Self {
            Self(0)
        }
        pub fn initial_state(self) -> bool {
            self.0 == 0
        }
        /// registration of finaly impossible or initialization paniced
        pub fn initialization_skiped(self) -> bool {
            self.0 & INIT_SKIPED_BIT != 0
        }
        pub fn registration_attempt_done(self) -> bool {
            self.0 & REGISTERED_BIT != 0
        }
        pub fn unregistrable(self) -> bool {
            self.0 == REGISTERED_BIT | INIT_SKIPED_BIT
        }
        pub fn initialized(self) -> bool {
            self.0 & INITED_BIT != 0
        }
        pub fn finalized(self) -> bool {
            self.0 & FINALIZED_BIT != 0
        }

        pub fn registrating(self) -> bool {
            self.0 & REGISTRATING_BIT != 0  && !self.initialization_skiped()
        }
        pub fn registration_panic(self) -> bool {
            self.0 & REGISTRATING_BIT != 0 && self.initialization_skiped()
        }

        pub fn initializing(self) -> bool {
            self.0 & INITIALIZING_BIT != 0  && !self.initialization_skiped()
        }
        pub fn initialization_panic(self) -> bool {
            self.0 & INITIALIZING_BIT != 0  && self.initialization_skiped()
        }

        pub fn finalizing(self) -> bool {
            self.0 & FINALIZING_BIT != 0
        }
        pub fn finalization_panic(self) -> bool {
            self.0 & FINALIZATION_PANIC_BIT != 0
        }

    }
}
pub use phase::Phase;

pub struct LocalManager(Cell<Phase>);

impl LocalManager {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(Cell::new(Phase::new()))
    }
}

impl ManagerBase for LocalManager {
    fn phase(&self) -> Phase {
        self.0.get()
    }
}

struct Guard<'a>(&'a Cell<Phase>, Phase);
impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        self.0.set(self.1)
    }
}

#[inline(never)]
#[cold]
fn register_uninited<T: Static>(
    this: &LocalManager,
    s: &T,
    init: impl FnOnce(&<T as Static>::Data),
    reg: impl FnOnce(&T) -> bool,
    init_on_reg_failure: bool,
) {
    use phase::*;

    //states:
    // 1) 0
    // 2) REGISTRATING|LOCKED_BIT (|PARKED_BIT)
    // 3)    REGISTRATING|INIT_SKIPED (final)
    //    a) REGISTRATING|REGISTERED|INITIALIZING|LOCKED (|PARKED)
    //    b) REGISTRATING|INITIALIZING|LOCKED (|PARKED)
    //       REGISTERED|INIT_SKIPED (final)
    // branch a):
    // 4) REGISTRATING|REGISTERED|INITIALIZING|INIT_SKIPED (final)
    //    REGISTRATING|REGISTERED|INITIALIZED
    // 5) REGISTRATING|REGISTERED|INITIALIZED|FINALIZING
    // 6) REGISTRATING|REGISTERED|INITIALIZED|FINALIZED (final)
    //    REGISTRATING|REGISTERED|INITIALIZED|FINALIZATION_PANIC(final)
    // branch b):
    // 4) REGISTRATING|INITIALIZING|INIT_SKIPED (final)
    //    REGISTRATING|INITIALIZED (final)
    // 5) REGISTRATING|INITIALIZED|FINALIZING (if manualy finalize)
    // 6) REGISTRATING|INITIALIZED|FINALIZED (final)
    //    REGISTRATING|INITIALIZED|FINALIZATION_PANIC(final)

    this.0.set(Phase(REGISTRATING_BIT));
    let guard = Guard(&this.0, Phase(REGISTRATING_BIT | INIT_SKIPED_BIT));
    let cond = reg(s);
    forget(guard);

    if cond {
        this.0
            .set(Phase(INITIALIZING_BIT | REGISTERED_BIT));
        let guard = Guard(
            &this.0,
            Phase(REGISTERED_BIT | INITIALIZING_BIT | INIT_SKIPED_BIT),
        );

        init(Static::data(s));

        forget(guard);
        this.0
            .set(Phase(INITED_BIT | REGISTERED_BIT));
    } else if init_on_reg_failure {
        this.0
            .set(Phase(INITIALIZING_BIT));
        let guard = Guard(
            &this.0,
            Phase(INITIALIZING_BIT | INIT_SKIPED_BIT),
        );

        init(Static::data(s));

        forget(guard);

        this.0
            .set(Phase(INITED_BIT));

    } else {
        this.0.set(Phase(REGISTERED_BIT|INIT_SKIPED_BIT));
    }
}

unsafe impl<T: Static> OnceManager<T> for LocalManager
where
    T::Manager: AsRef<LocalManager>,
{
    #[inline(always)]
    fn register(
        s: &T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Static>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool
    ) -> bool {
        let this = Static::manager(s).as_ref();

        let cur = this.0.get();

        if shall_proceed(cur) {
            #[cfg(debug_mode)]
            {
                if cur.initializing() || cur.registrating() {
                    std::panic::panic_any(CyclicPanic);
                }
            }
            register_uninited(this, s, init, reg, init_on_reg_failure);
            shall_proceed(this.0.get())
        } else {
            false
        }
    }

    fn finalize(s: &T, f: impl FnOnce(&T::Data)) {
        use phase::*;

        let this = Static::manager(s).as_ref();

        struct Guard<'a>(&'a Cell<Phase>);
        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                // Mark the state as poisoned, unlock it and unpark all threads.
                let p = self.0;
                p.set(Phase(p.get().0 ^ (FINALIZING_BIT | FINALIZATION_PANIC_BIT)));
            }
        }
        if this.0.get().0 & INIT_SKIPED_BIT == 0 {
            assert_eq!(this.0.get().0 & (FINALIZING_BIT | FINALIZED_BIT | FINALIZATION_PANIC_BIT),0);
            this.0.set(Phase(
                this.0.get().0 | FINALIZING_BIT,
            ));

            let guard = Guard(&this.0);

            f(Static::data(s));

            forget(guard);

            this.0.set(Phase( this.0.get().0 ^ (FINALIZING_BIT |FINALIZED_BIT)));
        }     
    }
}

#[cfg(feature="global_once")]
mod global_once {
use super::{Phase,phase,ManagerBase, OnceManager, Static};
use core::mem::forget;
use core::sync::atomic::{fence, AtomicU32, Ordering};

#[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
mod inited {

    use core::sync::atomic::{AtomicBool, Ordering};

    static LAZY_INIT_ENSURED: AtomicBool = AtomicBool::new(false);

    #[static_init_macro::constructor(__lazy_init_finished)]
    extern "C" fn mark_inited() {
        LAZY_INIT_ENSURED.store(true, Ordering::Release);
    }

    #[inline(always)]
    pub(crate) fn global_inited_hint() -> bool {
        LAZY_INIT_ENSURED.load(Ordering::Acquire)
    }
}


#[cfg(debug_mode)]
use super::CyclicPanic;
#[cfg(debug_mode)]
use core::sync::atomic::AtomicUsize;

#[inline(never)]
#[cold]
fn atomic_register_uninited<'a, T: Static, const GLOBAL: bool>(
    this: &'a GlobalManager<GLOBAL>,
    s: &T,
    shall_proceed: impl Fn(Phase) -> bool,
    init: impl FnOnce(&<T as Static>::Data),
    reg: impl FnOnce(&T) -> bool,
    init_on_reg_failure: bool,
    park: impl Fn(),
    unpark: impl Fn() + Copy,
    #[cfg(debug_mode)]
    id: &AtomicUsize
) -> bool {
    use phase::*;

    use parking_lot_core::SpinWait;

    let mut spin_wait = SpinWait::new();

    let mut cur = this.0.load(Ordering::Relaxed);

    loop {
        if !shall_proceed(Phase(cur)) {
            fence(Ordering::Acquire);
            return false;
        }
        if cur & LOCKED_BIT == 0 {
            match this.0.compare_exchange_weak(
                cur,
                (cur | LOCKED_BIT | REGISTRATING_BIT) & !INIT_SKIPED_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => cur = x,
            }
            continue;
        }
        if cur & PARKED_BIT == 0 && spin_wait.spin() {
            cur = this.0.load(Ordering::Relaxed);
            continue;
        }
        if cur & PARKED_BIT == 0 {
            if let Err(x) = this.0.compare_exchange_weak(
                cur,
                cur | PARKED_BIT,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                cur = x;
                continue;
            }
        }
        #[cfg(debug_mode)]
        {
        let id = id.load(Ordering::Relaxed);
        if id != 0 {
            use parking_lot::lock_api::GetThreadId;
            if id == parking_lot::RawThreadId.nonzero_thread_id().into() {
                std::panic::panic_any(CyclicPanic);
            }
        }
        }

        park();
        spin_wait.reset();
        cur = this.0.load(Ordering::Relaxed);
    }

    let _debug_guard = 
    {
        #[cfg(debug_mode)]
        {
        use parking_lot::lock_api::GetThreadId;
        id.store(parking_lot::RawThreadId.nonzero_thread_id().into(),Ordering::Relaxed); 
        struct Guard<'a>(&'a AtomicUsize);
        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                self.0.store(0,Ordering::Relaxed);
            }
        }
        Guard(id)
        }
        #[cfg(not(debug_mode))]
        ()
    };

    struct UnparkGuard<'a, F: Fn(), const G: bool>(&'a GlobalManager<G>, F, u32);
    impl<'a, F: Fn(), const G: bool> Drop for UnparkGuard<'a, F, G> {
        fn drop(&mut self) {
            // Mark the state as poisoned, unlock it and unpark all threads.
            let man = self.0;
            let cur = man.0.swap(self.2, Ordering::Release);
            if cur & PARKED_BIT != 0 {
                self.1();
            }
        }
    }

    struct Guard<'a, const G: bool>(&'a GlobalManager<G>, u32);
    impl<'a, const G: bool> Drop for Guard<'a, G> {
        fn drop(&mut self) {
            // Mark the state as poisoned, unlock it and unpark all threads.
            let man = self.0;
            man.0.store(self.1, Ordering::Release);
        }
    }
    //states:
    // 1) 0
    // 2) REGISTRATING|LOCKED_BIT (|PARKED_BIT)
    // 3)    REGISTRATING|INIT_SKIPED (final)
    //    a) REGISTERED|INITIALIZING|LOCKED (|PARKED)
    //    b) INITIALIZING|LOCKED (|PARKED)
    //       REGISTERED|INIT_SKIPED (final)
    // branch a):
    // 4) REGISTERED|INITIALIZING|INIT_SKIPED (final)
    //    REGISTERED|INITIALIZED
    // 5) REGISTERED|INITIALIZED|FINALIZING
    // 6) REGISTERED|INITIALIZED|FINALIZED (final)
    //    REGISTERED|INITIALIZED|FINALIZATION_PANIC(final)
    // branch b):
    // 4) INITIALIZING|INIT_SKIPED (final)
    //    INITIALIZED (final)
    // 5) INITIALIZED|FINALIZING (if manualy finalize)
    // 6) INITIALIZED|FINALIZED (final)
    //    INITIALIZED|FINALIZATION_PANIC(final)

    let guard = UnparkGuard(&this, unpark, REGISTRATING_BIT | INIT_SKIPED_BIT);
    let cond = reg(s);
    forget(guard);

    if cond {
        let guard = UnparkGuard(
            &this,
            unpark,
            REGISTERED_BIT | INITIALIZING_BIT | INIT_SKIPED_BIT,
        );
        this.0
            .fetch_xor(REGISTRATING_BIT | REGISTERED_BIT | INITIALIZING_BIT, Ordering::Release);

        init(Static::data(s));

        forget(guard);

        let prev = this.0.swap(
            INITED_BIT | REGISTERED_BIT,
            Ordering::Release,
        );
        if prev & PARKED_BIT != 0 {
            unpark()
        }
        return shall_proceed(Phase(INITED_BIT | REGISTERED_BIT));
    } else if init_on_reg_failure {

        let guard = UnparkGuard(
            &this,
            unpark,
            INITIALIZING_BIT | INIT_SKIPED_BIT,
        );

        this.0
            .fetch_xor(REGISTRATING_BIT|INITIALIZING_BIT, Ordering::Release);

        init(Static::data(s));

        forget(guard);

        let prev = this.0.swap(
            INITED_BIT,
            Ordering::Release,
        );
        if prev & PARKED_BIT != 0 {
            unpark()
        }
        return shall_proceed(Phase(INITED_BIT));

    } else {

        let prev = this.0.swap(REGISTERED_BIT|INIT_SKIPED_BIT, Ordering::Release);
        if prev & PARKED_BIT != 0 {
            unpark()
        }
        return shall_proceed(Phase(REGISTERED_BIT | INIT_SKIPED_BIT));
    }
}

pub struct GlobalManager<const GLOBAL: bool>(AtomicU32,
#[cfg(debug_mode)] AtomicUsize);

impl<const GLOBAL: bool> ManagerBase for GlobalManager<GLOBAL> {
    #[inline(always)]
    fn phase(&self) -> Phase {
        Phase(self.0.load(Ordering::Acquire))
    }
}
impl GlobalManager<true> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(AtomicU32::new(0),
#[cfg(debug_mode)] AtomicUsize::new(0))

    }
}
impl GlobalManager<false> {
    #[inline(always)]
    pub const fn new_lazy() -> Self {
        Self(AtomicU32::new(0),
#[cfg(debug_mode)] AtomicUsize::new(0))
    }
}

unsafe impl<T: Static, const GLOBAL: bool> OnceManager<T> for GlobalManager<GLOBAL>
where
    T::Manager: AsRef<GlobalManager<GLOBAL>>,
{
    #[inline(always)]
    fn register(
        s: &T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Static>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool,
    ) -> bool {
        use phase::*;

        let this = Static::manager(s).as_ref();

        let park_validate = || {
            this.0.load(Ordering::Relaxed) & (LOCKED_BIT | PARKED_BIT) == LOCKED_BIT | PARKED_BIT
        };

        let parker = || unsafe {
            parking_lot_core::park(
                this as *const _ as usize,
                park_validate,
                || {},
                |_, _| {},
                parking_lot_core::DEFAULT_PARK_TOKEN,
                None,
            );
        };

        let unpark = || unsafe {
            parking_lot_core::unpark_all(
                this as *const _ as usize,
                parking_lot_core::DEFAULT_UNPARK_TOKEN,
            );
        };

        if cfg!(not(all(
            support_priority,
            not(feature = "test_no_global_lazy_hint")
        ))) || !GLOBAL
        {
            let cur = this.0.load(Ordering::Acquire);

            if shall_proceed(Phase(cur)) {
                atomic_register_uninited(this, s, shall_proceed, init, reg,init_on_reg_failure, parker, unpark, #[cfg(debug_mode)] &this.1)
            } else {
                false
            }
        } else {
            #[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
            {
            if GLOBAL {
                if inited::global_inited_hint() {
                    debug_assert!(!shall_proceed(Phase(this.0.load(Ordering::Relaxed))));
                    false
                } else {
                    atomic_register_uninited(this, s, shall_proceed, init, reg,init_on_reg_failure, parker, unpark, #[cfg(debug_mode)] &this.1)
                }
            } else {
                unreachable!()
            }
            }
            #[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
            {
                unreachable!()
            }
        }
    }
    fn finalize(s: &T, f: impl FnOnce(&T::Data)) {
        use phase::*;

        let this = Static::manager(s).as_ref();

        struct Guard<'a, const G: bool>(&'a GlobalManager<G>);
        impl<'a, const G: bool> Drop for Guard<'a, G> {
            fn drop(&mut self) {
                // Mark the state as poisoned, unlock it and unpark all threads.
                let man = self.0;
                man.0.fetch_xor(
                    FINALIZATION_PANIC_BIT|FINALIZING_BIT,
                    Ordering::Relaxed,
                );
            }
        }

        this.0.load(Ordering::Relaxed);

        use parking_lot_core::SpinWait;

        let mut spin_wait = SpinWait::new();

        let mut cur = this.0.load(Ordering::Relaxed);

        let park = |cur| unsafe {
            #[cfg(debug_order)]
            {
            }
            parking_lot_core::park(
                this as *const _ as usize,
                || this.0.load(Ordering::Relaxed) == cur,
                || {},
                |_, _| {},
                parking_lot_core::DEFAULT_PARK_TOKEN,
                None,
            );
        };

        loop {
            if cur & INIT_SKIPED_BIT != 0 {
                return;
            }
            assert_eq!(cur & (FINALIZING_BIT | FINALIZED_BIT | FINALIZATION_PANIC_BIT), 0);
            match this.0.compare_exchange_weak(
                cur & !(LOCKED_BIT|PARKED_BIT),
                cur & !(LOCKED_BIT|PARKED_BIT) | FINALIZING_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => cur = x,
            }
            if cur & LOCKED_BIT == 0 {
                continue;
            }
            if cur & PARKED_BIT == 0 && spin_wait.spin() {
                cur = this.0.load(Ordering::Relaxed);
                continue;
            }
            if cur & PARKED_BIT == 0 {
                if let Err(x) = this.0.compare_exchange_weak(
                    cur,
                    cur | PARKED_BIT,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    cur = x;
                    continue;
                }
            }
            park(cur | PARKED_BIT);
            spin_wait.reset();
            cur = this.0.load(Ordering::Relaxed);
        }

        let guard = Guard(this);

        f(Static::data(s));

        forget(guard);

        this.0.fetch_xor(
            FINALIZING_BIT | FINALIZED_BIT,
            Ordering::Release,
        );
    }
}
}
#[cfg(feature="global_once")]
pub use global_once::GlobalManager;
