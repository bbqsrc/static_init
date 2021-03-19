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
     fn set_phase_committed(&mut self, p:Phase);
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

    pub(crate) struct SyncPhasedLocker(AtomicU32);

    pub(crate) struct Mutex<T>(UnsafeCell<T>,SyncPhasedLocker);

    unsafe impl<T:Send> Sync for Mutex<T> {}

    unsafe impl<T:Send> Send for Mutex<T> {}

    pub(crate) struct Lock<'a> {
        state:         &'a AtomicU32,
        pub(crate) on_unlock: Phase,
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
        fn set_phase_committed(&mut self, p:Phase) {
            //Butter fly trick
            let to_xor = self.1.on_unlock ^ p;
            self.1.xor_phase(to_xor);

            self.1.on_unlock = p;
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
                if cur & LOCKED_BIT == 0 {
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
            Some(Lock {
                state:     &self.0,
                on_unlock: Phase::from_bits_truncate(cur),
            })
        }
    }
}
pub(crate) use mutex::{SyncPhasedLocker,Mutex};
pub use mutex::{SyncPhaseGuard};

mod local_mutex {
    use super::PhaseGuard;
    use crate::Phase;
    use core::ops::Deref;
    use core::cell::Cell;

    pub(crate) struct UnSyncPhaseLocker(Cell<Phase>);

    pub struct UnSyncSyncPhaseGuard<'a,T:?Sized>(&'a T,&'a Cell<Phase>, Phase);

    impl<'a,T> Deref for UnSyncSyncPhaseGuard<'a,T> {
        type Target = T;
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> UnSyncSyncPhaseGuard<'a,T> {
        pub(crate) fn new(r: &'a T, p: &'a Cell<Phase>) -> Self {
            Self(r,p, p.get())
        }
    }
    impl<'a,T:?Sized> PhaseGuard<T> for UnSyncSyncPhaseGuard<'a,T> {
        fn set_phase(&mut self,p: Phase) {
            self.2 = p;
        }
        fn set_phase_committed(&mut self, p:Phase) {
            self.1.set(p);
            self.2 = p;
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

    impl<'a, T:?Sized> Drop for UnSyncSyncPhaseGuard<'a,T> {
        fn drop(&mut self) {
            self.1.set(self.2);
        }
    }

    impl UnSyncPhaseLocker {
        pub const fn new(p: Phase) -> Self {
            Self(Cell::new(p))
        }
        pub fn phase(&self) -> Phase {
            self.0.get()
        }
        pub fn lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->bool) -> Option<UnSyncSyncPhaseGuard<'_,T>> {
            if shall_proceed(self.phase()) {
                Some(UnSyncSyncPhaseGuard::new(v, &self.0))
            } else {
                None
            }
        }
    }
}
pub(crate) use local_mutex::UnSyncPhaseLocker;
pub use local_mutex::{UnSyncSyncPhaseGuard};
