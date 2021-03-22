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

pub enum LockNature {
    Read,
    Write,
    None
}
pub enum LockResult<R,W> {
    Read(R),
    Write(W),
    None
}

mod mutex {
    use super::{PhaseGuard,LockNature,LockResult};
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
        on_unlock: Phase,
    }
    pub(crate) struct ReadLock<'a> {
        state:         &'a AtomicU32,
        init_phase: Phase,
    }
    pub(crate) struct MutexGuard<'a,T>(&'a mut T,Lock<'a>);

    pub struct SyncPhaseGuard<'a,T:?Sized>(&'a T,Lock<'a>);

    impl<'a,T> Deref for SyncPhaseGuard<'a,T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> SyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn new(r: &'a T, lock: Lock<'a>) -> Self {
            Self(r,lock)
        }
    }
    impl<'a,T:?Sized> PhaseGuard<T> for SyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn set_phase(&mut self,p: Phase) {
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
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> SyncReadPhaseGuard<'a,T> {
        #[inline(always)]
        fn new(r: &'a T, lock: ReadLock<'a>) -> Self {
            Self(r,lock)
        }
        #[inline(always)]
        pub fn phase(this: &Self) -> Phase {
            this.1.init_phase
        }
    }
    impl<'a,T> Into<SyncReadPhaseGuard<'a,T>> for SyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn into(self) -> SyncReadPhaseGuard<'a,T> {
            SyncReadPhaseGuard(self.0,self.1.into())
        }
    }

    impl<T> Mutex<T> {
        #[inline(always)]
        pub(crate) const fn new(value: T) -> Self {
            Self(UnsafeCell::new(value), SyncPhasedLocker::new(Phase::empty()))
        }
        #[inline(always)]
        pub(crate) fn lock(&self) -> MutexGuard<'_,T> {
            let lk = if let LockResult::Write(l) = self.1.raw_lock(|_p| {LockNature::Write},Phase::empty()) {
                l
            } else {
                unreachable!()
            };
            MutexGuard(unsafe{&mut *self.0.get()},lk)
        }
    }

    impl<'a,T> Deref for MutexGuard<'a,T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }
    impl<'a,T> DerefMut for MutexGuard<'a,T> {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            self.0
        }
    }

    impl<'a> Lock<'a> {
        #[inline(always)]
        pub fn phase(&self) -> Phase {
            let v = self.state.load(Ordering::Relaxed);
            Phase::from_bits_truncate(v)
        }
        #[inline(always)]
        pub fn xor_phase(&self, xor: Phase) -> Phase {
            let v = self.state.fetch_xor(xor.bits(), Ordering::Release);
            Phase::from_bits_truncate(v) ^ xor
        }
    }

    impl<'a> Drop for Lock<'a> {
        #[inline(always)]
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
        #[inline(always)]
       fn into(self) -> ReadLock<'a> { 
           let p = self.phase();
           let xorp = p ^ self.on_unlock;
           let xor_state = xorp.bits() | LOCKED_BIT | READER_UNITY;
           let x = self.state.fetch_xor(xor_state,Ordering::AcqRel);
           debug_assert_ne!(x & LOCKED_BIT, 0);
           debug_assert_eq!(x & READER_BITS, 0);
           let r = ReadLock{state:self.state, init_phase:self.on_unlock};
           forget(self);
           r
       }
    }

    impl<'a> Drop for ReadLock<'a> {
        #[inline(always)]
        fn drop(&mut self) {
            let mut cur = self.state.load(Ordering::Relaxed);
            let mut target;
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
        #[inline(always)]
        pub const fn new(p: Phase) -> Self {
            SyncPhasedLocker(AtomicU32::new(p.bits()))
        }
        #[inline(always)]
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.load(Ordering::Acquire))
        }
        #[inline(always)]
        pub fn lock<'a,T:?Sized>(
            &'a self,
            v: &'a T,
            how: impl Fn(Phase) -> LockNature,
            hint: Phase,
            #[cfg(debug_mode)] id: &AtomicUsize,
        )  -> LockResult<SyncReadPhaseGuard<'_,T>,SyncPhaseGuard<'_,T>> {
            match self.raw_lock(how,hint,#[cfg(debug_mode)] id) {
                LockResult::Write(l) => LockResult::Write(SyncPhaseGuard::new(v,l)),
                LockResult::Read(l) => LockResult::Read(SyncReadPhaseGuard::new(v,l)),
                LockResult::None => LockResult::None
            }
        }
        #[inline(always)]
        fn raw_lock(
            &self,
            how: impl Fn(Phase) -> LockNature,
            hint: Phase,
            #[cfg(debug_mode)] id: &AtomicUsize,
        )  -> LockResult<ReadLock<'_>,Lock<'_>> {
            let cur = hint.bits();
            match how(hint){
                LockNature::None => { 
                          let real = self.0.load(Ordering::Acquire);
                          if Phase::from_bits_truncate(real) == hint {
                            return LockResult::None;
                          }
                    }
                LockNature::Write => {
                    let expect = cur &!(LOCKED_BIT|PARKED_BIT|READER_BITS);
                    if self.0.compare_exchange(
                        expect,
                        expect | LOCKED_BIT,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ).is_ok() {
                        return LockResult::Write(Lock{state: &self.0, on_unlock: Phase::from_bits_truncate(expect)});
                    }
                }
                LockNature::Read => {
                    let expect = if cur & READER_BITS != READER_BITS {
                        cur &!(LOCKED_BIT|PARKED_BIT)
                    } else {
                        cur &!(LOCKED_BIT|PARKED_BIT)-READER_UNITY
                    };
                    if self.0.compare_exchange(
                          expect,
                          expect + READER_UNITY,
                          Ordering::Acquire,
                          Ordering::Relaxed,
                      ).is_ok() {
                          return LockResult::Read(ReadLock{state:&self.0, init_phase:Phase::from_bits_truncate(expect)});
                      }
                    
                }
            }
            self.raw_lock_slow(how)
        }
        fn raw_lock_slow(
            &self,
            how: impl Fn(Phase) -> LockNature,
            #[cfg(debug_mode)] id: &AtomicUsize,
        )  -> LockResult<ReadLock<'_>,Lock<'_>> {

            let mut spin_wait = SpinWait::new();

            let mut cur = self.0.load(Ordering::Relaxed);

            loop {
                match how(Phase::from_bits_truncate(cur)){
                    LockNature::None => { fence(Ordering::Acquire);
                              return LockResult::None;
                        }
                    LockNature::Write => {
                        if cur & (LOCKED_BIT|READER_BITS) == 0 {
                            match self.0.compare_exchange_weak(
                                cur,
                                cur | LOCKED_BIT,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => return LockResult::Write(Lock{state: &self.0, on_unlock: Phase::from_bits_truncate(cur)}),
                                Err(x) => cur = x,
                            }
                            continue;
                        }
                        if cur & PARKED_BIT == 0 && (cur&READER_BITS) < (READER_UNITY<<4) && spin_wait.spin() {
                            cur = self.0.load(Ordering::Relaxed);
                            continue;
                        }
                    }
                    LockNature::Read => {
                        if cur & (LOCKED_BIT|PARKED_BIT) == 0 && ((cur & READER_BITS) != READER_BITS){
                            match self.0.compare_exchange_weak(
                                cur,
                                cur + READER_UNITY,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => return LockResult::Read(ReadLock{state:&self.0,init_phase: Phase::from_bits_truncate(cur)}),
                                Err(x) => cur = x,
                            }
                            continue;
                        }
                        if cur & PARKED_BIT == 0 && spin_wait.spin() {
                            cur = self.0.load(Ordering::Relaxed);
                            continue;
                        }
                    }
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
        }
    }
}
pub(crate) use mutex::{SyncPhasedLocker,Mutex};
pub use mutex::{SyncPhaseGuard,SyncReadPhaseGuard};

mod local_mutex {
    use super::{PhaseGuard,LockResult,LockNature};
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
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> UnSyncPhaseGuard<'a,T> {
        #[inline(always)]
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r,p, Phase::from_bits_truncate(p.get()))
        }
    }

    impl<'a,T:?Sized> PhaseGuard<T> for UnSyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn set_phase(&mut self,p: Phase) {
            self.2 = p;
        }
        #[inline(always)]
        fn commit_phase(&mut self) {
            self.1.set(self.2.bits()|LOCKED_BIT);
        }
        #[inline(always)]
        fn phase(&self) -> Phase { 
            self.2
        }
        #[inline(always)]
        fn transition<R>(&mut self,f: impl FnOnce(&T)->R, on_success:Phase, on_panic: Phase) -> R {
            self.2 = on_panic;
            let res = f(self.0);
            self.2 = on_success;
            res
        }
    }
    impl<'a, T:?Sized> Into<UnSyncReadPhaseGuard<'a,T>> for UnSyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn into(self) -> UnSyncReadPhaseGuard<'a,T> {
            self.1.set(self.2.bits()|READER_UNITY);
            let r = UnSyncReadPhaseGuard(self.0,self.1);
            forget(self);
            r
        }
    }

    impl<'a, T:?Sized> Drop for UnSyncPhaseGuard<'a,T> {
        #[inline(always)]
        fn drop(&mut self) {
            self.1.set(self.2.bits());
        }
    }

    impl<'a,T> Deref for UnSyncReadPhaseGuard<'a,T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            self.0
        }
    }

    impl<'a,T:?Sized> UnSyncReadPhaseGuard<'a,T> {
        #[inline(always)]
        pub(crate) fn new(r: &'a T, p: &'a Cell<u32>) -> Self {
            Self(r,p)
        }
    }

    impl<'a, T:?Sized> Drop for UnSyncReadPhaseGuard<'a,T> {
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
        pub fn phase(&self) -> Phase {
            Phase::from_bits_truncate(self.0.get())
        }
        #[inline(always)]
        pub fn lock<'a,T:?Sized>(&'a self,v: &'a T,shall_proceed: impl Fn(Phase)->LockNature) -> LockResult<UnSyncReadPhaseGuard<'_,T>,UnSyncPhaseGuard<'_,T>> {
            match shall_proceed(self.phase()) {
                LockNature::Write => {
                    assert_eq!(self.0.get() & (LOCKED_BIT|READER_BITS), 0, "Recursive lock detected");
                    self.0.set(self.0.get() | LOCKED_BIT);
                    LockResult::Write(UnSyncPhaseGuard::new(v, &self.0))
            } 
                LockNature::Read => {
                    assert_eq!(self.0.get() & LOCKED_BIT, 0, "Recursive lock detected");
                    assert_ne!(self.0.get() & (READER_BITS),READER_BITS,"Maximal number of shared reference exceeded");
                    self.0.set(self.0.get()  + READER_UNITY);
                    LockResult::Read(UnSyncReadPhaseGuard::new(v, &self.0))
                }
                LockNature::None => LockResult::None,
            }
        }
    }
}
pub(crate) use local_mutex::UnSyncPhaseLocker;
pub use local_mutex::{UnSyncPhaseGuard,UnSyncReadPhaseGuard};
