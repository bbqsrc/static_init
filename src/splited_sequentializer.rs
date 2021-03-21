use crate::mutex::{PhaseGuard, UnSyncPhaseGuard, UnSyncPhaseLocker};
use crate::{Phase, Phased, Sequential, Sequentializer, SplitedLazySequentializer};

#[cfg(debug_mode)]
use super::CyclicPanic;

/// Ensure sequentialization, similar to SyncSequentializer
/// but in a maner that does not support that a reference to
/// the object is shared between threads.
pub struct UnSyncSequentializer(UnSyncPhaseLocker);

impl UnSyncSequentializer {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(UnSyncPhaseLocker::new(Phase::empty()))
    }
}

impl Phased for UnSyncSequentializer {
    fn phase(this: &Self) -> Phase {
        this.0.phase()
    }
}

impl<'a, T: Sequential + 'a> Sequentializer<'a, T> for UnSyncSequentializer
where
    T::Sequentializer: AsRef<UnSyncSequentializer>,
{
    type Guard = Option<UnSyncPhaseGuard<'a, T>>;

    fn lock(s: &'a T, shall_proceed: impl Fn(Phase) -> bool) -> Self::Guard {
        let this = Sequential::sequentializer(s).as_ref();

        this.0.lock(s, &shall_proceed)
    }
}

impl<'a, T: Sequential + 'a> SplitedLazySequentializer<'a, T> for UnSyncSequentializer
where
    T::Sequentializer: AsRef<UnSyncSequentializer>,
{
    #[inline(always)]
    fn init(
        s: &'a T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Sequential>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool,
    ) -> Self::Guard {

        let phase_guard = match <Self as Sequentializer<T>>::lock(s, shall_proceed) {
            None => return None,
            Some(l) => l,
        };

        let phase_guard = lazy_initialization(phase_guard, init, reg, init_on_reg_failure);

        return Some(phase_guard);
    }

    fn finalize_callback(s: &T, f: impl FnOnce(&T::Data)) {
        let this = Sequential::sequentializer(s).as_ref();

        let phase_guard = match this.0.lock(Sequential::data(s), |p| {
            (p & (Phase::FINALIZATION
                | Phase::FINALIZED
                | Phase::FINALIZATION_PANICKED
                | Phase::INITIALIZATION_SKIPED))
                .is_empty()
        }) {
            None => return,
            Some(l) => l,
        };

        lazy_finalization(phase_guard, f);
    }
}

#[cfg(feature = "global_once")]
mod global_once {
    use super::{lazy_finalization, lazy_initialization};
    use super::{Phase, Phased, Sequential, Sequentializer, SplitedLazySequentializer};
    use crate::mutex::{SyncPhaseGuard, SyncPhasedLocker as PhasedLocker};

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
        s: &'a T,
        shall_proceed: impl Fn(Phase) -> bool,
        init: impl FnOnce(&<T as Sequential>::Data),
        reg: impl FnOnce(&T) -> bool,
        init_on_reg_failure: bool,
        #[cfg(debug_mode)] id: &AtomicUsize,
    ) -> Option<SyncPhaseGuard<'a, T>> {

        let phase_guard = match this.0.lock(s, &shall_proceed) {
            None => return None,
            Some(l) => l,
        };

        let phase_guard = lazy_initialization(phase_guard, init, reg, init_on_reg_failure);

        return Some(phase_guard);
    }

    #[cfg_attr(docsrs, doc(cfg(feature = "global_once")))]
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
    pub struct SyncSequentializer<const GLOBAL: bool>(PhasedLocker, #[cfg(debug_mode)] AtomicUsize);

    impl<const GLOBAL: bool> Phased for SyncSequentializer<GLOBAL> {
        #[inline(always)]
        fn phase(this: &Self) -> Phase {
            this.0.phase()
        }
    }
    impl SyncSequentializer<true> {
        #[inline(always)]
        pub const fn new() -> Self {
            Self(
                PhasedLocker::new(Phase::empty()),
                #[cfg(debug_mode)]
                AtomicUsize::new(0),
            )
        }
    }
    impl SyncSequentializer<false> {
        #[inline(always)]
        pub const fn new_lazy() -> Self {
            Self(
                PhasedLocker::new(Phase::empty()),
                #[cfg(debug_mode)]
                AtomicUsize::new(0),
            )
        }
    }

    impl<'a, T: Sequential + 'a, const G: bool> Sequentializer<'a, T> for SyncSequentializer<G>
    where
        T::Sequentializer: AsRef<SyncSequentializer<G>>,
    {
        type Guard = Option<SyncPhaseGuard<'a, T>>;

        fn lock(s: &'a T, shall_proceed: impl Fn(Phase) -> bool) -> Self::Guard {
            let this = Sequential::sequentializer(s).as_ref();

            this.0.lock(s, &shall_proceed)
        }
    }

    impl<'a, T: Sequential + 'a, const GLOBAL: bool> SplitedLazySequentializer<'a, T>
        for SyncSequentializer<GLOBAL>
    where
        T::Sequentializer: AsRef<SyncSequentializer<GLOBAL>>,
    {
        #[inline(always)]
        fn init(
            s: &'a T,
            shall_proceed: impl Fn(Phase) -> bool,
            init: impl FnOnce(&<T as Sequential>::Data),
            reg: impl FnOnce(&T) -> bool,
            init_on_reg_failure: bool,
        ) -> Self::Guard {
            let this = Sequential::sequentializer(s).as_ref();

            if cfg!(not(all(
                support_priority,
                not(feature = "test_no_global_lazy_hint")
            ))) || !GLOBAL
            {
                let cur = this.0.phase();

                if shall_proceed(cur) {
                    atomic_register_uninited(
                        this,
                        s,
                        shall_proceed,
                        init,
                        reg,
                        init_on_reg_failure,
                        #[cfg(debug_mode)]
                        &this.1,
                    )
                } else {
                    None
                }
            } else {
                #[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
                {
                    if GLOBAL {
                        if inited::global_inited_hint() {
                            debug_assert!(!shall_proceed(this.0.phase()));
                            None
                        } else {
                            atomic_register_uninited(
                                this,
                                s,
                                shall_proceed,
                                init,
                                reg,
                                init_on_reg_failure,
                                #[cfg(debug_mode)]
                                &this.1,
                            )
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
            let this = Sequential::sequentializer(s).as_ref();

            let phase_guard = match this.0.lock(Sequential::data(s), |p| {
                (p & (Phase::FINALIZATION
                    | Phase::FINALIZED
                    | Phase::FINALIZATION_PANICKED
                    | Phase::INITIALIZATION_SKIPED))
                    .is_empty()
            }) {
                None => return,
                Some(l) => l,
            };

            lazy_finalization(phase_guard, f);
        }
    }
}
//TODO: diviser en deux SyncSequentializer && ProgramInitedSyncSequentializer
#[cfg(feature = "global_once")]
pub use global_once::SyncSequentializer;

fn lazy_initialization<P: PhaseGuard<S>, S: Sequential>(
    mut phase_guard: P,
    init: impl FnOnce(&<S as Sequential>::Data),
    reg: impl FnOnce(&S) -> bool,
    init_on_reg_failure: bool,
) -> P {
    let cur = phase_guard.phase();

    let registrating = cur | Phase::REGISTRATION;

    let registration_finished = cur;

    let registration_failed = cur | Phase::REGISTRATION_PANICKED | Phase::INITIALIZATION_SKIPED;

    phase_guard.set_phase_committed(registrating);

    let cond = phase_guard.transition(reg, registration_finished, registration_failed);

    if cond {
        let initializing = registration_finished | Phase::REGISTERED | Phase::INITIALIZATION;
        let initialized = registration_finished | Phase::REGISTERED | Phase::INITIALIZED;
        let initialization_panic = registration_finished
            | Phase::REGISTERED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;

        phase_guard.set_phase_committed(initializing);

        phase_guard.transition(
            |s| init(Sequential::data(s)),
            initialized,
            initialization_panic,
        );
    } else if init_on_reg_failure {
        let initializing =
            registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZATION;
        let initialized = registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZED;
        let initialization_panic = registration_finished
            | Phase::REGISTRATION_REFUSED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;

        phase_guard.set_phase_committed(initializing);

        phase_guard.transition(
            |s| init(Sequential::data(s)),
            initialized,
            initialization_panic,
        );
    } else {
        let no_init =
            registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZATION_SKIPED;

        phase_guard.set_phase_committed(no_init);
    }
    phase_guard
}

fn lazy_finalization<T, P: PhaseGuard<T>>(mut phase_guard: P, f: impl FnOnce(&T)) {
    let cur = phase_guard.phase();

    let finalizing = cur | Phase::FINALIZATION;

    let finalizing_success = cur | Phase::FINALIZED;

    let finalizing_failed = cur | Phase::FINALIZATION_PANICKED;

    phase_guard.set_phase_committed(finalizing);
    phase_guard.transition(f, finalizing_success, finalizing_failed);
}
