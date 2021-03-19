use crate::{Phased, phase, Phase, SplitedSequentializer, Sequential};
use core::cell::Cell;
use core::mem::forget;

#[cfg(debug_mode)]
use super::CyclicPanic;

/// Ensure sequentialization, similar to SyncSequentializer
/// but in a maner that does not support that a reference to
/// the object is shared between threads.
pub struct UnSyncSequentializer(Cell<Phase>);

impl UnSyncSequentializer {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(Cell::new(Phase::new()))
    }
}

impl Phased for UnSyncSequentializer {
    fn phase(this :&Self) -> Phase {
        this.0.get()
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
fn register_uninited<T: Sequential>(
    this: &UnSyncSequentializer,
    s: &T,
    init: impl FnOnce(&<T as Sequential>::Data),
    reg: impl FnOnce(&T) -> bool,
    init_on_reg_failure: bool,
) {
    use phase::*;

    this.0.set(Phase(REGISTRATING_BIT));
    let guard = Guard(&this.0, Phase(REGISTRATING_PANIC_BIT | INIT_SKIPED_BIT));
    let cond = reg(s);
    forget(guard);

    if cond {
        this.0
            .set(Phase(INITIALIZING_BIT | REGISTERED_BIT));
        let guard = Guard(
            &this.0,
            Phase(REGISTERED_BIT | INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT),
        );

        init(Sequential::data(s));

        forget(guard);
        this.0
            .set(Phase(INITED_BIT | REGISTERED_BIT));
    } else if init_on_reg_failure {
        this.0
            .set(Phase(REGISTRATION_REFUSED_BIT|INITIALIZING_BIT));
        let guard = Guard(
            &this.0,
            Phase(INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT),
        );

        init(Sequential::data(s));

        forget(guard);

        this.0
            .set(Phase(REGISTRATION_REFUSED_BIT|INITED_BIT));

    } else {
        this.0.set(Phase(REGISTERED_BIT|INIT_SKIPED_BIT));
    }
}

impl<T: Sequential> SplitedSequentializer<T> for UnSyncSequentializer
where
    T::Sequentializer: AsRef<UnSyncSequentializer>,
{
    #[inline(always)]
    fn init(
        s: &T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Sequential>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool
    ) -> bool {
        let this = Sequential::sequentializer(s).as_ref();

        let cur = this.0.get();

        if shall_proceed(cur) {
            #[cfg(debug_mode)]
            {
                if cur.initialization() || cur.finalize_registration() {
                    std::panic::panic_any(CyclicPanic);
                }
            }
            register_uninited(this, s, init, reg, init_on_reg_failure);
            shall_proceed(this.0.get())
        } else {
            false
        }
    }

    fn finalize_callback(s: &T, f: impl FnOnce(&T::Data)) {
        use phase::*;

        let this = Sequential::sequentializer(s).as_ref();

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

            f(Sequential::data(s));

            forget(guard);

            this.0.set(Phase( this.0.get().0 ^ (FINALIZING_BIT |FINALIZED_BIT)));
        }     
    }
}

#[cfg(feature="global_once")]
mod global_once {
use super::{Phase,phase,Phased, SplitedSequentializer, Sequential};
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
fn atomic_register_uninited<'a, T: Sequential, const GLOBAL: bool>(
    this: &'a SyncSequentializer<GLOBAL>,
    s: &T,
    shall_proceed: impl Fn(Phase) -> bool,
    init: impl FnOnce(&<T as Sequential>::Data),
    reg: impl FnOnce(&T) -> bool,
    init_on_reg_failure: bool,
    #[cfg(debug_mode)]
    id: &AtomicUsize
) -> bool {
    use phase::*;

    use crate::spinwait::SpinWait;

    use crate::futex::{park,unpark_all};

    let mut spin_wait = SpinWait::new();

    let mut cur = this.0.load(Ordering::Relaxed);

    loop {
        if !shall_proceed(Phase(cur & !(PARKED_BIT|LOCKED_BIT))) {
            fence(Ordering::Acquire);
            return false;
        }
        if cur & LOCKED_BIT == 0 {
            match this.0.compare_exchange_weak(
                cur,
                cur | LOCKED_BIT | REGISTRATING_BIT,
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

        park(&this.0,cur|PARKED_BIT);
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

    struct UnparkGuard<'a, const G: bool>(&'a SyncSequentializer<G>, u32);
    impl<'a, const G: bool> Drop for UnparkGuard<'a, G> {
        fn drop(&mut self) {
            // Mark the state as poisoned, unlock it and unpark all threads.
            let man = self.0;
            let cur = man.0.swap(self.1, Ordering::Release);
            if cur & PARKED_BIT != 0 {
                unpark_all(&man.0)
            }
        }
    }

    struct Guard<'a, const G: bool>(&'a SyncSequentializer<G>, u32);
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

    let guard = UnparkGuard(&this, REGISTRATING_PANIC_BIT | INIT_SKIPED_BIT);
    let cond = reg(s);
    forget(guard);

    if cond {
        let guard = UnparkGuard(
            &this,
            REGISTERED_BIT | INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT,
        );
        this.0
            .fetch_xor(REGISTRATING_BIT | REGISTERED_BIT | INITIALIZING_BIT, Ordering::Release);

        init(Sequential::data(s));

        forget(guard);

        let prev = this.0.swap(
            INITED_BIT | REGISTERED_BIT,
            Ordering::Release,
        );
        if prev & PARKED_BIT != 0 {
            unpark_all(&this.0)
        }
        return shall_proceed(Phase(INITED_BIT | REGISTERED_BIT));
    } else if init_on_reg_failure {

        let guard = UnparkGuard(
            &this,
            REGISTRATION_REFUSED_BIT | INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT,
        );

        this.0
            .fetch_xor(REGISTRATING_BIT|REGISTRATION_REFUSED_BIT|INITIALIZING_BIT, Ordering::Release);

        init(Sequential::data(s));

        forget(guard);

        let prev = this.0.swap(
            REGISTRATION_REFUSED_BIT|INITED_BIT,
            Ordering::Release,
        );
        if prev & PARKED_BIT != 0 {
            unpark_all(&this.0)
        }
        return shall_proceed(Phase(REGISTRATION_REFUSED_BIT|INITED_BIT));

    } else {

        let prev = this.0.swap(REGISTERED_BIT|INIT_SKIPED_BIT, Ordering::Release);
        if prev & PARKED_BIT != 0 {
            unpark_all(&this.0)
        }
        return shall_proceed(Phase(REGISTERED_BIT | INIT_SKIPED_BIT));
    }
}

#[cfg_attr(docsrs, doc(cfg(feature="global_once")))]
/// Ensure sequentialization.
///
/// The SplitedSequentializer::init method can be called concurently on this 
/// object, only one thread will perform the initialization.
/// 
/// More over the SplitedSequentializer::finalize method can be called by
/// one thread while other threads call init. The finalize call will wait
/// until the init function finished or skiped the initialization process.
///
/// The finalization function will proceed only if the Sequential is in
/// initialized phase. Concurent call to finalize may lead to concurent
/// calls the finalize argument functor.
///
/// # Initialization phases:
///
/// The init function will firt check if `shall_proceed` functor is true.
/// If it is the following phase transition of the object will happen
///
///  1. Initial state
///
///  2. registration
///
///  3. Either:   
///
///     a. registration_panicked and initialization_skiped (final)
///
///     b. registrated and initializing
///
///     c. registration_refused and initializing (if init_on_reg_failure is true) 
///
///     d. registrated and initiazation_skiped (final if init_on_ref_failure is false)
///
/// Then, if 3) is b:
///
/// 4. Either: 
///
///     - registrated and initialization_panicked
///
///     - registrated and initialized 
///
/// Or, if 3) is c): 
///
/// 4. Either:
///
///     - initialization_panicked
///
///     - initialized
///
/// # Finalization phase:
///
/// The finalization phase will be executed only if the previous phase is initialized
///
/// The phase will conserve its qualificatif (registrated, initialized) and the following attriute
/// transition will happend:
///
/// 1. Finalization
///
/// 2. Either:
///
///     a. Finalized
///
///     b. Finalization panicked
///
pub struct SyncSequentializer<const GLOBAL: bool>(AtomicU32,
#[cfg(debug_mode)] AtomicUsize);

impl<const GLOBAL: bool> Phased for SyncSequentializer<GLOBAL> {
    #[inline(always)]
    fn phase(this: &Self) -> Phase {
        Phase(this.0.load(Ordering::Acquire))
    }
}
impl SyncSequentializer<true> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(AtomicU32::new(0),
#[cfg(debug_mode)] AtomicUsize::new(0))

    }
}
impl SyncSequentializer<false> {
    #[inline(always)]
    pub const fn new_lazy() -> Self {
        Self(AtomicU32::new(0),
#[cfg(debug_mode)] AtomicUsize::new(0))
    }
}

impl<T: Sequential, const GLOBAL: bool> SplitedSequentializer<T> for SyncSequentializer<GLOBAL>
where
    T::Sequentializer: AsRef<SyncSequentializer<GLOBAL>>,
{
    #[inline(always)]
    fn init(
        s: &T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Sequential>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool,
    ) -> bool {
        use phase::*;

        let this = Sequential::sequentializer(s).as_ref();

        if cfg!(not(all(
            support_priority,
            not(feature = "test_no_global_lazy_hint")
        ))) || !GLOBAL
        {
            let cur = this.0.load(Ordering::Acquire);

            if shall_proceed(Phase(cur)) {
                atomic_register_uninited(this, s, shall_proceed, init, reg,init_on_reg_failure, #[cfg(debug_mode)] &this.1)
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
                    atomic_register_uninited(this, s, shall_proceed, init, reg,init_on_reg_failure, #[cfg(debug_mode)] &this.1)
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
    fn finalize_callback(s: &T, f: impl FnOnce(&T::Data)) {
        use phase::*;

        let this = Sequential::sequentializer(s).as_ref();

        struct Guard<'a, const G: bool>(&'a SyncSequentializer<G>);
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

        use crate::spinwait::SpinWait;
        use crate::futex::park;

        let mut spin_wait = SpinWait::new();

        let mut cur = this.0.load(Ordering::Relaxed);


        loop {
            if cur & (FINALIZING_BIT | FINALIZED_BIT | FINALIZATION_PANIC_BIT|INIT_SKIPED_BIT) != 0 {
                return;
            }
            assert!(cur & INITED_BIT != 0);
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
            park(&this.0, cur | PARKED_BIT);
            spin_wait.reset();
            cur = this.0.load(Ordering::Relaxed);
        }

        let guard = Guard(this);

        f(Sequential::data(s));

        forget(guard);

        this.0.fetch_xor(
            FINALIZING_BIT | FINALIZED_BIT,
            Ordering::Release,
        );
    }
}
}
//TODO: diviser en deux SyncSequentializer && ProgramInitedSyncSequentializer
#[cfg(feature="global_once")]
pub use global_once::SyncSequentializer;


