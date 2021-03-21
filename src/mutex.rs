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
    use core::sync::atomic::AtomicU32;
    use libc::{syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT, FUTEX_WAKE};

    pub(super) fn park(at: &AtomicU32, value: u32) -> bool {
        unsafe {
            syscall(
                SYS_futex,
                at as *const _ as *const _,
                FUTEX_WAIT | FUTEX_PRIVATE_FLAG,
                value,
                ptr::null::<u32>(),
            ) == 0
        }
    }
    pub(super) fn unpark_all(at: &AtomicU32) {
        unsafe {
            syscall(
                SYS_futex,
                at as *const _ as *const _,
                FUTEX_WAKE | FUTEX_PRIVATE_FLAG,
                u32::MAX,
            )
        };
    }
}
#[cfg(any(target_os = "linux", target_os = "android"))]
use linux::{park, unpark_all};

#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
mod other {
    use parking_lot_core::{DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN};

    pub(super) fn park(at: &AtomicU32, value: u32) -> bool {
        parking_lot_core::park(
            at as *const _ as usize,
            || at.load(Ordering::Relaxed) == value,
            || {},
            |_, _| {},
            parking_lot_core::DEFAULT_PARK_TOKEN,
            None,
        );
    }
    pub(super) fn unpark(at: &AtomicU32, value: u32) -> bool {
        parking_lot_core::unpark_all(
            this as *const _ as usize,
            parking_lot_core::DEFAULT_UNPARK_TOKEN,
        );
    }
}
#[cfg(not(any(target_os = "linux", target_os = "android", target_os = "windows")))]
pub(super) use other::{park, unpark_all};

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

pub trait PhaseGuard<T:?Sized> {
     fn set_phase(&mut self,p: Phase);
     fn commit_phase(&mut self);
     fn phase(&self) -> Phase;
     fn transition<R>(&mut self,f: impl FnOnce(&T)->R, on_success:Phase, on_panic: Phase) -> R;
}

mod mutex {
    use super::PhaseGuard;
    use super::spin_wait::SpinWait;
    use super::{park, unpark_all};
    use crate::phase::*;
    use crate::Phase;
    use core::sync::atomic::{fence, AtomicU32, Ordering};
    use core::cell::UnsafeCell;
    use core::ops::{Deref,DerefMut};
    use core::hint;
    use core::mem::forget;

    /// peut être pas un bon choix pour un static donnant un accès dérière un lock;
    /// un rwlock serait aussi un bon choix puisque dans ce cas tous les readlock synchronize
    /// ensemble est avec write locks.
    pub(crate) struct SyncPhasedLocker(AtomicU32);

    pub(crate) struct Mutex<T>(UnsafeCell<T>,SyncPhasedLocker);

    unsafe impl<T:Send> Sync for Mutex<T> {}

    unsafe impl<T:Send> Send for Mutex<T> {}

    pub(crate) struct Lock<'a> {
        state:         &'a AtomicU32,
        pub(crate) on_unlock: Phase,
    }
    pub(crate) struct ReadLock<'a> {
        state:         &'a AtomicU32,
    }
    pub(crate) struct MutexGuard<'a,T>(&'a mut T,Lock<'a>);

    pub struct SyncPhaseGuard<'a,T:?Sized>(&'a T,Lock<'a>);

    impl<'a,T> Deref for SyncPhaseGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> SyncPhaseGuard<'a,T> {
        fn new(r: &'a T, lock: Lock<'a>) -> Self {
            Self(r,lock)
        }
    }
    impl<'a,T:?Sized> PhaseGuard<T> for SyncPhaseGuard<'a,T> {
        fn set_phase(&mut self,p: Phase) {
            self.1.on_unlock = p;
        }
        fn commit_phase(&mut self) {
            //Butter fly trick
            let cur = self.1.phase();
            let to_xor = self.1.on_unlock ^ cur;
            self.1.xor_phase(to_xor);
        }
        fn phase(&self) -> Phase { 
            self.1.on_unlock
        }
        fn transition<R>(&mut self,f: impl FnOnce(&T)->R, on_success:Phase, on_panic: Phase) -> R {
            self.1.on_unlock = on_panic;
            let res = f(self.0);
            self.1.on_unlock = on_success;
            res
        }
    }

    pub struct SyncReadPhaseGuard<'a,T:?Sized>(&'a T,ReadLock<'a>);

    impl<'a,T> Deref for SyncReadPhaseGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> SyncReadPhaseGuard<'a,T> {
        fn new(r: &'a T, lock: ReadLock<'a>) -> Self {
            Self(r,lock)
        }
    }
    impl<'a,T> Into<SyncReadPhaseGuard<'a,T>> for SyncPhaseGuard<'a,T> {
        fn into(self) -> SyncReadPhaseGuard<'a,T> {
            SyncReadPhaseGuard(self.0,self.1.into())
        }
    }

    impl<T> Mutex<T> {
        pub(crate) const fn new(value: T) -> Self {
            Self(UnsafeCell::new(value), SyncPhasedLocker::new(Phase::empty()))
        }
        pub(crate) fn lock(&self) -> MutexGuard<'_,T> {
            MutexGuard(unsafe{&mut *self.0.get()},self.1.raw_lock(|_p| {true}).unwrap())
        }
    }

    impl<'a,T> Deref for MutexGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }
    impl<'a,T> DerefMut for MutexGuard<'a,T> {
        fn deref_mut(&mut self) -> &mut T {
            self.0
        }
    }

    impl<'a> Lock<'a> {
        pub fn phase(&self) -> Phase {
            let v = self.state.load(Ordering::Relaxed);
            Phase::from_bits_truncate(v)
        }
        pub fn xor_phase(&self, xor: Phase) -> Phase {
            let v = self.state.fetch_xor(xor.bits(), Ordering::Release);
            Phase::from_bits_truncate(v) ^ xor
        }
    }

    impl<'a> Drop for Lock<'a> {
        fn drop(&mut self) {
            let prev = self.state.swap(
                self.on_unlock.bits(),
                Ordering::Release,
            );
            if prev & PARKED_BIT != 0 {
                unpark_all(self.state)
            }
        }
    }

    impl<'a> Into<ReadLock<'a>> for Lock<'a> {
       fn into(self) -> ReadLock<'a> { 
           let p = self.phase();
           let xorp = p ^ self.on_unlock;
           let xor_state = xorp.bits() | LOCKED_BIT | READER_UNITY;
           let x = self.state.fetch_xor(xor_state,Ordering::AcqRel);
           debug_assert_ne!(x & LOCKED_BIT, 0);
           debug_assert_eq!(x & READER_BITS, 0);
           let r = ReadLock{state:self.state};
           forget(self);
           r
       }
    }

    impl<'a> Drop for ReadLock<'a> {
        fn drop(&mut self) {
            let mut cur = self.state.load(Ordering::Relaxed);
            let mut target;
            assert!(cur & READER_BITS != 0, "XXXXXXXXXXXXXXXXXXXX");
            loop {
                if (cur & READER_BITS) == READER_UNITY {
                    target = cur & !(READER_BITS|PARKED_BIT)
                } else {
                    target = cur - READER_UNITY 
                }
                match self.state.compare_exchange_weak(cur, target, Ordering::Release,Ordering::Relaxed) {
                    Ok(_) => {
                        if (cur & PARKED_BIT != 0) && (target & PARKED_BIT == 0) {
                            unpark_all(self.state);
                        }
                        break;
                    }
                    Err(v) => {
                        cur = v;
                        hint::spin_loop();
                    }
                }
            }
        }
    }

    impl SyncPhasedLocker {
        pub const fn new(p: Phase) -> Self {
            SyncPhasedLocker(AtomicU32::new(p.bits()))
        }
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.load(Ordering::Acquire))
        }
        pub fn lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->bool) -> Option<SyncPhaseGuard<'_,T>> {
            self.raw_lock(shall_proceed).map(|l| SyncPhaseGuard::new(v,l))
        }
        fn raw_lock(
            &self,
            shall_proceed: impl Fn(Phase) -> bool,
            #[cfg(debug_mode)] id: &AtomicUsize,
        ) -> Option<Lock> {

            let mut spin_wait = SpinWait::new();

            let mut cur = self.0.load(Ordering::Relaxed);

            loop {
                if !shall_proceed(Phase::from_bits_truncate(cur)) {
                    fence(Ordering::Acquire);
                    return None;
                }
                if cur & (LOCKED_BIT|READER_BITS) == 0 {
                    match self.0.compare_exchange_weak(
                        cur,
                        cur | LOCKED_BIT,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(x) => cur = x,
                    }
                    continue;
                }
                if cur & PARKED_BIT == 0 && (cur&READER_BITS) < (READER_UNITY<<4) && spin_wait.spin() {
                    cur = self.0.load(Ordering::Relaxed);
                    continue;
                }
                if cur & PARKED_BIT == 0 {
                    match self.0.compare_exchange_weak(
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

                park(&self.0, cur);
                spin_wait.reset();
                cur = self.0.load(Ordering::Relaxed);
            }
            Some(Lock {
                state:     &self.0,
                on_unlock: Phase::from_bits_truncate(cur),
            })
        }
        pub fn read_lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->bool) -> Option<SyncReadPhaseGuard<'_,T>> {
            self.raw_read_lock(shall_proceed).map(|l| SyncReadPhaseGuard::new(v,l))
        }
        fn raw_read_lock(
            &self,
            shall_proceed: impl Fn(Phase) -> bool,
            #[cfg(debug_mode)] id: &AtomicUsize,
        ) -> Option<ReadLock> {

            let mut spin_wait = SpinWait::new();

            let mut cur = self.0.load(Ordering::Relaxed);

            loop {
                if !shall_proceed(Phase::from_bits_truncate(cur)) {
                    fence(Ordering::Acquire);
                    return None;
                }
                if cur & (LOCKED_BIT|PARKED_BIT) == 0 && ((cur & READER_BITS) != READER_BITS){
                    match self.0.compare_exchange_weak(
                        cur,
                        cur + READER_UNITY,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => break,
                        Err(x) => cur = x,
                    }
                    continue;
                }
                if cur & PARKED_BIT == 0 && spin_wait.spin() {
                    cur = self.0.load(Ordering::Relaxed);
                    continue;
                }
                if cur & PARKED_BIT == 0 {
                    match self.0.compare_exchange_weak(
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

                park(&self.0, cur);
                spin_wait.reset();
                cur = self.0.load(Ordering::Relaxed);
            }
            Some(ReadLock {
                state:     &self.0,
            })
        }
    }
}
pub(crate) use mutex::{SyncPhasedLocker,Mutex};
pub use mutex::{SyncPhaseGuard,SyncReadPhaseGuard};

mod local_mutex {
    use super::PhaseGuard;
    use crate::Phase;
    use core::ops::Deref;
    use core::cell::Cell;
    use core::mem::forget;
    use crate::phase::*;

    pub(crate) struct UnSyncPhaseLocker(Cell<u32>);

    pub struct UnSyncPhaseGuard<'a,T:?Sized>(&'a T,&'a Cell<u32>, Phase);

    pub struct UnSyncReadPhaseGuard<'a,T:?Sized>(&'a T,&'a Cell<u32>);

    impl<'a,T> Deref for UnSyncPhaseGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> UnSyncPhaseGuard<'a,T> {
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r,p, Phase::from_bits_truncate(p.get()))
        }
    }

    impl<'a,T:?Sized> PhaseGuard<T> for UnSyncPhaseGuard<'a,T> {
        fn set_phase(&mut self,p: Phase) {
            self.2 = p;
        }
        fn commit_phase(&mut self) {
            self.1.set(self.2.bits()|LOCKED_BIT);
        }
        fn phase(&self) -> Phase { 
            self.2
        }
        fn transition<R>(&mut self,f: impl FnOnce(&T)->R, on_success:Phase, on_panic: Phase) -> R {
            self.2 = on_panic;
            let res = f(self.0);
            self.2 = on_success;
            res
        }
    }
    impl<'a, T:?Sized> Into<UnSyncReadPhaseGuard<'a,T>> for UnSyncPhaseGuard<'a,T> {
        fn into(self) -> UnSyncReadPhaseGuard<'a,T> {
            self.1.set(self.2.bits()|READER_UNITY);
            let r = UnSyncReadPhaseGuard(self.0,self.1);
            forget(self);
            r
        }
    }

    impl<'a, T:?Sized> Drop for UnSyncPhaseGuard<'a,T> {
        fn drop(&mut self) {
            self.1.set(self.2.bits());
        }
    }

    impl<'a,T> Deref for UnSyncReadPhaseGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> UnSyncReadPhaseGuard<'a,T> {
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r,p)
        }
    }

    impl<'a, T:?Sized> Drop for UnSyncReadPhaseGuard<'a,T> {
        fn drop(&mut self) {
            let count = self.1.get() & READER_BITS;
            debug_assert!(count >= READER_UNITY);
            self.1.set(self.1.get() - READER_UNITY);
        }
    }

    impl UnSyncPhaseLocker {
        pub const fn new(p: Phase) -> Self {
            Self(Cell::new(p.bits()))
        }
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.get())
        }
        pub fn lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->bool) -> Option<UnSyncPhaseGuard<'_,T>> {
            if shall_proceed(self.phase()) {
                assert_eq!(self.0.get() & (LOCKED_BIT|READER_BITS), 0, "Recursive lock detected");
                self.0.set(self.0.get() | LOCKED_BIT);
                Some(UnSyncPhaseGuard::new(v, &self.0))
            } else {
                None
            }
        }
        pub fn read_lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->bool) -> Option<UnSyncReadPhaseGuard<'_,T>> {
            if shall_proceed(self.phase()) {
                assert_eq!(self.0.get() & LOCKED_BIT, 0, "Recursive lock detected");
                assert_ne!(self.0.get() & (READER_BITS),READER_BITS,"Maximal number of shared reference exceeded");
                self.0.set(self.0.get()  + READER_UNITY);
                Some(UnSyncReadPhaseGuard::new(v, &self.0))
            } else {
                None
            }
        }
    }
}
pub(crate) use local_mutex::UnSyncPhaseLocker;
pub use local_mutex::{UnSyncPhaseGuard,UnSyncReadPhaseGuard};
