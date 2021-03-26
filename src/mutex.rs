#[cfg(all(
    not(feature = "parking_lot_core"),
    any(target_os = "linux", target_os = "android")
))]
mod linux {
    use core::ptr;
    use core::sync::atomic::AtomicU32;
    use libc::{
        sched_yield, syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT_BITSET, FUTEX_WAKE_BITSET,
    };

    pub(super) struct Parker {
        futex: AtomicU32,
    }

    impl Parker {
        pub(super) const fn new(value: u32) -> Self {
            Self {
                futex: AtomicU32::new(value),
            }
        }

        pub(super) fn value(&self) -> &AtomicU32 {
            &self.futex
        }
        pub(super) fn park_reader(&self, value: u32) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    1,
                ) == 0
            }
        }
        pub(super) fn park_writer(&self, value: u32) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    2,
                ) == 0
            }
        }
        pub(super) fn unpark_readers(&self) -> u32 {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    i32::MAX,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    1,
                ) as u32
            }
        }
        pub(super) fn unpark_one_writer(&self) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    1,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    2,
                ) == 1
            }
        }
    }
    pub(super) fn yield_now() {
        unsafe {
            sched_yield();
        }
    }
}
#[cfg(all(
    not(feature = "parking_lot_core"),
    any(target_os = "linux", target_os = "android")
))]
use linux::{Parker, yield_now};

#[cfg(feature = "parking_lot_core")]
mod other {
    use core::sync::atomic::{AtomicU32, Ordering};
    use parking_lot_core::{
        park, unpark_all, unpark_one, ParkResult, DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN,
    };

    pub(super) struct Parker(AtomicU32);

    impl Parker {
        pub(super) const fn new(value: u32) -> Self {
            Self(AtomicU32::new(value))
        }

        pub(super) fn value(&self) -> &AtomicU32 {
            &self.0
        }
        pub(super) fn park_reader(&self, value: u32) -> bool {
            unsafe {
                matches!(
                    park(
                        &self.0 as *const _ as usize,
                        || self.0.load(Ordering::Relaxed) == value,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    ),
                    ParkResult::Unparked(_)
                )
            }
        }
        pub(super) fn park_writer(&self, value: u32) -> bool {
            unsafe {
                matches!(
                    park(
                        (&self.0 as *const _ as usize) + 1,
                        || self.0.load(Ordering::Relaxed) == value,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    ),
                    ParkResult::Unparked(_)
                )
            }
        }
        pub(super) fn unpark_readers(&self) -> u32 {
            unsafe { unpark_all(&self.0 as *const _ as usize, DEFAULT_UNPARK_TOKEN) as u32 }
        }
        pub(super) fn unpark_one_writer(&self) -> bool {
            unsafe {
                let r = unpark_one((&self.0 as *const _ as usize) + 1, |_| DEFAULT_UNPARK_TOKEN);
                r.unparked_threads == 1
            }
        }
    }
}
#[cfg(feature = "parking_lot_core")]
use other::Parker;
#[cfg(feature = "parking_lot_core")]
use std::thread::yield_now;

mod spin_wait {
    use super::yield_now;
    use core::hint;
    // Extracted from parking_lot_core
    //
    #[inline]
    fn cpu_relax(iterations: u32) {
        for _ in 0..iterations {
            hint::spin_loop()
        }
    }

    /// A counter used to perform exponential backoff in spin loops.
    #[derive(Default)]
    pub(super) struct SpinWait {
        counter: u32,
    }

    impl SpinWait {
        /// Creates a new `SpinWait`.
        #[inline]
        pub fn new() -> Self {
            Self::default()
        }

        /// Resets a `SpinWait` to its initial state.
        #[inline]
        pub fn reset(&mut self) {
            self.counter = 0;
        }

        /// Spins until the sleep threshold has been reached.
        ///
        /// This function returns whether the sleep threshold has been reached, at
        /// which point further spinning has diminishing returns and the thread
        /// should be parked instead.
        ///
        /// The spin strategy will initially use a CPU-bound loop but will fall back
        /// to yielding the CPU to the OS after a few iterations.
        #[inline]
        pub fn spin(&mut self) -> bool {
            if self.counter >= 10 {
                return false;
            }
            self.counter += 1;
            if self.counter <= 3 {
                cpu_relax(1 << self.counter);
            } else {
                yield_now();
            }
            true
        }

        ///// Spins without yielding the thread to the OS.
        /////
        ///// Instead, the backoff is simply capped at a maximum value. This can be
        ///// used to improve throughput in `compare_exchange` loops that have high
        ///// contention.
        //#[inline]
        //pub fn spin_no_yield(&mut self) {
        //    self.counter += 1;
        //    if self.counter > 10 {
        //        self.counter = 10;
        //    }
        //    cpu_relax(1 << self.counter);
        //}
    }
}
use super::Phase;

/// A phase guard ensure that the target object will
/// performed atomic phase transition
///
/// # Safety
///
/// The trait is unsafe because the implementation must fullfill the
/// following requirement described in the documentation of the functions
pub unsafe trait PhaseGuard<'a, T: ?Sized + 'a> {
    /// Set the phase at which will be the traget object
    /// when the phase guard will be dropped
    fn set_phase(&mut self, p: Phase);
    /// Set the phase of the target object with release semantic if the
    /// PhaseGuard is Sync
    fn commit_phase(&mut self);
    /// Return the phase at which will be the object
    fn phase(&self) -> Phase;
    /// Execute the function f then:
    ///   - if execution of f does not panic change, call Self::set_phase(on_success)
    ///   - if execution of f panics: set the phase of the target object to on_panic and
    ///   release the lock.
    fn transition<R>(
        &mut self,
        f: impl FnOnce(&'a T) -> R,
        on_success: Phase,
        on_panic: Phase,
    ) -> R;
}

/// Nature of the lock requested
pub enum LockNature {
    Read,
    Write,
    None,
}
/// Result of a Phased locking
pub enum LockResult<R, W> {
    Read(R),
    Write(W),
    None,
}

mod mutex {
    use super::spin_wait::SpinWait;
    use super::Parker;
    use super::{LockNature, LockResult, PhaseGuard};
    use crate::phase::*;
    use crate::Phase;
    use core::cell::UnsafeCell;
    use core::mem::forget;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::{fence, Ordering};

    #[cfg(debug_mode)]
    use crate::CyclicPanic;
    #[cfg(debug_mode)]
    use core::sync::atomic::AtomicUsize;

    /// A phase locker.
    pub struct SyncPhasedLocker(Parker);

    pub(crate) struct Mutex<T>(UnsafeCell<T>, SyncPhasedLocker);

    unsafe impl<T: Send> Sync for Mutex<T> {}

    unsafe impl<T: Send> Send for Mutex<T> {}

    pub(crate) struct Lock<'a> {
        state:     &'a SyncPhasedLocker,
        on_unlock: Phase,
    }
    pub(crate) struct ReadLock<'a> {
        state:      &'a SyncPhasedLocker,
        init_phase: Phase,
    }
    pub(crate) struct MutexGuard<'a, T>(&'a mut T, Lock<'a>);

    /// A phase guard that allow atomic phase transition that
    /// can be turned fastly into a [SyncReadPhaseGuard].
    pub struct SyncPhaseGuard<'a, T: ?Sized>(&'a T, Lock<'a>);

    /// A kind of read lock.
    pub struct SyncReadPhaseGuard<'a, T: ?Sized>(&'a T, ReadLock<'a>);

    impl<'a, T> Deref for SyncPhaseGuard<'a, T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a, T: ?Sized> SyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn new(r: &'a T, lock: Lock<'a>) -> Self {
            Self(r, lock)
        }
    }
    unsafe impl<'a, T: ?Sized> PhaseGuard<'a, T> for SyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn set_phase(&mut self, p: Phase) {
            self.1.on_unlock = p;
        }
        #[inline(always)]
        fn commit_phase(&mut self) {
            //Butter fly trick
            let cur = self.1.phase();
            let to_xor = self.1.on_unlock ^ cur;
            self.1.xor_phase(to_xor);
        }
        #[inline(always)]
        fn phase(&self) -> Phase {
            self.1.on_unlock
        }
        #[inline(always)]
        fn transition<R>(
            &mut self,
            f: impl FnOnce(&'a T) -> R,
            on_success: Phase,
            on_panic: Phase,
        ) -> R {
            self.1.on_unlock = on_panic;
            let res = f(self.0);
            self.1.on_unlock = on_success;
            res
        }
    }

    impl<'a, T> Deref for SyncReadPhaseGuard<'a, T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a, T: ?Sized> SyncReadPhaseGuard<'a, T> {
        #[inline(always)]
        fn new(r: &'a T, lock: ReadLock<'a>) -> Self {
            Self(r, lock)
        }
        #[inline(always)]
        pub fn phase(this: &Self) -> Phase {
            this.1.init_phase
        }
    }
    impl<'a, T> Into<SyncReadPhaseGuard<'a, T>> for SyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn into(self) -> SyncReadPhaseGuard<'a, T> {
            SyncReadPhaseGuard(self.0, self.1.into())
        }
    }

    impl<T> Mutex<T> {
        #[inline(always)]
        pub(crate) const fn new(value: T) -> Self {
            Self(
                UnsafeCell::new(value),
                SyncPhasedLocker::new(Phase::empty()),
            )
        }
        #[inline(always)]
        pub(crate) fn lock(&self) -> MutexGuard<'_, T> {
            let lk = if let LockResult::Write(l) = {
                #[cfg(not(debug_mode))]
                {
                    self.1.raw_lock(|_p| LockNature::Write, Phase::empty())
                }
                #[cfg(debug_mode)]
                {
                    let id = AtomicUsize::new(0);
                    self.1.raw_lock(|_p| LockNature::Write, Phase::empty(), &id)
                }
            } {
                l
            } else {
                unreachable!()
            };
            MutexGuard(unsafe { &mut *self.0.get() }, lk)
        }
    }

    impl<'a, T> Deref for MutexGuard<'a, T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }
    impl<'a, T> DerefMut for MutexGuard<'a, T> {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            self.0
        }
    }

    impl<'a> Lock<'a> {
        #[inline(always)]
        pub fn phase(&self) -> Phase {
            let v = self.state.0.value().load(Ordering::Relaxed);
            Phase::from_bits_truncate(v)
        }
        #[inline(always)]
        pub fn xor_phase(&self, xor: Phase) -> Phase {
            let v = self
                .state
                .0
                .value()
                .fetch_xor(xor.bits(), Ordering::Release);
            Phase::from_bits_truncate(v) ^ xor
        }
    }

    impl<'a> Lock<'a> {
        #[inline(always)]
        fn into_read_lock(self, cur: u32) -> ReadLock<'a> {
            let p = Phase::from_bits_truncate(cur);
            let xor = (p ^ self.on_unlock).bits() | LOCKED_BIT | READER_UNITY;
            let prev = self.state.0.value().fetch_xor(xor, Ordering::Release);

            if prev & PARKED_BIT != 0 {
                self.state
                    .0
                    .value()
                    .fetch_xor(PARKED_BIT | LOCKED_BIT, Ordering::Release);
                let c = self.state.0.unpark_readers();
                self.state
                    .0
                    .value()
                    .fetch_sub(LOCKED_BIT - READER_UNITY * (c + 1), Ordering::Release);
            }

            let r = ReadLock {
                state:      self.state,
                init_phase: Phase::from_bits_truncate(prev),
            };
            forget(self);
            r
        }
    }
    impl<'a> Lock<'a> {
        #[cold]
        #[inline(never)]
        fn drop_slow(&mut self, mut cur: u32) {
            // try to reaquire the lock
            if let Err(_) = self.state.0.value().compare_exchange(
                cur,
                cur | LOCKED_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                return;
            }

            cur |= LOCKED_BIT;

            loop {
                if cur & PARKED_BIT != 0 {
                    self.state
                        .0
                        .value()
                        .fetch_and(!PARKED_BIT, Ordering::Relaxed);
                    let c = self.state.0.unpark_readers();
                    self.state
                        .0
                        .value()
                        .fetch_sub(LOCKED_BIT - READER_UNITY * (c + 1), Ordering::Relaxed);
                    drop(ReadLock {
                        state:      self.state,
                        init_phase: Phase::from_bits_truncate(cur),
                    });
                    return;
                }
                if cur & WPARKED_BIT != 0 {
                    self.state
                        .0
                        .value()
                        .fetch_and(!WPARKED_BIT, Ordering::Release);
                    if self.state.0.unpark_one_writer() {
                        return;
                    }
                    cur &= !WPARKED_BIT
                }
                match self.state.0.value().compare_exchange_weak(
                    cur,
                    cur & !LOCKED_BIT,
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return,
                    Err(x) => {
                        cur = x;
                        core::hint::spin_loop();
                    }
                }
            }
        }
    }

    impl<'a> Drop for Lock<'a> {
        #[inline(always)]
        fn drop(&mut self) {
            let p = self.phase();
            let xor = (p ^ self.on_unlock).bits() | LOCKED_BIT;
            let prev = self.state.0.value().fetch_xor(xor, Ordering::Release);
            if prev & (PARKED_BIT | WPARKED_BIT) != 0 {
                self.drop_slow(prev ^ xor);
            }
        }
    }

    impl<'a> Into<ReadLock<'a>> for Lock<'a> {
        #[inline(always)]
        fn into(self) -> ReadLock<'a> {
            let cur = self.state.0.value().load(Ordering::Relaxed);
            self.into_read_lock(cur)
        }
    }
    impl<'a> ReadLock<'a> {
        #[inline(never)]
        #[cold]
        fn drop_slow(&mut self, mut cur: u32) {
            if let Err(_) = self.state.0.value().compare_exchange(
                cur,
                cur | LOCKED_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                return;
            }
            cur |= LOCKED_BIT;
            loop {
                if cur & WPARKED_BIT != 0 {
                    self.state
                        .0
                        .value()
                        .fetch_and(!WPARKED_BIT, Ordering::Release);
                    if self.state.0.unpark_one_writer() {
                        return;
                    };
                    cur &= !WPARKED_BIT;
                }

                if cur & PARKED_BIT != 0 {
                    self.state
                        .0
                        .value()
                        .fetch_and(!PARKED_BIT, Ordering::Release);
                    let v = self.state.0.unpark_readers();
                    self.state
                        .0
                        .value()
                        .fetch_sub(LOCKED_BIT - READER_UNITY * (v + 1), Ordering::Release);
                    drop(ReadLock {
                        state:      self.state,
                        init_phase: Phase::from_bits_truncate(cur),
                    });
                    return;
                }
                // ici on possède un lock unique

                match self.state.0.value().compare_exchange_weak(
                    cur,
                    cur & !(LOCKED_BIT),
                    Ordering::Release,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return,
                    Err(x) => {
                        cur = x;
                        core::hint::spin_loop();
                    }
                }
            }
        }
    }

    impl<'a> Drop for ReadLock<'a> {
        #[inline(always)]
        fn drop(&mut self) {
            //let mut cur = self.state.0.value().load(Ordering::Relaxed);
            //let mut target;
            let prev = self
                .state
                .0
                .value()
                .fetch_sub(READER_UNITY, Ordering::Release);

            if prev & (READER_BITS | LOCKED_BIT) == READER_UNITY {
                let cur = prev - READER_UNITY;
                self.drop_slow(cur);
            }
        }
    }

    impl SyncPhasedLocker {
        #[inline(always)]
        pub const fn new(p: Phase) -> Self {
            SyncPhasedLocker(Parker::new(p.bits()))
        }
        #[inline(always)]
        /// Return the current phase and synchronize with the end of the
        /// phase transition that leads to this phase.
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.value().load(Ordering::Acquire))
        }
        #[inline(always)]
        /// lock the phase.
        ///
        /// If the returned value is a LockResult::Read, then other threads
        /// may also hold a such a lock. This lock call synchronize with the
        /// phase transition that leads to the current phase and the phase will
        /// not change while this lock is held
        ///
        /// If the returned value is a LockResult::Write, then only this thread
        /// hold the lock and the phase can be atomically transitionned using the
        /// returned lock.
        ///
        /// If the returned value is LockResult::None, then the call to lock synchronize
        /// whit the end of the phase transition that led to the current phase.
        pub fn lock<'a, T: ?Sized>(
            &'a self,
            v: &'a T,
            how: impl Fn(Phase) -> LockNature,
            hint: Phase,
            #[cfg(debug_mode)] id: &AtomicUsize,
        ) -> LockResult<SyncReadPhaseGuard<'_, T>, SyncPhaseGuard<'_, T>> {
            match self.raw_lock(
                how,
                hint,
                #[cfg(debug_mode)]
                id,
            ) {
                LockResult::Write(l) => LockResult::Write(SyncPhaseGuard::new(v, l)),
                LockResult::Read(l) => LockResult::Read(SyncReadPhaseGuard::new(v, l)),
                LockResult::None => LockResult::None,
            }
        }
        #[inline(always)]
        fn raw_lock(
            &self,
            how: impl Fn(Phase) -> LockNature,
            hint: Phase,
            #[cfg(debug_mode)] id: &AtomicUsize,
        ) -> LockResult<ReadLock<'_>, Lock<'_>> {
            let expect = hint.bits();
            match how(hint) {
                LockNature::None => {
                    let real = self.0.value().load(Ordering::Acquire);
                    if Phase::from_bits_truncate(real) == hint {
                        return LockResult::None;
                    }
                }
                LockNature::Write => {
                    if self
                        .0
                        .value()
                        .compare_exchange(
                            expect,
                            expect | LOCKED_BIT,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        return LockResult::Write(Lock {
                            state:     self,
                            on_unlock: Phase::from_bits_truncate(expect),
                        });
                    }
                }
                LockNature::Read => {
                    if self
                        .0
                        .value()
                        .compare_exchange(
                            expect,
                            expect + READER_UNITY,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                    {
                        return LockResult::Read(ReadLock {
                            state:      self,
                            init_phase: Phase::from_bits_truncate(expect),
                        });
                    }
                }
            }
            self.raw_lock_slow(
                how,
                #[cfg(debug_mode)]
                id,
            )
        }
        #[cold]
        #[inline(never)]
        fn raw_lock_slow(
            &self,
            how: impl Fn(Phase) -> LockNature,
            #[cfg(debug_mode)] id: &AtomicUsize,
        ) -> LockResult<ReadLock<'_>, Lock<'_>> {
            let mut spin_wait = SpinWait::new();

            let mut cur = self.0.value().load(Ordering::Relaxed);

            loop {
                match how(Phase::from_bits_truncate(cur)) {
                    LockNature::None => {
                        fence(Ordering::Acquire);
                        return LockResult::None;
                    }
                    LockNature::Write => {
                        if cur & (LOCKED_BIT | READER_BITS | PARKED_BIT | WPARKED_BIT) == 0 {
                            match self.0.value().compare_exchange_weak(
                                cur,
                                cur | LOCKED_BIT,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => {
                                    return LockResult::Write(Lock {
                                        state:     self,
                                        on_unlock: Phase::from_bits_truncate(cur),
                                    })
                                }
                                Err(x) => cur = x,
                            }
                            continue;
                        }
                        if cur & (PARKED_BIT | WPARKED_BIT) == 0
                            && cur & READER_BITS < (READER_UNITY << 4)
                            && spin_wait.spin()
                        {
                            cur = self.0.value().load(Ordering::Relaxed);
                            continue;
                        }
                        if cur & WPARKED_BIT == 0 {
                            match self.0.value().compare_exchange_weak(
                                cur,
                                cur | WPARKED_BIT,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Err(x) => {
                                    cur = x;
                                    continue;
                                }
                                Ok(_) => cur |= WPARKED_BIT,
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

                        if self.0.park_writer(cur) {
                            cur = self.0.value().fetch_or(WPARKED_BIT, Ordering::Relaxed);
                            let lock = Lock {
                                state:     self,
                                on_unlock: Phase::from_bits_truncate(cur),
                            };
                            match how(Phase::from_bits_truncate(cur)) {
                                LockNature::Write => return LockResult::Write(lock),
                                LockNature::Read => {
                                    return LockResult::Read(lock.into_read_lock(cur))
                                }
                                LockNature::None => return LockResult::None,
                            }
                        }
                    }
                    LockNature::Read => {
                        if (cur & (LOCKED_BIT | PARKED_BIT | WPARKED_BIT) == 0
                            || cur & READER_BITS > 0)
                            && ((cur & READER_BITS) != READER_BITS)
                        {
                            match self.0.value().compare_exchange_weak(
                                cur,
                                cur + READER_UNITY,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => {
                                    return LockResult::Read(ReadLock {
                                        state:      self,
                                        init_phase: Phase::from_bits_truncate(cur),
                                    })
                                }
                                Err(x) => cur = x,
                            }
                            continue;
                        }
                        if cur & (PARKED_BIT | WPARKED_BIT) == 0
                            && cur & READER_BITS < (READER_UNITY << 4)
                            && spin_wait.spin()
                        {
                            cur = self.0.value().load(Ordering::Relaxed);
                            continue;
                        }
                        if cur & PARKED_BIT == 0 {
                            match self.0.value().compare_exchange_weak(
                                cur,
                                cur | PARKED_BIT,
                                Ordering::Relaxed,
                                Ordering::Relaxed,
                            ) {
                                Err(x) => {
                                    cur = x;
                                    continue;
                                }
                                Ok(_) => cur |= PARKED_BIT,
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

                        if self.0.park_reader(cur) {
                            let lock = ReadLock {
                                state:      self,
                                init_phase: Phase::from_bits_truncate(cur),
                            };
                            match how(Phase::from_bits_truncate(cur)) {
                                LockNature::Read => return LockResult::Read(lock),
                                LockNature::None => return LockResult::None,
                                LockNature::Write => {
                                    spin_wait.reset();
                                    continue;
                                } //drop the lock and try again;
                            }
                        }
                    }
                }
                cur = self.0.value().load(Ordering::Relaxed);
                spin_wait.reset();
            }
        }
    }
}
pub(crate) use mutex::{Mutex, SyncPhasedLocker};
pub use mutex::{SyncPhaseGuard, SyncReadPhaseGuard};

mod local_mutex {
    use super::{LockNature, LockResult, PhaseGuard};
    use crate::phase::*;
    use crate::Phase;
    use core::cell::Cell;
    use core::mem::forget;
    use core::ops::Deref;

    /// A kind of RefCell that is also phase locker.
    pub struct UnSyncPhaseLocker(Cell<u32>);

    /// Equivalent to std::cell::Ref.
    pub struct UnSyncPhaseGuard<'a, T: ?Sized>(&'a T, &'a Cell<u32>, Phase);

    /// Equivalent to std::cell::RefMut that implements PhaseLocker.
    pub struct UnSyncReadPhaseGuard<'a, T: ?Sized>(&'a T, &'a Cell<u32>);

    impl<'a, T> Deref for UnSyncPhaseGuard<'a, T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a, T: ?Sized> UnSyncPhaseGuard<'a, T> {
        #[inline(always)]
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r, p, Phase::from_bits_truncate(p.get()))
        }
    }

    unsafe impl<'a, T: ?Sized> PhaseGuard<'a, T> for UnSyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn set_phase(&mut self, p: Phase) {
            self.2 = p;
        }
        #[inline(always)]
        fn commit_phase(&mut self) {
            self.1.set(self.2.bits() | LOCKED_BIT);
        }
        #[inline(always)]
        fn phase(&self) -> Phase {
            self.2
        }
        #[inline(always)]
        fn transition<R>(
            &mut self,
            f: impl FnOnce(&'a T) -> R,
            on_success: Phase,
            on_panic: Phase,
        ) -> R {
            self.2 = on_panic;
            let res = f(self.0);
            self.2 = on_success;
            res
        }
    }
    impl<'a, T: ?Sized> Into<UnSyncReadPhaseGuard<'a, T>> for UnSyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn into(self) -> UnSyncReadPhaseGuard<'a, T> {
            self.1.set(self.2.bits() | READER_UNITY);
            let r = UnSyncReadPhaseGuard(self.0, self.1);
            forget(self);
            r
        }
    }

    impl<'a, T: ?Sized> Drop for UnSyncPhaseGuard<'a, T> {
        #[inline(always)]
        fn drop(&mut self) {
            self.1.set(self.2.bits());
        }
    }

    impl<'a, T> Deref for UnSyncReadPhaseGuard<'a, T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a, T: ?Sized> UnSyncReadPhaseGuard<'a, T> {
        #[inline(always)]
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r, p)
        }
    }

    impl<'a, T: ?Sized> Drop for UnSyncReadPhaseGuard<'a, T> {
        #[inline(always)]
        fn drop(&mut self) {
            let count = self.1.get() & READER_BITS;
            debug_assert!(count >= READER_UNITY);
            self.1.set(self.1.get() - READER_UNITY);
        }
    }

    impl UnSyncPhaseLocker {
        #[inline(always)]
        pub const fn new(p: Phase) -> Self {
            Self(Cell::new(p.bits()))
        }
        #[inline(always)]
        /// Return the current (phase)[crate::Phase].
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.get())
        }
        #[inline(always)]
        /// Return a lock whose nature depends on 'lock_nature'
        ///
        /// # Panic
        ///
        /// Panic if an attempt to get a read or write lock is made
        /// while a write_lock is already held or if an attempt is made
        /// to get a write_lock if any read or write lock is held.
        pub fn lock<'a, T: ?Sized>(
            &'a self,
            v: &'a T,
            lock_nature: impl Fn(Phase) -> LockNature,
        ) -> LockResult<UnSyncReadPhaseGuard<'_, T>, UnSyncPhaseGuard<'_, T>> {
            match lock_nature(self.phase()) {
                LockNature::Write => {
                    assert_eq!(
                        self.0.get() & (LOCKED_BIT | READER_BITS),
                        0,
                        "Cannot get a mutable reference if it is already mutably borrowed"
                    );
                    self.0.set(self.0.get() | LOCKED_BIT);
                    LockResult::Write(UnSyncPhaseGuard::new(v, &self.0))
                }
                LockNature::Read => {
                    assert_eq!(
                        self.0.get() & LOCKED_BIT,
                        0,
                        "Cannot get a shared reference if it is alread mutably borrowed"
                    );
                    assert_ne!(
                        self.0.get() & (READER_BITS),
                        READER_BITS,
                        "Maximal number of shared borrow reached."
                    );
                    self.0.set(self.0.get() + READER_UNITY);
                    LockResult::Read(UnSyncReadPhaseGuard::new(v, &self.0))
                }
                LockNature::None => LockResult::None,
            }
        }
    }
}
pub use local_mutex::UnSyncPhaseLocker;
pub use local_mutex::{UnSyncPhaseGuard, UnSyncReadPhaseGuard};
