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
use crate::mutex::{PhasedLocker, PhaseGuard};

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

    let mut phase_guard = match this.0.lock(s,&shall_proceed) {
        None => return false,
        Some(l) => l,
    };


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

    let cur = phase_guard.phase().0;

    let registrating = cur | REGISTRATING_BIT;

    let registration_finished = cur;

    let registration_failed = cur |REGISTRATING_PANIC_BIT|INIT_SKIPED_BIT;
    
    phase_guard.set_phase_committed(Phase(registrating));

    let cond = phase_guard.transition(reg
        ,Phase(registration_finished)
        ,Phase(registration_failed));


    if cond {
        let initializing = registration_finished | REGISTERED_BIT | INITIALIZING_BIT;
        let initialized = registration_finished | REGISTERED_BIT | INITED_BIT;
        let initialization_panic = registration_finished | REGISTERED_BIT | INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT;

        phase_guard.set_phase_committed(Phase(initializing));

        phase_guard.transition(|s| init(Sequential::data(s)),
            Phase(initialized),
            Phase(initialization_panic)
            );

        return shall_proceed(Phase(initialized));
    } else if init_on_reg_failure {
        
        let initializing = registration_finished | REGISTRATION_REFUSED_BIT | INITIALIZING_BIT;
        let initialized = registration_finished | REGISTRATION_REFUSED_BIT | INITED_BIT;
        let initialization_panic = registration_finished | REGISTRATION_REFUSED_BIT | INITIALIZING_PANICKED_BIT | INIT_SKIPED_BIT;

        phase_guard.set_phase_committed(Phase(initializing));

        phase_guard.transition(|s| init(Sequential::data(s)),
            Phase(initialized),
            Phase(initialization_panic)
            );

        return shall_proceed(Phase(REGISTRATION_REFUSED_BIT|INITED_BIT));

    } else {
        let no_init = registration_finished | REGISTRATION_REFUSED_BIT | INIT_SKIPED_BIT;

        phase_guard.set_phase_committed(Phase(no_init));

        return shall_proceed(Phase(no_init));
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
pub struct SyncSequentializer<const GLOBAL: bool>(PhasedLocker,
#[cfg(debug_mode)] AtomicUsize);

impl<const GLOBAL: bool> Phased for SyncSequentializer<GLOBAL> {
    #[inline(always)]
    fn phase(this: &Self) -> Phase {
        this.0.phase()
    }
}
impl SyncSequentializer<true> {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(PhasedLocker::new(Phase(0)),
#[cfg(debug_mode)] AtomicUsize::new(0))

    }
}
impl SyncSequentializer<false> {
    #[inline(always)]
    pub const fn new_lazy() -> Self {
        Self(PhasedLocker::new(Phase(0)),
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
        let this = Sequential::sequentializer(s).as_ref();

        if cfg!(not(all(
            support_priority,
            not(feature = "test_no_global_lazy_hint")
        ))) || !GLOBAL
        {
            let cur = this.0.phase();

            if shall_proceed(cur) {
                atomic_register_uninited(this, s, shall_proceed, init, reg,init_on_reg_failure, #[cfg(debug_mode)] &this.1)
            } else {
                false
            }
        } else {
            #[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
            {
            if GLOBAL {
                if inited::global_inited_hint() {
                    debug_assert!(!shall_proceed(this.0.phase()));
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

        let mut phase_guard = match this.0.lock(Sequential::data(s),
            |p| {p.0 & (FINALIZING_BIT | FINALIZED_BIT | FINALIZATION_PANIC_BIT|INIT_SKIPED_BIT) == 0})
            {
            None => return,
            Some(l) => l,
        };
    
        let cur = phase_guard.phase().0;

        let finalizing = cur | FINALIZING_BIT;

        let finalizing_success = cur | FINALIZED_BIT;

        let finalizing_failed = cur | FINALIZATION_PANIC_BIT;

        phase_guard.set_phase_committed(Phase(finalizing
        ));
        phase_guard.transition(f
            ,Phase(finalizing_success)
            ,Phase(finalizing_failed));
    }
}
}
//TODO: diviser en deux SyncSequentializer && ProgramInitedSyncSequentializer
#[cfg(feature="global_once")]
pub use global_once::SyncSequentializer;


