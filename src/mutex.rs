#[cfg(windows)]
mod windows {
    use core::mem::size_of;
    use winapi::shared::minwindef::TRUE;
    use winapi::um::synchapi::{WaitOnAdress, WakeByAddressAll};
    use winapi::um::winbase::INFINITE;
    pub(super) fn park(at: &AtomicU32, value: u32) -> bool {
        WaitOnAddress(
            at as *const _ as *const _,
            (&value) as *const _ as *const _,
            size_of::<u32>(),
            INFINITE,
        ) == TRUE
    }
    pub(super) fn unpark_all(at: &AtomicUsize) {
        WakeByAddressAll(at as *const _ as *const _)
    }
}
#[cfg(windows)]
use windows::{park, unpark_all};

#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux {
    use core::ptr;
    use core::sync::atomic::{AtomicU32};
    use libc::{syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT, FUTEX_WAKE, FUTEX_WAKE_BITSET, FUTEX_WAIT_BITSET};

    pub(super) struct Parker {
        futex:        AtomicU32,
    }

    impl Parker {
        pub(super) const fn new(value: u32) -> Self {
            Self {
                futex:        AtomicU32::new(value),
            }
        }

        pub(super) fn park(&self, value: u32) {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                )
            };
        }
        pub(super) fn unpark_all(&self) {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                    i32::MAX
                )
            };
        }
        pub(super) fn unpark_all_readers(&self) {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    i32::MAX,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    2
                ) as u32 
            }
        }
        pub(super) fn unpark_one_writer(&self) {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    1, 
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    1
                )
            };
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
                    1
                ) == 0
            }
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
                    2
                ) == 0
            }
        }
        pub(super) fn value(&self) -> &AtomicU32 {
            &self.futex
        }
    }
}
#[cfg(any(target_os = "linux", target_os = "android"))]
use linux::Parker;

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
mod other {
    use core::sync::atomic::{AtomicU32, Ordering};
    use parking_lot_core::{DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN};

    pub(super) struct Parker(AtomicU32);

    impl Parker {
        pub(super) const fn new(value: u32) -> Self {
            Self(AtomicU32::new(value))
        }

        pub(super) fn park(&self, value: u32) {
            unsafe {
                parking_lot_core::park(
                    &self.0 as *const _ as usize,
                    || self.0.load(Ordering::Relaxed) == value,
                    || {},
                    |_, _| {},
                    DEFAULT_PARK_TOKEN,
                    None,
                )
            };
        }
        pub(super) fn unpark_all(&self) {
            unsafe {
                parking_lot_core::unpark_all(&self.0 as *const _ as usize, DEFAULT_UNPARK_TOKEN)
            };
        }
        pub(super) fn value(&self) -> &AtomicU32 {
            &self.0
        }
    }
}
#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
use other::Parker;

mod spin_wait {
    // Extracted from parking_lot_core
    //
    // without thread yield... for no_std.
    //
    use core::hint;

    // Wastes some CPU time for the given number of iterations,
    // using a hint to indicate to the CPU that we are spinning.
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
        #[inline]
        pub fn new() -> Self {
            Self::default()
        }

        #[inline]
        pub fn reset(&mut self) {
            self.counter = 0;
        }

        #[inline]
        pub fn spin(&mut self) -> bool {
            if self.counter >= 4 {
                return false;
            }
            self.counter += 1;
            cpu_relax(1 << self.counter);
            true
        }
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
    use core::hint;
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

    impl<'a> Drop for Lock<'a> {
        #[inline(always)]
        fn drop(&mut self) {
            let prev = self
                .state
                .0
                .value()
                .swap(self.on_unlock.bits(), Ordering::Release);
            if prev & PARKED_BIT != 0 {
                self.state.0.unpark_all();
            }
        }
    }

    impl<'a> Into<ReadLock<'a>> for Lock<'a> {
        #[inline(always)]
        fn into(self) -> ReadLock<'a> {
            let p = self.phase();
            let xorp = p ^ self.on_unlock;
            let xor_state = xorp.bits() | LOCKED_BIT | READER_UNITY;
            let x = self.state.0.value().fetch_xor(xor_state, Ordering::AcqRel);
            debug_assert_ne!(x & LOCKED_BIT, 0);
            debug_assert_eq!(x & READER_BITS, 0);
            let r = ReadLock {
                state:      self.state,
                init_phase: self.on_unlock,
            };
            forget(self);
            r
        }
    }

    impl<'a> Drop for ReadLock<'a> {
        #[inline(always)]
        fn drop(&mut self) {
            //let mut cur = self.state.0.value().load(Ordering::Relaxed);
            //let mut target;
            let prev = self.state.0.value().fetch_sub(READER_UNITY,Ordering::Release);
            if prev & READER_BITS == READER_UNITY && prev & PARKED_BIT != 0 {
                self.state.0.value().fetch_and(!PARKED_BIT,Ordering::Relaxed);
                self.state.0.unpark_all();
            }

            //}
            //loop {
            //    if (cur & READER_BITS) == READER_UNITY {
            //        target = cur & !(READER_BITS | PARKED_BIT)
            //    } else {
            //        target = cur - READER_UNITY
            //    }
            //    match self.state.0.value().compare_exchange_weak(
            //        cur,
            //        target,
            //        Ordering::Release,
            //        Ordering::Relaxed,
            //    ) {
            //        Ok(_) => {
            //            if (cur & PARKED_BIT != 0) && (target & PARKED_BIT == 0) {
            //                self.state.0.unpark_all();
            //            } 
            //            break;
            //        }
            //        Err(v) => {
            //            cur = v;
            //            hint::spin_loop();
            //        }
            //    }
            //}
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
                        if cur & (LOCKED_BIT | PARKED_BIT | READER_BITS) == 0 {
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
                        if cur & PARKED_BIT == 0
                            && (cur & READER_BITS) < (READER_UNITY << 4)
                            && spin_wait.spin()
                        {
                            cur = self.0.value().load(Ordering::Relaxed);
                            continue;
                        }
                    }
                    LockNature::Read => {
                        if cur & (LOCKED_BIT | PARKED_BIT) == 0
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
                        if cur & PARKED_BIT == 0 && spin_wait.spin() {
                            cur = self.0.value().load(Ordering::Relaxed);
                            continue;
                        }
                    }
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

                self.0.park(cur);

                spin_wait.reset();
                cur = self.0.value().load(Ordering::Relaxed);
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
