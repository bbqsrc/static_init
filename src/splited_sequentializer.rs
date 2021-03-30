use crate::mutex::{
    LockNature, LockResult, PhaseGuard, UnSyncPhaseGuard, UnSyncPhaseLocker, UnSyncReadPhaseGuard,
};
use crate::{Phase, Phased, Sequential, Sequentializer, SplitedLazySequentializer};

use core::hint::unreachable_unchecked;

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

// SAFETY: it is safe because it does implement circular initialization panic
unsafe impl<'a, T: Sequential + 'a> Sequentializer<'a, T> for UnSyncSequentializer
where
    T::Sequentializer: AsRef<UnSyncSequentializer>,
{
    type ReadGuard = UnSyncReadPhaseGuard<'a, T>;
    type WriteGuard = UnSyncPhaseGuard<'a, T>;

    fn lock(
        s: &'a T,
        lock_nature: impl Fn(Phase) -> LockNature,
    ) -> LockResult<UnSyncReadPhaseGuard<'a, T>, UnSyncPhaseGuard<'a, T>> {
        let this = Sequential::sequentializer(s).as_ref();

        this.0.lock(s, &lock_nature)
    }
    fn try_lock(
        s: &'a T,
        lock_nature: impl Fn(Phase) -> LockNature,
    ) -> Option<LockResult<UnSyncReadPhaseGuard<'a, T>, UnSyncPhaseGuard<'a, T>>> {
        let this = Sequential::sequentializer(s).as_ref();

        this.0.try_lock(s, &lock_nature)
    }
}

// SAFETY: it is safe because it does implement circular initialization panic
unsafe impl<'a, T: Sequential + 'a> SplitedLazySequentializer<'a, T> for UnSyncSequentializer
where
    T::Sequentializer: AsRef<UnSyncSequentializer>,
{
    #[inline(always)]
    fn init(
        s: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Phase {
        let phase_guard = match <Self as Sequentializer<T>>::lock(s, |p| {
            if shall_init(p) {
                LockNature::Write
            } else {
                LockNature::None
            }
        }) {
            LockResult::None(p) => return p,
            LockResult::Write(l) => l,
            LockResult::Read(_) => unsafe { unreachable_unchecked() },
        };

        let ph = lazy_initialization(phase_guard, init, reg, init_on_reg_failure);
        ph.phase()
    }
    #[inline(always)]
    fn init_then_read_guard(
        s: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Self::ReadGuard {
        match <Self as Sequentializer<T>>::lock(s, |p| {
            if shall_init(p) {
                LockNature::Write
            } else {
                LockNature::Read
            }
        }) {
            LockResult::Read(l) => l,
            LockResult::Write(l) => {
                let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                l.into()
            }
            LockResult::None(_) => unsafe { unreachable_unchecked() },
        }
    }
    #[inline(always)]
    fn init_then_write_guard(
        s: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Self::WriteGuard {
        match <Self as Sequentializer<T>>::lock(s, |_| LockNature::Write) {
            LockResult::Write(l) => {
                if shall_init(l.phase()) {
                    lazy_initialization(l, init, reg, init_on_reg_failure)
                } else {
                    l
                }
            }
            LockResult::Read(_) => unsafe { unreachable_unchecked() },
            LockResult::None(_) => unsafe { unreachable_unchecked() },
        }
    }
    #[inline(always)]
    fn try_init_then_read_guard(
        s: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Option<Self::ReadGuard> {
        <Self as Sequentializer<T>>::try_lock(s, |p| {
            if shall_init(p) {
                LockNature::Write
            } else {
                LockNature::Read
            }
        }).map(|l| match l {
            LockResult::Read(l) => l,
            LockResult::Write(l) => {
                let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                l.into()
            }
            LockResult::None(_) => unsafe { unreachable_unchecked() },
        })
    }
    #[inline(always)]
    fn try_init_then_write_guard(
        s: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Option<Self::WriteGuard> {
        <Self as Sequentializer<T>>::try_lock(s, |_| LockNature::Write).map(|l| match l {
            LockResult::Write(l) => {
                if shall_init(l.phase()) {
                    lazy_initialization(l, init, reg, init_on_reg_failure)
                } else {
                    l
                }
            }
            LockResult::Read(_) => unsafe { unreachable_unchecked() },
            LockResult::None(_) => unsafe { unreachable_unchecked() },
        })
    }

    fn finalize_callback(s: &T, f: impl FnOnce(&T::Data)) {
        let this = Sequential::sequentializer(s).as_ref();

        let phase_guard = match this.0.lock(Sequential::data(s), |p| {
            if (p
                & (
                    Phase::FINALIZED
                    | Phase::FINALIZATION_PANICKED
                    ))
                .is_empty() && p.intersects(Phase::INITIALIZED)
            {
                LockNature::Write
            } else {
                LockNature::None
            }
        }) {
            LockResult::None(_) => return,
            LockResult::Write(l) => l,
            LockResult::Read(_) => unsafe { unreachable_unchecked() },
        };

        lazy_finalization(phase_guard, f);
    }
}

mod global_once {
    use super::{lazy_finalization, lazy_initialization};
    use super::{Phase, Phased, Sequential, Sequentializer, SplitedLazySequentializer};
    use crate::mutex::{
        LockNature, LockResult, PhaseGuard, SyncPhaseGuard, SyncPhasedLocker as PhasedLocker,
        SyncReadPhaseGuard,
    };

    use core::hint::unreachable_unchecked;
    #[cfg(debug_mode)]
    use core::sync::atomic::{AtomicUsize, Ordering};
    #[cfg(debug_mode)]
    use crate::CyclicPanic;

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
    pub struct SyncSequentializer(PhasedLocker, #[cfg(debug_mode)] AtomicUsize);

    impl Phased for SyncSequentializer {
        #[inline(always)]
        fn phase(this: &Self) -> Phase {
            this.0.phase()
        }
    }
    impl SyncSequentializer {
        #[inline(always)]
        pub const fn new() -> Self {
            Self(
                PhasedLocker::new(Phase::empty()),
                #[cfg(debug_mode)]
                AtomicUsize::new(0),
            )
        }
    }

    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<'a, T: Sequential + 'a> Sequentializer<'a, T> for SyncSequentializer
    where
        T::Sequentializer: AsRef<SyncSequentializer>,
    {
        type ReadGuard = SyncReadPhaseGuard<'a, T>;
        type WriteGuard = SyncPhaseGuard<'a, T>;

        fn lock(
            s: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> LockResult<SyncReadPhaseGuard<'a, T>, SyncPhaseGuard<'a, T>> {
            let this = Sequential::sequentializer(s).as_ref();

            this.0.lock(
                s,
                &lock_nature,
                &lock_nature,
                Phase::INITIALIZED | Phase::REGISTERED,
            )
        }

        fn try_lock(
            s: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> Option<LockResult<SyncReadPhaseGuard<'a, T>, SyncPhaseGuard<'a, T>>> {
            let this = Sequential::sequentializer(s).as_ref();

            this.0.try_lock(
                s,
                &lock_nature,
                Phase::INITIALIZED | Phase::REGISTERED,
            )
        }
    }

    #[inline(always)]
    fn debug_save_thread<T:Sequential> (_s: &T) 
        where T::Sequentializer: AsRef<SyncSequentializer>
        {
            #[cfg(debug_mode)]
            {
                let this = Sequential::sequentializer(_s).as_ref();
                use parking_lot::lock_api::GetThreadId;
                this.1.store(
                    parking_lot::RawThreadId.nonzero_thread_id().into(),
                    Ordering::Relaxed,
                );
            }
    }
    #[inline(always)]
    fn debug_thread_zero<T:Sequential> (_s: &T) 
        where T::Sequentializer: AsRef<SyncSequentializer>
        {
            #[cfg(debug_mode)]
            {
                let this = Sequential::sequentializer(_s).as_ref();
                this.1.store(0, Ordering::Relaxed);
            }
    }
    #[inline(always)]
    fn debug_test<T: Sequential> (_s: &T) 
        where T::Sequentializer: AsRef<SyncSequentializer>
        {
          #[cfg(debug_mode)]
          {
              let this = Sequential::sequentializer(_s).as_ref();
              let id = this.1.load(Ordering::Relaxed);
              if id != 0 {
                  use parking_lot::lock_api::GetThreadId;
                  if id == parking_lot::RawThreadId.nonzero_thread_id().into() {
                      std::panic::panic_any(CyclicPanic);
                  }
              }
          }
    }
    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<'a, T: Sequential + 'a> SplitedLazySequentializer<'a, T> for SyncSequentializer
    where
        T::Sequentializer: AsRef<SyncSequentializer>,
    {
        #[inline(always)]
        fn init(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> Phase {
            let this = Sequential::sequentializer(s).as_ref();

            let phase_guard = match this.0.lock(
                s,
                |p| {
                    if shall_init(p) {
                        debug_test(s);
                        LockNature::Write
                    } else {
                        LockNature::None
                    }
                },
                |_| LockNature::Read,
                Phase::INITIALIZED | Phase::REGISTERED,
            ) {
                LockResult::None(p) => return p,
                LockResult::Write(l) => l,
                LockResult::Read(l) => return l.phase(),
            };

            debug_save_thread(s);
            let ph = lazy_initialization(phase_guard, init, reg, init_on_reg_failure);
            debug_thread_zero(s);
            ph.phase()
        }
        #[inline(always)]
        fn init_then_read_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> Self::ReadGuard {
            let this = Sequential::sequentializer(s).as_ref();

            match this.0.lock(
                s,
                |p| {
                    if shall_init(p) {
                        debug_test(s);
                        LockNature::Write
                    } else {
                        LockNature::Read
                    }
                },
                |_| LockNature::Read,
                Phase::INITIALIZED | Phase::REGISTERED,
            ) {
                LockResult::Read(l) => l,
                LockResult::Write(l) => {
                    debug_save_thread(s);
                    let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                    debug_thread_zero(s);
                    l.into()
                }
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            }
        }
        #[inline(always)]
        fn init_then_write_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> Self::WriteGuard {
            match <Self as Sequentializer<T>>::lock(s, |_| LockNature::Write) {
                LockResult::Write(l) => {
                    if shall_init(l.phase()) {
                        debug_test(s);
                        debug_save_thread(s);
                        let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                        debug_thread_zero(s);
                        l
                    } else {
                        l
                    }
                }
                LockResult::Read(_) => unsafe { unreachable_unchecked() },
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            }
        }

        #[inline(always)]
        fn try_init_then_read_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> Option<Self::ReadGuard> {
            let this = Sequential::sequentializer(s).as_ref();

            this.0.try_lock(
                s,
                |p| {
                    if shall_init(p) {
                        debug_test(s);
                        LockNature::Write
                    } else {
                        LockNature::Read
                    }
                },
                Phase::INITIALIZED | Phase::REGISTERED,
            ).map(|l| match l {
                LockResult::Read(l) => l,
                LockResult::Write(l) => {
                    debug_save_thread(s);
                    let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                    debug_thread_zero(s);
                    l.into()
                }
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            })
        }
        #[inline(always)]
        fn try_init_then_write_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> Option<Self::WriteGuard> {
            <Self as Sequentializer<T>>::try_lock(s, |_| LockNature::Write).map(|l| match l {
                LockResult::Write(l) => {
                    if shall_init(l.phase()) {
                        debug_test(s);
                        debug_save_thread(s);
                        let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                        debug_thread_zero(s);
                        l
                    } else {
                        l
                    }
                }
                LockResult::Read(_) => unsafe { unreachable_unchecked() },
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            })
        }
        #[inline(always)]

        fn finalize_callback(s: &T, f: impl FnOnce(&T::Data)) {
            let this = Sequential::sequentializer(s).as_ref();

            let how = |p: Phase| {
                if (p
                    & (
                         Phase::FINALIZED
                        | Phase::FINALIZATION_PANICKED
                        ))
                    .is_empty() && p.intersects(Phase::INITIALIZED)
                {
                    LockNature::Write
                } else {
                    LockNature::None
                }
            };

            let phase_guard = match this.0.lock(
                Sequential::data(s),
                how,
                how,
                Phase::INITIALIZED | Phase::REGISTERED,
            ) {
                LockResult::None(_) => return,
                LockResult::Write(l) => l,
                LockResult::Read(_) => unsafe { unreachable_unchecked() },
            };

            lazy_finalization(phase_guard, f);
        }
    }
}
//TODO: diviser en deux SyncSequentializer && ProgramInitedSyncSequentializer
pub use global_once::SyncSequentializer;

#[inline(never)]
#[cold]
fn lazy_initialization<'a, P: PhaseGuard<'a, S>, S: Sequential + 'a>(
    mut phase_guard: P,
    init: impl FnOnce(&'a <S as Sequential>::Data),
    reg: impl FnOnce(&'a S) -> bool,
    init_on_reg_failure: bool,
) -> P
where
    <S as Sequential>::Data: 'a,
{
    let cur = phase_guard.phase();

    let registration_finished = cur;

    let registration_failed = cur | Phase::REGISTRATION_PANICKED | Phase::INITIALIZATION_SKIPED;

    let cond = phase_guard.transition(reg, registration_finished, registration_failed);

    if cond {

        let initialized = registration_finished | Phase::REGISTERED | Phase::INITIALIZED;

        let initialization_panic = registration_finished
            | Phase::REGISTERED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;


        phase_guard.transition(
            |s| init(Sequential::data(s)),
            initialized,
            initialization_panic,
        );
    } else if init_on_reg_failure {

        let initialized = registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZED;

        let initialization_panic = registration_finished
            | Phase::REGISTRATION_REFUSED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;

        phase_guard.transition(
            |s| init(Sequential::data(s)),
            initialized,
            initialization_panic,
        );
    } else {
        let no_init =
            registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZATION_SKIPED;

        phase_guard.set_phase(no_init);
    }
    phase_guard
}

fn lazy_finalization<'a, T: 'a, P: PhaseGuard<'a, T>>(mut phase_guard: P, f: impl FnOnce(&'a T)) {
    let cur = phase_guard.phase();

    let finalizing_success = cur | Phase::FINALIZED;

    let finalizing_failed = cur | Phase::FINALIZATION_PANICKED;

    phase_guard.transition(f, finalizing_success, finalizing_failed);
}
