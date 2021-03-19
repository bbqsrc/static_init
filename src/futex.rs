
#[cfg(windows)]
mod windows {
    use core::mem::size_of;
    use winapi::shared::minwindef::TRUE;
    use winapi::um::winbase::INFINITE;
    use winapi::um::synchapi::{WaitOnAdress,WakeByAddressAll};
    pub(crate) fn park(at: &AtomicU32, value: u32) -> bool {
       WaitOnAddress(at as *const _ as *const _, (&value) as *const _ as *const _ 
       ,size_of::<u32>(),INFINITE) == TRUE 
    }
    pub(crate) fn unpark_all(at: &AtomicUsize) {
        WakeByAddressAll(at as *const _ as *const _)
    }
}
#[cfg(windows)]
pub(crate) use windows::{park,unpark_all};

#[cfg(any(target_os="linux",target_os="android"))]
mod linux {
    use core::sync::atomic::AtomicU32;
    use core::ptr;
    use libc::{syscall, SYS_futex, FUTEX_WAIT, FUTEX_PRIVATE_FLAG, FUTEX_WAKE};

    pub(crate) fn park(at: &AtomicU32, value: u32) -> bool {
        unsafe{syscall( SYS_futex,at as *const _ as *const _, FUTEX_WAIT | FUTEX_PRIVATE_FLAG, value, ptr::null::<u32>()) == 0}
    }
    pub(crate) fn unpark_all(at: &AtomicU32) {
        unsafe{syscall(SYS_futex, at as *const _ as *const _, FUTEX_WAKE | FUTEX_PRIVATE_FLAG, u32::MAX)};
    }
}
#[cfg(any(target_os="linux",target_os="android"))]
pub(crate) use linux::{park,unpark_all};

#[cfg(not(any(target_os="linux",target_os="android",target_os="windows")))]
mod other {
    use parking_lot_core::{DEFAULT_PARK_TOKEN,DEFAULT_UNPARK_TOKEN};

    pub(crate) fn park(at: &AtomicU32, value: u32) -> bool {
            parking_lot_core::park(
                at as *const _ as usize,
                || at.load(Ordering::Relaxed) == value,
                || {},
                |_, _| {},
                parking_lot_core::DEFAULT_PARK_TOKEN,
                None,
            );
    }
    pub(crate) fn unpark(at: &AtomicU32, value: u32) -> bool {
         parking_lot_core::unpark_all(
             this as *const _ as usize,
             parking_lot_core::DEFAULT_UNPARK_TOKEN,
         );
    }
}
#[cfg(not(any(target_os="linux",target_os="android",target_os="windows")))]
pub(crate) use other::{park,unpark_all};

mod mutex {
use super::{park, unpark_all};
use core::sync::atomic::{fence,AtomicU32,Ordering};
use crate::Phase;
use crate::phase::*;

struct PhasedLock(AtomicU32);

struct Lock<'a>{
    state: &'a AtomicU32,
    pub on_unlock: Phase,
}

impl<'a> Lock<'a> {
    fn unlock(self) {}
}

impl<'a> Drop for Lock<'a> {
    fn drop(&mut self) {
        let prev = self.state.swap(self.on_unlock.0 & !(PARKED_BIT|LOCKED_BIT),Ordering::Release);
        if prev & PARKED_BIT != 0 {
            unpark_all(self.state)
        }
    }
}


impl PhasedLock {
    fn phase(&self) -> Phase {
        Phase(self.0.load(Ordering::Acquire))
    }
fn lock(
    &self,
    shall_proceed: impl Fn(Phase) -> bool,
    into_phase: impl Fn(Phase) -> Phase,
    #[cfg(debug_mode)]
    id: &AtomicUsize
) -> Option<Lock> {

    use crate::spinwait::SpinWait;

    let mut spin_wait = SpinWait::new();

    let mut cur = self.0.load(Ordering::Relaxed);

    loop {
        if !shall_proceed(Phase(cur & !(PARKED_BIT|LOCKED_BIT))) {
            fence(Ordering::Acquire);
            return None;
        }
        if cur & LOCKED_BIT == 0 {
            let target = into_phase(Phase(cur)).0 & !(PARKED_BIT|LOCKED_BIT);
            match self.0.compare_exchange_weak(
                cur,
                target | LOCKED_BIT,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => {cur=target; break}
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

        park(&self.0,cur);
        spin_wait.reset();
        cur = self.0.load(Ordering::Relaxed);
    }
    Some(Lock{state: &self.0,on_unlock: Phase(cur)})
}

}
}
