use crate::phase_locker::SyncPhaseLocker;
use crate::phase_locker::{PhaseGuard, UnSyncPhaseLocker};
use crate::{Phase, Sequential};

pub type SyncSequentializer = generic::LazySequentializer<SyncPhaseLocker>;
pub type UnSyncSequentializer = generic::LazySequentializer<UnSyncPhaseLocker>;

#[inline]
#[cold]
fn lazy_initialization_only<'a, T: 'a, P: PhaseGuard<'a, T>>(
    mut phase_guard: P,
    init: impl FnOnce(&'a T),
) -> P {
    let cur = phase_guard.phase();

    let initialized = cur | Phase::INITIALIZED;

    let initialization_panic = cur | Phase::INITIALIZATION_PANICKED | Phase::INITIALIZATION_SKIPED;

    unsafe { phase_guard.transition(init, initialized, initialization_panic) };

    phase_guard
}

#[inline]
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

    let cond = unsafe { phase_guard.transition(reg, registration_finished, registration_failed) };

    if cond {
        let initialized = registration_finished | Phase::REGISTERED | Phase::INITIALIZED;

        let initialization_panic = registration_finished
            | Phase::REGISTERED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;

        unsafe {
            phase_guard.transition(
                |s| init(Sequential::data(s)),
                initialized,
                initialization_panic,
            )
        };
    } else if init_on_reg_failure {
        let initialized = registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZED;

        let initialization_panic = registration_finished
            | Phase::REGISTRATION_REFUSED
            | Phase::INITIALIZATION_PANICKED
            | Phase::INITIALIZATION_SKIPED;

        unsafe {
            phase_guard.transition(
                |s| init(Sequential::data(s)),
                initialized,
                initialization_panic,
            )
        };
    } else {
        let no_init =
            registration_finished | Phase::REGISTRATION_REFUSED | Phase::INITIALIZATION_SKIPED;

        unsafe { phase_guard.set_phase(no_init) };
    }
    phase_guard
}

fn lazy_finalization<'a, T: 'a, P: PhaseGuard<'a, T>>(mut phase_guard: P, f: impl FnOnce(&'a T)) {
    let cur = phase_guard.phase();

    let finalizing_success = cur | Phase::FINALIZED;

    let finalizing_failed = cur | Phase::FINALIZATION_PANICKED;

    unsafe { phase_guard.transition(f, finalizing_success, finalizing_failed) };
}

mod generic {
    use super::{lazy_finalization, lazy_initialization, lazy_initialization_only};
    use crate::{
        LazySequentializer as LazySequentializerTrait, Phase, Phased, Sequential, Sequentializer,
        FinalizableLazySequentializer, InitResult
    };
    use crate::phase_locker::{LockNature, LockResult, Mappable, PhaseGuard, PhaseLocker};

    #[cfg(debug_mode)]
    use crate::{CyclicPanic};
    use core::hint::unreachable_unchecked;
    #[cfg(debug_mode)]
    use core::sync::atomic::{AtomicUsize, Ordering};

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
    pub struct LazySequentializer<Locker>(Locker, #[cfg(debug_mode)] AtomicUsize);

    impl<L> Phased for LazySequentializer<L>
    where
        L: Phased,
    {
        #[inline(always)]
        fn phase(this: &Self) -> Phase {
            Phased::phase(&this.0)
        }
    }
    impl<L> LazySequentializer<L> {
        #[inline(always)]
        pub const fn new(locker: L) -> Self {
            Self(
                locker,
                #[cfg(debug_mode)]
                AtomicUsize::new(0),
            )
        }
    }

    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<'a, T: Sequential + 'a, L: 'static> Sequentializer<'a, T> for LazySequentializer<L>
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
        T::Sequentializer: AsMut<LazySequentializer<L>>,
        L: PhaseLocker<'a, T::Data>,
        L: Phased,
    {
        type ReadGuard = L::ReadGuard;
        type WriteGuard = L::WriteGuard;

        #[inline(always)]
        fn lock(
            s: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> LockResult<Self::ReadGuard, Self::WriteGuard> {
            let this = Sequential::sequentializer(s).as_ref();

            let data = Sequential::data(s);

            this.0.lock(
                data,
                &lock_nature,
                &lock_nature,
                Phase::INITIALIZED | Phase::REGISTERED,
            )
        }

        #[inline(always)]
        fn try_lock(
            s: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> Option<LockResult<Self::ReadGuard, Self::WriteGuard>> {
            let this = Sequential::sequentializer(s).as_ref();

            let data = Sequential::data(s);

            this.0
                .try_lock(data, &lock_nature, Phase::INITIALIZED | Phase::REGISTERED)
        }

        #[inline(always)]
        fn lock_mut(s: &'a mut T) -> Self::WriteGuard {
            let (that, data) = Sequential::sequentializer_data_mut(s);

            that.as_mut().0.lock_mut(data)
        }
    }

    #[inline(always)]
    fn whole_lock<'a, T: Sequential + 'a, L: 'static>(
        s: &'a T,
        lock_nature: impl Fn(Phase) -> LockNature,
    ) -> LockResult<L::ReadGuard, L::WriteGuard>
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
        T::Sequentializer: AsMut<LazySequentializer<L>>,
        L: PhaseLocker<'a, T>,
    {
        let this = Sequential::sequentializer(s).as_ref();

        this.0.lock(
            s,
            &lock_nature,
            &lock_nature,
            Phase::INITIALIZED | Phase::REGISTERED,
        )
    }

    #[inline(always)]
    fn try_whole_lock<'a, T: Sequential + 'a, L: 'static>(
        s: &'a T,
        lock_nature: impl Fn(Phase) -> LockNature,
    ) -> Option<LockResult<L::ReadGuard, L::WriteGuard>>
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
        T::Sequentializer: AsMut<LazySequentializer<L>>,
        L: PhaseLocker<'a, T>,
    {
        let this = Sequential::sequentializer(s).as_ref();

        this.0
            .try_lock(s, &lock_nature, Phase::INITIALIZED | Phase::REGISTERED)
    }

    #[inline(always)]
    fn debug_save_thread<T: Sequential, L>(_s: &T)
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
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
    fn debug_thread_zero<T: Sequential, L>(_s: &T)
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
    {
        #[cfg(debug_mode)]
        {
            let this = Sequential::sequentializer(_s).as_ref();
            this.1.store(0, Ordering::Relaxed);
        }
    }
    #[inline(always)]
    fn debug_test<T: Sequential, L>(_s: &T)
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
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
    unsafe impl<'a, T: Sequential + 'a, L: 'static> FinalizableLazySequentializer<'a, T>
        for LazySequentializer<L>
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
        T::Sequentializer: AsMut<LazySequentializer<L>>,
        L: PhaseLocker<'a, T>,
        L: PhaseLocker<'a, T::Data>,
        L: Phased,
        <L as PhaseLocker<'a, T>>::ReadGuard:
            Mappable<T, T::Data, <L as PhaseLocker<'a, T::Data>>::ReadGuard>,
        <L as PhaseLocker<'a, T>>::WriteGuard:
            Mappable<T, T::Data, <L as PhaseLocker<'a, T::Data>>::WriteGuard>,
        <L as PhaseLocker<'a, T::Data>>::ReadGuard:
            From<<L as PhaseLocker<'a, T::Data>>::WriteGuard>,
    {
        #[inline(always)]
        fn init(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> InitResult<Phase> {
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
                LockResult::None(p) => return InitResult{initer:false, result:p},
                LockResult::Write(l) => l,
                LockResult::Read(l) => return InitResult{initer:false, result:Phased::phase(&l)},
            };

            debug_save_thread(s);
            let ph = lazy_initialization(phase_guard, init, reg, init_on_reg_failure);
            debug_thread_zero(s);
            InitResult{initer:true,result:ph.phase()}
        }

        #[inline(always)]
        fn only_init(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Phase> {
            let this = Sequential::sequentializer(s).as_ref();

            let phase_guard = match this.0.lock(
                Sequential::data(s),
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
                LockResult::None(p) => return InitResult{initer:false, result:p},
                LockResult::Write(l) => l,
                LockResult::Read(l) => return InitResult{initer:false, result:Phased::phase(&l)},
            };

            debug_save_thread(s);
            let ph = lazy_initialization_only(phase_guard, init);
            debug_thread_zero(s);
            InitResult{initer:true,result:ph.phase()}
        }

        #[inline(always)]
        fn only_init_unique(
            s: &'a mut T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Phase> {
            let phase_guard = <Self as Sequentializer<'a, T>>::lock_mut(s);

            if shall_init(phase_guard.phase()) {
                let ph = lazy_initialization_only(phase_guard, init);
                InitResult{initer:true,result:ph.phase()}
            } else {
                InitResult{initer:false, result:phase_guard.phase()}
            }
        }

        #[inline(always)]
        fn init_then_read_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
            reg: impl FnOnce(&'a T) -> bool,
            init_on_reg_failure: bool,
        ) -> InitResult<Self::ReadGuard> {
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
                LockResult::Read(l) => InitResult{initer:false, result:l.map(|s| Sequential::data(s))},
                LockResult::Write(l) => {
                    debug_save_thread(s);
                    let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                    debug_thread_zero(s);
                    InitResult{initer:true,result:l.map(|s| Sequential::data(s)).into()}
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
        ) -> InitResult<Self::WriteGuard> {
            match whole_lock(s, |_| LockNature::Write) {
                LockResult::Write(l) => if shall_init(l.phase()) {
                    debug_test(s);
                    debug_save_thread(s);
                    let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                    debug_thread_zero(s);
                    InitResult{initer:true,result:l.map(|s| Sequential::data(s))}
                } else {
                    InitResult{initer:false, result:l.map(|s| Sequential::data(s))}
                }
                ,
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
        ) -> Option<InitResult<Self::ReadGuard>> {
            let this = Sequential::sequentializer(s).as_ref();

            this.0
                .try_lock(
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
                )
                .map(|l| match l {
                    LockResult::Read(l) => InitResult{initer:false, result:l.map(|s| Sequential::data(s))},
                    LockResult::Write(l) => {
                        debug_save_thread(s);
                        let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                        debug_thread_zero(s);
                        InitResult{initer:true,result:l.map(|s| Sequential::data(s)).into()}
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
        ) -> Option<InitResult<Self::WriteGuard>> {
            try_whole_lock(s, |_| LockNature::Write).map(|l| match l {
                LockResult::Write(l) => if shall_init(l.phase()) {
                    debug_test(s);
                    debug_save_thread(s);
                    let l = lazy_initialization(l, init, reg, init_on_reg_failure);
                    debug_thread_zero(s);
                    InitResult{initer:true,result:l.map(|s| Sequential::data(s))}
                } else {
                    InitResult{initer:false, result:l.map(|s| Sequential::data(s))}
                }
                ,
                LockResult::Read(_) => unsafe { unreachable_unchecked() },
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            })
        }
        #[inline(always)]

        fn finalize_callback(s: &'a T, f: impl FnOnce(&'a T::Data)) {
            let this = Sequential::sequentializer(s).as_ref();

            let how = |p: Phase| {
                if (p & (Phase::FINALIZED | Phase::FINALIZATION_PANICKED)).is_empty()
                    && p.intersects(Phase::INITIALIZED)
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

    impl<L> AsRef<LazySequentializer<L>> for LazySequentializer<L> {
        #[inline(always)]
        fn as_ref(&self) -> &Self {
            self
        }
    }
    impl<L> AsMut<LazySequentializer<L>> for LazySequentializer<L> {
        #[inline(always)]
        fn as_mut(&mut self) -> &mut Self {
            self
        }
    }

    // SAFETY: it is safe because it does implement synchronized locks
    unsafe impl<'a, T: Sequential + 'a, L: 'static> LazySequentializerTrait<'a, T>
        for LazySequentializer<L>
    where
        T::Sequentializer: AsRef<LazySequentializer<L>>,
        T::Sequentializer: AsMut<LazySequentializer<L>>,
        L: PhaseLocker<'a, T>,
        L: PhaseLocker<'a, T::Data>,
        L: Phased,
        <L as PhaseLocker<'a, T>>::ReadGuard:
            Mappable<T, T::Data, <L as PhaseLocker<'a, T::Data>>::ReadGuard>,
        <L as PhaseLocker<'a, T>>::WriteGuard:
            Mappable<T, T::Data, <L as PhaseLocker<'a, T::Data>>::WriteGuard>,
        <L as PhaseLocker<'a, T::Data>>::ReadGuard:
            From<<L as PhaseLocker<'a, T::Data>>::WriteGuard>,
    {
        #[inline(always)]
        fn init(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Phase> {
            <Self as FinalizableLazySequentializer<'a, T>>::only_init(s, shall_init, init)
        }

        #[inline(always)]
        fn init_unique(
            s: &'a mut T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Phase> {
            <Self as FinalizableLazySequentializer<'a, T>>::only_init_unique(s, shall_init, init)
        }

        #[inline(always)]
        fn init_then_read_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Self::ReadGuard> {
            let this = Sequential::sequentializer(s).as_ref();

            match this.0.lock(
                Sequential::data(s),
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
                LockResult::Read(l) => InitResult{initer:false, result:l},
                LockResult::Write(l) => {
                    debug_save_thread(s);
                    let l = lazy_initialization_only(l, init);
                    debug_thread_zero(s);
                    InitResult{initer:true,result:l.into()}
                }
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            }
        }
        #[inline(always)]
        fn init_then_write_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> InitResult<Self::WriteGuard> {
            match <Self as Sequentializer<'a, T>>::lock(s, |_| LockNature::Write) {
                LockResult::Write(l) => {
                    if shall_init(l.phase()) {
                        debug_test(s);
                        debug_save_thread(s);
                        let l = lazy_initialization_only(l, init);
                        debug_thread_zero(s);
                        InitResult{initer:true, result:l}
                    } else {
                        InitResult{initer:false,result:l}
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
        ) -> Option<InitResult<Self::ReadGuard>> {
            let this = Sequential::sequentializer(s).as_ref();

            this.0
                .try_lock(
                    Sequential::data(s),
                    |p| {
                        if shall_init(p) {
                            debug_test(s);
                            LockNature::Write
                        } else {
                            LockNature::Read
                        }
                    },
                    Phase::INITIALIZED | Phase::REGISTERED,
                )
                .map(|l| match l {
                    LockResult::Read(l) => InitResult{initer:false, result:l},
                    LockResult::Write(l) => {
                        debug_save_thread(s);
                        let l = lazy_initialization_only(l, init);
                        debug_thread_zero(s);
                        InitResult{initer:true,result:l.into()}
                    }
                    LockResult::None(_) => unsafe { unreachable_unchecked() },
                })
        }
        #[inline(always)]
        fn try_init_then_write_guard(
            s: &'a T,
            shall_init: impl Fn(Phase) -> bool,
            init: impl FnOnce(&'a <T as Sequential>::Data),
        ) -> Option<InitResult<Self::WriteGuard>> {
            <Self as Sequentializer<'a, T>>::try_lock(s, |_| LockNature::Write).map(|l| match l {
                LockResult::Write(l) => {
                    if shall_init(l.phase()) {
                        debug_test(s);
                        debug_save_thread(s);
                        let l = lazy_initialization_only(l, init);
                        debug_thread_zero(s);
                        InitResult{initer:true, result:l}
                    } else {
                        InitResult{initer:false,result:l}
                    }
                }
                LockResult::Read(_) => unsafe { unreachable_unchecked() },
                LockResult::None(_) => unsafe { unreachable_unchecked() },
            })
        }
    }
}
