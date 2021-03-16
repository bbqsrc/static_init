pub use parking_lot::{Once as PkOnce, OnceState};

use super::{ManagerBase, OnceManager, Phase, Static};
use core::cell::Cell;
use core::mem::forget;
use core::sync::atomic::{fence, AtomicU32, Ordering};

#[cfg(debug_mode)]
use super::CyclicPanic;
#[cfg(debug_mode)]
use core::sync::atomic::AtomicUsize;

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
) {
    use crate::phase::*;

    //states:
    // 1) 0
    // 2) REGISTRATING
    // 3) REGISTRATING|POISON (final)
    //    REGISTRATING|REGISTERED|INITIALIZING
    //    REGISTERED (final)
    // 4) REGISTRATING|REGISTERED|INITIALIZING|POISON (final)
    //    REGISTRATING|REGISTERED|INITIALIZED
    // 5) REGISTRATING|REGISTERED|INITIALIZED|FINALIZING
    // 6) REGISTRATING|REGISTERED|INITIALIZED|FINALIZED (final)
    // 6) REGISTRATING|REGISTERED|INITIALIZED|FINALIZATION_PANIC(final)

    this.0.set(Phase(REGISTRATING_BIT));
    let guard = Guard(&this.0, Phase(REGISTRATING_BIT | POISON_BIT));
    let cond = reg(s);
    forget(guard);

    if cond {
        this.0
            .set(Phase(INITIALIZING_BIT | REGISTRATING_BIT | REGISTERED_BIT));
        let guard = Guard(
            &this.0,
            Phase(REGISTRATING_BIT | REGISTERED_BIT | INITIALIZING_BIT | POISON_BIT),
        );

        init(Static::data(s));

        forget(guard);
        this.0
            .set(Phase(INITED_BIT | REGISTERED_BIT | REGISTRATING_BIT));
    } else {
        this.0.set(Phase(REGISTERED_BIT|POISON_BIT));
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
    ) {
        let this = Static::manager(s).as_ref();

        let cur = this.0.get();

        if shall_proceed(cur) {
            register_uninited(this, s, init, reg);
        }
    }

    fn finalize(s: &T, f: impl FnOnce(&T::Data)) {
        use crate::phase::*;

        let this = Static::manager(s).as_ref();

        struct Guard<'a>(&'a Cell<Phase>);
        impl<'a> Drop for Guard<'a> {
            fn drop(&mut self) {
                // Mark the state as poisoned, unlock it and unpark all threads.
                let man = self.0;
                man.set(Phase(
                    INITED_BIT | REGISTERED_BIT | REGISTRATING_BIT | FINALIZATION_PANIC_BIT,
                ));
            }
        }
        if this.0.get().0 == INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT {
            this.0.set(Phase(
                FINALIZING_BIT | INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT,
            ));

            let guard = Guard(&this.0);

            f(Static::data(s));

            forget(guard);

            this.0.set(Phase(
                FINALIZED_BIT | INITED_BIT | REGISTERED_BIT | REGISTRATING_BIT,
            ));
        } else {
            if  ! (this.0.get()
                    == Phase(REGISTRATING_BIT | REGISTERED_BIT | INITIALIZING_BIT | POISON_BIT)
                    || this.0.get() == Phase(REGISTERED_BIT|POISON_BIT)){
            panic!("{:?}",this.0.get());
            }
        }
    }
}

#[inline(never)]
#[cold]
fn atomic_register_uninited<'a, T: Static, const GLOBAL: bool>(
    this: &'a GlobalManager<GLOBAL>,
    s: &T,
    shall_proceed: impl Fn(Phase) -> bool,
    init: impl FnOnce(&<T as Static>::Data),
    reg: impl FnOnce(&T) -> bool,
    park: impl Fn(),
    unpark: impl Fn() + Copy,
    #[cfg(debug_mode)]
    id: &AtomicUsize
) {
    use crate::phase::*;

    use parking_lot_core::SpinWait;

    let mut spin_wait = SpinWait::new();

    let mut cur = this.0.load(Ordering::Relaxed);

    loop {
        if !shall_proceed(Phase(cur)) {
            fence(Ordering::Acquire);
            return;
        }
        if cur & LOCKED_BIT == 0 {
            match this.0.compare_exchange_weak(
                cur,
                (cur | LOCKED_BIT | REGISTRATING_BIT) & !POISON_BIT,
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
        let id = id.load(Ordering::Release);
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
    // 3) REGISTRATING|POISON (final)
    //    REGISTRATING|REGISTERED|INITIALIZING|LOCKED (|PARKED)
    //    REGISTERED|POISON (final)
    // 4) REGISTRATING|REGISTERED|INITIALIZING|POISON (final)
    //    REGISTRATING|REGISTERED|INITIALIZED
    // 5) REGISTRATING|REGISTERED|INITIALIZED|FINALIZING
    // 6) REGISTRATING|REGISTERED|INITIALIZED|FINALIZED (final)
    // 6) REGISTRATING|REGISTERED|INITIALIZED|FINALIZING|POISON(final)

    let guard = UnparkGuard(&this, unpark, REGISTRATING_BIT | POISON_BIT);
    let cond = reg(s);
    forget(guard);

    if cond {
        let guard = UnparkGuard(
            &this,
            unpark,
            REGISTRATING_BIT | REGISTERED_BIT | INITIALIZING_BIT | POISON_BIT,
        );
        this.0
            .fetch_or(REGISTERED_BIT | INITIALIZING_BIT, Ordering::Release);

        init(Static::data(s));

        forget(guard);

        let prev = this.0.swap(
            INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT,
            Ordering::Release,
        );
        if prev & PARKED_BIT != 0 {
            unpark()
        }
    } else {
        let prev = this.0.swap(REGISTERED_BIT|POISON_BIT, Ordering::Release);
        if prev & PARKED_BIT != 0 {
            unpark()
        }
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
    ) {
        use crate::phase::*;

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
                atomic_register_uninited(this, s, shall_proceed, init, reg, parker, unpark, #[cfg(debug_mode)] &this.1);
            }
        } else {
            #[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
            if GLOBAL {
                if inited::global_inited_hint() {
                    debug_assert!(!shall_proceed(Phase(this.0.load(Ordering::Relaxed))))
                } else {
                    atomic_register_uninited(this, s, shall_proceed, init, reg, parker, unpark, #[cfg(debug_mode)] &this.1);
                }
            }
        }
    }
    fn finalize(s: &T, f: impl FnOnce(&T::Data)) {
        use crate::phase::*;

        let this = Static::manager(s).as_ref();

        struct Guard<'a, const G: bool>(&'a GlobalManager<G>);
        impl<'a, const G: bool> Drop for Guard<'a, G> {
            fn drop(&mut self) {
                // Mark the state as poisoned, unlock it and unpark all threads.
                let man = self.0;
                man.0.store(
                    INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT | FINALIZATION_PANIC_BIT,
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
            if cur & POISON_BIT != 0 {
                return;
            }
            match this.0.compare_exchange_weak(
                INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT,
                INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT | FINALIZING_BIT,
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

        this.0.store(
            INITED_BIT | REGISTRATING_BIT | REGISTERED_BIT | FINALIZED_BIT,
            Ordering::Release,
        );
    }
}
