use super::futex::Futex;
use super::spin_wait::SpinWait;
use super::{LockNature, LockResult, PhaseGuard};
use crate::phase::*;
use crate::{Phase, Phased};
use core::cell::UnsafeCell;
use core::mem::forget;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{fence, Ordering};

/// A synchronised phase locker.
pub struct SyncPhasedLocker(Futex);

pub(crate) struct Lock<'a> {
    futex:      &'a Futex,
    init_phase: Phase,
    on_unlock:  Phase,
}

/// A phase guard that allow atomic phase transition that
/// can be turned fastly into a [SyncReadPhaseGuard].
pub struct SyncPhaseGuard<'a, T: ?Sized>(&'a T, Lock<'a>);

pub(crate) struct ReadLock<'a> {
    futex:      &'a Futex,
    init_phase: Phase,
}

/// A kind of read lock.
pub struct SyncReadPhaseGuard<'a, T: ?Sized>(&'a T, ReadLock<'a>);

pub(crate) struct Mutex<T>(UnsafeCell<T>, SyncPhasedLocker);

pub(crate) struct MutexGuard<'a, T>(&'a mut T, Lock<'a>);

// SyncPhaseGuard
//-------------------
//
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

    #[inline(always)]
    pub fn map<S: ?Sized>(self, f: impl Fn(&'a T) -> &'a S) -> SyncPhaseGuard<'a, S> {
        SyncPhaseGuard(f(self.0), self.1)
    }
}
unsafe impl<'a, T: ?Sized> PhaseGuard<'a, T> for SyncPhaseGuard<'a, T> {
    #[inline(always)]
    unsafe fn set_phase(&mut self, p: Phase) {
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
    unsafe fn transition<R>(
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

impl<'a, T> Phased for SyncPhaseGuard<'a, T> {
    fn phase(this: &Self) -> Phase {
        this.1.on_unlock
    }
}

// SyncReadPhaseGuard
//-------------------
//
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
    pub fn phase(&self) -> Phase {
        self.1.init_phase
    }

    #[inline(always)]
    pub fn map<S: ?Sized>(self, f: impl Fn(&'a T) -> &'a S) -> SyncReadPhaseGuard<'a, S> {
        SyncReadPhaseGuard(f(self.0), self.1)
    }
}
impl<'a, T> From<SyncPhaseGuard<'a, T>> for SyncReadPhaseGuard<'a, T> {
    #[inline(always)]
    fn from(this: SyncPhaseGuard<'a, T>) -> SyncReadPhaseGuard<'a, T> {
        SyncReadPhaseGuard(this.0, this.1.into())
    }
}

impl<'a, T> Phased for SyncReadPhaseGuard<'a, T> {
    fn phase(this: &Self) -> Phase {
        this.1.init_phase
    }
}

// Mutex
//-------------------
//
unsafe impl<T: Send> Sync for Mutex<T> {}

unsafe impl<T: Send> Send for Mutex<T> {}

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
            self.1.raw_lock(
                |_p| LockNature::Write,
                |_p| LockNature::Write,
                Phase::empty(),
            )
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

// Lock
// ----

// STATES:
// LOCKED_BIT | <PARKED_BIT|WPARKED_BIT> => Write lock held
// any READER_BIT | <PARKED_BIT|WPARKED_BIT> => Read lock held
// LOCKED_BIT|any READER_BIT | <PARKED_BIT|WPARKED_BIT>
//       => wlock or rlock is being transfered to rlock
//       => rlock are taken right now
// any PARKED_BIT,WPARKED_BIT => a lock is being released
// during transfer to a write lock, the WPARKED_BIT is 0
// but if the transfer succeed, it means that there where one or
// more waiter for the write lock and WPARKED_BIT must be reset to 1
// if a waiter is awaken.

impl<'a> Lock<'a> {
    #[inline(always)]
    fn new(futex: &'a Futex, current: u32) -> Self {
        let p = Phase::from_bits_truncate(current);
        Self {
            futex,
            init_phase: p,
            on_unlock: p,
        }
    }
    #[inline(always)]
    pub fn phase(&self) -> Phase {
        let v = self.futex.load(Ordering::Relaxed);
        Phase::from_bits_truncate(v)
    }
    #[inline(always)]
    pub fn xor_phase(&self, xor: Phase) -> Phase {
        let v = self.futex.fetch_xor(xor.bits(), Ordering::Release);
        Phase::from_bits_truncate(v) ^ xor
    }
}

impl<'a> Lock<'a> {
    #[inline(always)]
    fn into_read_lock(self, cur: Phase) -> ReadLock<'a> {
        //state: old_phase | LOCKED_BIT | <0:PARKED_BIT|0:WPARKED_BIT>
        let xor = (cur ^ self.on_unlock).bits() | LOCKED_BIT | READER_UNITY;
        //state: phase | READER_UNITY | <0:PARKED_BIT|0:WPARKED_BIT>
        let prev = self.futex.fetch_xor(xor, Ordering::Release);

        let r = if prev & PARKED_BIT != 0 {
            wake_readers(&self.futex, 0, true)
        } else {
            ReadLock::new(self.futex, self.on_unlock.bits())
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
        //state: phase | 0:PARKED_BIT<|>0:WPARKED_BIT
        loop {
            assert_eq!(cur & (LOCKED_BIT | READER_BITS | READER_OVERF), 0);
            let mut un_activate_lock = 0;
            if cur & WPARKED_BIT != 0 {
                //state: phase | <PARKED_BIT> | WPARKED_BIT
                let prev = self
                    .futex
                    .fetch_xor(WPARKED_BIT | LOCKED_BIT, Ordering::Relaxed);
                assert_ne!(prev & WPARKED_BIT, 0);
                assert_eq!(prev & (LOCKED_BIT | READER_BITS | READER_OVERF), 0);
                if self.futex.wake_one_writer() {
                    return;
                };
                cur ^= WPARKED_BIT | LOCKED_BIT;
                un_activate_lock = LOCKED_BIT;
                //phase: phase | LOCKED_BIT | <PARKED_BIT>
            }

            if cur & PARKED_BIT != 0 {
                //phase: phase | <LOCKED_BIT> | PARKED_BIT
                wake_readers(&self.futex, un_activate_lock, false);
                return;
            }
            // ici on possède un lock unique, il s'est avéré qu'il n'y
            // avait plus de thread parké pour write, cela sera nécessairement
            // réglé ici ou à la seconde iteration avant de retourner ici.

            //cur: phase | LOCKED_BIT
            cur = self.futex.fetch_and(!LOCKED_BIT, Ordering::Relaxed);
            assert_ne!(cur & LOCKED_BIT, 0);
            if has_no_waiters(cur) {
                break;
            } //else new threads were parked
            cur &= !LOCKED_BIT; //unused
            core::hint::spin_loop();
        }
        //loop {
        //    if cur & PARKED_BIT != 0 {
        //        //state: phase | 0:PARKED_BIT | <0:WPARKED_BIT>
        //        wake_readers(&self.futex,0,false);
        //        return;
        //    }

        //    //state: phase | <1:PARKED_BIT> | WPARKED_BIT
        //    self.futex
        //        .fetch_xor(WPARKED_BIT | LOCKED_BIT, Ordering::Relaxed);
        //    //state: phase | LOCKED_BIT | <1:PARKED_BIT>
        //    if self.futex.wake_one_writer() {
        //        return;
        //    }

        //    //cur: phase | LOCKED_BIT
        //    cur = self
        //        .futex
        //        .fetch_and(!LOCKED_BIT, Ordering::Relaxed);
        //    if has_no_waiters(cur) {
        //        break;
        //    } //else new threads were parked
        //    cur &= !LOCKED_BIT;
        //    core::hint::spin_loop();
        //}
    }
}

impl<'a> Drop for Lock<'a> {
    #[inline(always)]
    fn drop(&mut self) {
        //state: old_phase | LOCKED_BIT | <0:PARKED_BIT|0:WPARKED_BIT>
        let p = self.init_phase;
        let p = match self.futex.compare_exchange(
            p.bits() | LOCKED_BIT,
            self.on_unlock.bits(),
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(x) => Phase::from_bits_truncate(x),
        };
        //let p = self.phase();
        let xor = (p ^ self.on_unlock).bits() | LOCKED_BIT;
        let prev = self.futex.fetch_xor(xor, Ordering::Release);
        //state: phase | <1:PARKED_BIT|1:WPARKED_BIT>
        if has_waiters(prev) {
            //state: phase | 1:PARKED_BIT<|>1:WPARKED_BIT
            self.drop_slow(prev ^ xor);
        }
    }
}

impl<'a> From<Lock<'a>> for ReadLock<'a> {
    #[inline(always)]
    fn from(this: Lock<'a>) -> ReadLock<'a> {
        let p = this.init_phase;
        this.into_read_lock(p)
    }
}

// ReadLock
// --------
impl<'a> ReadLock<'a> {
    #[inline(always)]
    fn new(futex: &'a Futex, current: u32) -> Self {
        let p = Phase::from_bits_truncate(current);
        Self {
            futex,
            init_phase: p,
        }
    }
    #[inline(never)]
    #[cold]
    fn drop_slow(&mut self, mut cur: u32) {
        //state: phase | PARKED_BIT <|> WPARKED_BIT
        loop {
            let mut un_activate_lock = 0;
            if cur & WPARKED_BIT != 0 {
                //state: phase | <PARKED_BIT> | WPARKED_BIT
                let prev = self
                    .futex
                    .fetch_xor(WPARKED_BIT | LOCKED_BIT, Ordering::Relaxed);
                assert_eq!(prev & LOCKED_BIT, 0);
                assert_ne!(prev & WPARKED_BIT, 0);
                assert_eq!(prev & (READER_BITS | READER_OVERF), 0);
                if self.futex.wake_one_writer() {
                    return;
                };
                cur ^= WPARKED_BIT;
                un_activate_lock = LOCKED_BIT;
                //phase: phase | LOCKED_BIT | <PARKED_BIT>
            }

            if cur & PARKED_BIT != 0 {
                //phase: phase | <LOCKED_BIT> | PARKED_BIT
                wake_readers(&self.futex, un_activate_lock, false);
                return;
            }
            // ici on possède un lock unique, il s'est avéré qu'il n'y
            // avait plus de thread parké pour write, cela sera nécessairement
            // réglé ici ou à la seconde iteration avant de retourner ici.

            //cur: phase | LOCKED_BIT
            cur = self.futex.fetch_and(!LOCKED_BIT, Ordering::Relaxed);
            assert_ne!(cur & LOCKED_BIT, 0);
            if has_no_waiters(cur) {
                break;
            } //else new threads were parked
            cur &= !LOCKED_BIT; //unused
            core::hint::spin_loop();
        }
    }
}

impl<'a> Drop for ReadLock<'a> {
    #[inline(always)]
    fn drop(&mut self) {
        //state: phase | <LOCKED_BIT> | READER_UNITY*n | <0:PARKED_BIT> |<0:WPARKED_BIT>
        let prev = self.futex.fetch_sub(READER_UNITY, Ordering::Release);
        //state: phase | <LOCKED_BIT> | READER_UNITY*(n-1) | <1:PARKED_BIT> |<1:WPARKED_BIT>
        if has_one_reader(prev) && is_not_write_locked(prev) && has_waiters(prev) {
            //state: phase | PARKED_BIT <|> WPARKED_BIT
            let cur = prev - READER_UNITY;
            self.drop_slow(cur);
        }
    }
}

const MAX_WAKED_READERS: usize = (READER_OVERF / READER_UNITY) as usize;

#[inline(always)]
fn has_no_readers(v: u32) -> bool {
    v & (READER_OVERF | READER_BITS) == 0
}

#[inline(always)]
fn has_readers(v: u32) -> bool {
    v & (READER_OVERF | READER_BITS) != 0
}

#[inline(always)]
fn has_one_reader(v: u32) -> bool {
    v & (READER_OVERF | READER_BITS) == READER_UNITY
}

#[inline(always)]
fn has_readers_max(v: u32) -> bool {
    v & (READER_OVERF | READER_BITS) == READER_OVERF | READER_BITS
}

#[inline(always)]
fn is_not_write_locked(v: u32) -> bool {
    v & LOCKED_BIT == 0
}
#[inline(always)]
fn is_write_locked(v: u32) -> bool {
    v & LOCKED_BIT != 0
}
//#[inline(always)]
//fn is_write_locked(v:u32) -> bool {
//    v & LOCKED_BIT != 0
//}
#[inline(always)]
fn has_waiters(v: u32) -> bool {
    v & (PARKED_BIT | WPARKED_BIT) != 0
}
#[inline(always)]
fn has_no_waiters(v: u32) -> bool {
    v & (PARKED_BIT | WPARKED_BIT) == 0
}

#[inline(always)]
fn is_write_lockable(v: u32) -> bool {
    is_not_write_locked(v) && (has_readers(v) || has_no_waiters(v))
}
#[inline(always)]
fn is_read_lockable(v: u32) -> bool {
    (has_readers(v) || (has_no_waiters(v) && is_not_write_locked(v))) && !has_readers_max(v)
}

#[inline(always)]
fn wake_readers(futex: &Futex, to_unactivate: u32, converting: bool) -> ReadLock {
    // at least one reader must have been marked + READER_OVERF
    let rb = if converting { 0 } else { READER_UNITY };
    let v = futex.fetch_xor(
        PARKED_BIT | to_unactivate | READER_OVERF | rb,
        Ordering::Relaxed,
    );
    assert_eq!(v & to_unactivate, to_unactivate);
    if !converting {
        //otherwise threads may be already taking read lock
        assert_ne!(v & READER_UNITY, rb);//BUG: fired
    }
    assert_eq!((v ^ to_unactivate) & LOCKED_BIT, 0);
    let c = futex.wake_readers();
    //TODO: on peut se contanter de réveiller MAX_WAKED_READERS, ce qui évite l'assertion
    assert!(c <= MAX_WAKED_READERS);
    let cur = futex.fetch_sub(READER_OVERF - READER_UNITY * (c as u32), Ordering::Relaxed);
    ReadLock::new(futex, cur)
}

// SyncPhasedLocker
// ---------------
impl SyncPhasedLocker {
    #[inline(always)]
    pub const fn new(p: Phase) -> Self {
        SyncPhasedLocker(Futex::new(p.bits()))
    }
    #[inline(always)]
    /// Return the current phase and synchronize with the end of the
    /// phase transition that leads to this phase.
    pub fn phase(&self) -> Phase {
        Phase::from_bits_truncate(self.0.load(Ordering::Acquire))
    }
    #[inline(always)]
    /// Returns a mutable phase locker
    pub fn lock_mut<'a, T: ?Sized>(&'a mut self, v: &'a T) -> SyncPhaseGuard<'_, T> {
        let cur = self.0.fetch_or(LOCKED_BIT, Ordering::Acquire);
        SyncPhaseGuard::new(v, Lock::new(&self.0, cur))
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
        on_parking_how: impl Fn(Phase) -> LockNature,
        hint: Phase,
    ) -> LockResult<SyncReadPhaseGuard<'_, T>, SyncPhaseGuard<'_, T>> {
        match self.raw_lock(how, on_parking_how, hint) {
            LockResult::Write(l) => LockResult::Write(SyncPhaseGuard::new(v, l)),
            LockResult::Read(l) => LockResult::Read(SyncReadPhaseGuard::new(v, l)),
            LockResult::None(p) => LockResult::None(p),
        }
    }
    #[inline(always)]
    /// try to lock the phase.
    ///
    /// If the returned value is a Some(LockResult::Read), then other threads
    /// may also hold a such a lock. This lock call synchronize with the
    /// phase transition that leads to the current phase and the phase will
    /// not change while this lock is held
    ///
    /// If the returned value is a Some(LockResult::Write), then only this thread
    /// hold the lock and the phase can be atomically transitionned using the
    /// returned lock.
    ///
    /// If the returned value is Some(LockResult::None), then the call to lock synchronize
    /// whit the end of the phase transition that led to the current phase.
    ///
    /// If the returned value is None, the the lock is held by other threads and could
    /// not be obtain.
    pub fn try_lock<'a, T: ?Sized>(
        &'a self,
        v: &'a T,
        how: impl Fn(Phase) -> LockNature,
        hint: Phase,
    ) -> Option<LockResult<SyncReadPhaseGuard<'_, T>, SyncPhaseGuard<'_, T>>> {
        self.try_raw_lock(how, hint).map(|l| match l {
            LockResult::Write(l) => LockResult::Write(SyncPhaseGuard::new(v, l)),
            LockResult::Read(l) => LockResult::Read(SyncReadPhaseGuard::new(v, l)),
            LockResult::None(p) => LockResult::None(p),
        })
    }
    #[inline(always)]
    fn try_raw_lock(
        &self,
        how: impl Fn(Phase) -> LockNature,
        hint: Phase,
    ) -> Option<LockResult<ReadLock<'_>, Lock<'_>>> {
        let mut cur;
        match how(hint) {
            LockNature::None => {
                cur = self.0.load(Ordering::Acquire);
                let p = Phase::from_bits_truncate(cur);
                if hint == p {
                    return Some(LockResult::None(p));
                }
            }
            LockNature::Write => {
                match self.0.compare_exchange(
                    hint.bits(),
                    hint.bits() | LOCKED_BIT,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(x) => return Some(LockResult::Write(Lock::new(&self.0, x))),
                    Err(x) => cur = x,
                }
            }
            LockNature::Read => {
                match self.0.compare_exchange(
                    hint.bits(),
                    hint.bits() + READER_UNITY,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(x) => return Some(LockResult::Read(ReadLock::new(&self.0, x))),
                    Err(x) => cur = x,
                }
            }
        }
        loop {
            match how(Phase::from_bits_truncate(cur)) {
                LockNature::None => {
                    fence(Ordering::Acquire);
                    let p = Phase::from_bits_truncate(cur);
                    return Some(LockResult::None(p));
                }
                LockNature::Write => {
                    if !is_write_locked(cur) && !has_readers(cur) && !has_waiters(cur) {
                        match self.0.compare_exchange_weak(
                            cur,
                            cur | LOCKED_BIT,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        ) {
                            Ok(x) => return Some(LockResult::Write(Lock::new(&self.0, x))),
                            Err(x) => {
                                cur = x;
                                continue;
                            }
                        }
                    } else {
                        return None;
                    }
                }
                LockNature::Read => {
                    if is_read_lockable(cur) {
                        match self.0.compare_exchange_weak(
                            cur,
                            cur + READER_UNITY,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        ) {
                            Ok(x) => return Some(LockResult::Read(ReadLock::new(&self.0, x))),
                            Err(x) => {
                                cur = x;
                                continue;
                            }
                        }
                    } else {
                        return None;
                    }
                }
            }
        }
    }
    #[inline(always)]
    fn raw_lock(
        &self,
        how: impl Fn(Phase) -> LockNature,
        on_parking_how: impl Fn(Phase) -> LockNature,
        hint: Phase,
    ) -> LockResult<ReadLock<'_>, Lock<'_>> {
        let mut cur = hint.bits();
        match how(hint) {
            LockNature::None => {
                cur = self.0.load(Ordering::Acquire);
                let p = Phase::from_bits_truncate(cur);
                if p == hint || matches!(how(p), LockNature::None) {
                    return LockResult::None(p);
                }
            }
            LockNature::Write => {
                match self.0.compare_exchange(
                    cur,
                    cur | LOCKED_BIT,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => return LockResult::Write(Lock::new(&self.0, cur)),
                    Err(x) => {
                        cur = x;
                    }
                }
            }
            LockNature::Read => {
                match self.0.compare_exchange_weak(
                    cur,
                    cur + READER_UNITY,
                    Ordering::Acquire,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        return LockResult::Read(ReadLock::new(&self.0, cur));
                    }
                    Err(x) => {
                        cur = x;
                    }
                }
            }
        }

        let p = Phase::from_bits_truncate(cur);

        match how(p) {
            LockNature::Write => {
                if is_write_lockable(cur)
                    && has_no_readers(cur)
                    && self
                        .0
                        .compare_exchange(
                            cur,
                            cur | LOCKED_BIT,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        )
                        .is_ok()
                {
                    return LockResult::Write(Lock::new(&self.0, cur));
                }
            }
            LockNature::Read => {
                let mut spin_wait = SpinWait::new();
                loop {
                    if !is_read_lockable(cur)
                    {
                        break;
                    }
                    match self.0.compare_exchange_weak(
                        cur,
                        cur + READER_UNITY,
                        Ordering::Acquire,
                        Ordering::Relaxed,
                    ) {
                        Ok(_) => {
                            return LockResult::Read(ReadLock::new(&self.0, cur));
                        }
                        Err(_) =>
                        {
                            if !spin_wait.spin_no_yield() {
                                break;
                            }
                            cur = self.0.load(Ordering::Relaxed);
                            if !(how(Phase::from_bits_truncate(cur)) == LockNature::Read) {
                                break;
                            }
                        }
                    }
                }
            }
            LockNature::None => {
                fence(Ordering::Acquire);
                return LockResult::None(p);
            }
        }
        self.raw_lock_slow(how, on_parking_how)
    }
    #[cold]
    fn raw_lock_slow(
        &self,
        how: impl Fn(Phase) -> LockNature,
        on_parking_how: impl Fn(Phase) -> LockNature,
    ) -> LockResult<ReadLock<'_>, Lock<'_>> {
        let mut spin_wait = SpinWait::new();

        let mut cur = self.0.load(Ordering::Relaxed);

        loop {
            match how(Phase::from_bits_truncate(cur)) {
                LockNature::None => {
                    fence(Ordering::Acquire);
                    return LockResult::None(Phase::from_bits_truncate(cur));
                }
                LockNature::Write => {
                    if is_write_lockable(cur) {
                        if has_no_readers(cur) {
                            match self.0.compare_exchange_weak(
                                cur,
                                cur | LOCKED_BIT,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(_) => {
                                    return LockResult::Write(Lock::new(&self.0, cur));
                                }
                                Err(x) => {
                                    cur = x;
                                    //hint::spin_loop();
                                    continue;
                                }
                            }
                        } else {
                            //lock while readers
                            assert!(has_readers(cur));
                            assert_eq!(cur & LOCKED_BIT, 0);
                            match self.0.compare_exchange_weak(
                                cur,
                                cur | LOCKED_BIT,
                                Ordering::Acquire,
                                Ordering::Relaxed,
                            ) {
                                Ok(x) => cur = x | LOCKED_BIT,
                                Err(x) => {
                                    cur = x;
                                    //hint::spin_loop();
                                    continue;
                                }
                            }
                            // wait for reader releasing the lock
                            let mut spinwait = SpinWait::new();
                            while spinwait.spin() {
                                cur = self.0.load(Ordering::Acquire);
                                if has_no_readers(cur) {
                                    return LockResult::Write(Lock::new(&self.0, cur));
                                }
                            }

                            while cur & LOCKED_BIT != 0 {
                                match self.0.compare_exchange_weak(
                                    cur,
                                    (cur | WPARKED_BIT) & !LOCKED_BIT,
                                    Ordering::Relaxed,
                                    Ordering::Relaxed,
                                ) {
                                    Err(x) => {
                                        cur = x;
                                        if has_no_readers(cur) {
                                            fence(Ordering::Acquire);
                                            return LockResult::Write(Lock::new(&self.0, cur));
                                        }
                                    }
                                    Ok(_) => {
                                        cur = (cur | WPARKED_BIT) & !LOCKED_BIT;
                                        break;
                                    }
                                }
                            }

                            if self.0.compare_and_wait_as_writer(cur) {
                                //There could have more parked thread
                                cur = self.0.fetch_or(WPARKED_BIT, Ordering::Acquire);
                                assert_ne!(cur & LOCKED_BIT, 0);
                                let lock = Lock::new(&self.0, cur);
                                match how(Phase::from_bits_truncate(cur)) {
                                    LockNature::Write => return LockResult::Write(lock),
                                    LockNature::Read => {
                                        return LockResult::Read(
                                            lock.into_read_lock(Phase::from_bits_truncate(cur)),
                                        )
                                    }
                                    LockNature::None => {
                                        return LockResult::None(Phase::from_bits_truncate(cur))
                                    }
                                }
                            }

                            cur = self.0.load(Ordering::Relaxed);
                            spin_wait.reset();
                            continue;

                            // TODO: park here
                            // so LOCKED_BIT without WPARKED_BIT should mean waiting thread and
                            // LOCKED_BIT with WPARKED_BIT parked writer
                            // si LOCKED_BIT => pas de nouveau reader
                            // dans READER drop slow, tester si WPARKED pas pour LOCKED_BIT
                            //
                            // puis: autoriser à ce qu'un LOCK soit pausé durant les drop slow
                            // puis: probablement il faudra ralonger les SpinWait.
                            //loop {
                            //    match self.0.compare_exchange_weak(
                            //        cur,
                            //        cur & !LOCKED_BIT,
                            //        Ordering::Relaxed,
                            //        Ordering::Relaxed,
                            //    ) {
                            //        Ok(x) => {
                            //            cur = x;
                            //            break;
                            //        }
                            //        Err(x) => {
                            //            if has_readers(x) {
                            //                cur = x;
                            //                hint::spin_loop();
                            //                continue;
                            //            } else {
                            //                fence(Ordering::Acquire);
                            //                return LockResult::Write(Lock::new(&self.0,x));
                            //                }
                            //        }
                            //    }
                            //}
                        }
                    }
                    if cur & WPARKED_BIT == 0 && spin_wait.spin() {
                        cur = self.0.load(Ordering::Relaxed);
                        continue;
                    }
                }
                LockNature::Read => {
                    if is_read_lockable(cur) {
                        match self.0.compare_exchange_weak(
                            cur,
                            cur + READER_UNITY,
                            Ordering::Acquire,
                            Ordering::Relaxed,
                        ) {
                            Ok(_) => return LockResult::Read(ReadLock::new(&self.0, cur)),
                            Err(x) => {
                                cur = x;
                                continue;
                            }
                        }
                    }
                    if has_no_waiters(cur) && spin_wait.spin() {
                        cur = self.0.load(Ordering::Relaxed);
                        continue;
                    }
                }
            }
            match on_parking_how(Phase::from_bits_truncate(cur)) {
                LockNature::None => {
                    fence(Ordering::Acquire);
                    return LockResult::None(Phase::from_bits_truncate(cur));
                }
                LockNature::Write => {
                    if cur & WPARKED_BIT == 0 {
                        match self.0.compare_exchange_weak(
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

                    if self.0.compare_and_wait_as_writer(cur) {
                        //There could have more parked thread
                        cur = self.0.fetch_or(WPARKED_BIT, Ordering::Relaxed);
                        assert_ne!(cur & LOCKED_BIT, 0);
                        let lock = Lock::new(&self.0, cur);
                        match how(Phase::from_bits_truncate(cur)) {
                            LockNature::Write => return LockResult::Write(lock),
                            LockNature::Read => {
                                return LockResult::Read(
                                    lock.into_read_lock(Phase::from_bits_truncate(cur)),
                                )
                            }
                            LockNature::None => {
                                return LockResult::None(Phase::from_bits_truncate(cur))
                            }
                        }
                    }
                }
                LockNature::Read => {
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

                    if self.0.compare_and_wait_as_reader(cur) {
                        let cur = self.0.load(Ordering::Relaxed);
                        assert_ne!(cur & (READER_BITS | READER_OVERF), 0);
                        let lock = ReadLock::new(&self.0, cur);
                        match how(Phase::from_bits_truncate(cur)) {
                            LockNature::Read => return LockResult::Read(lock),
                            LockNature::None => {
                                return LockResult::None(Phase::from_bits_truncate(cur))
                            }
                            LockNature::Write => {
                                spin_wait.reset();
                                continue;
                            } //drop the lock and try again;
                        }
                    }
                }
            }
            cur = self.0.load(Ordering::Relaxed);
            spin_wait.reset();
        }
    }
}
