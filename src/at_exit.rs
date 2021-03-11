#![cfg(any(elf,mach_o,coff))]

use super::{destructor,ConstDrop};
use core::cell::Cell;
use core::sync::atomic::{AtomicU8, Ordering};
use parking_lot::{lock_api::RawMutex as _, Mutex, Once, OnceState, RawMutex};


/// At type to store a handler that is called with `data` as argument if the function register has
/// been called before main exit.
/// 
/// The handler is called just after main exit.
///
/// This is equivalent to a local variable declared in main body but that can be declared as
/// a static variable.
///
/// Contrarily to `libc::at_exit` registration of functions does not involve any memory allocation.
pub struct AtExit<T: 'static> {
    pub data:    T,
    pub managed: AtExitManaged,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    /// drop_const call has not yet been registered
    NonRegistered = 0,
    /// drop_const registration is running     
    Registrating  = 1,
    /// drop_const is registered but execution not yet done     
    Registered    = 2,
    /// drop_const is being executed     
    Executing     = 3,
    /// drop_const has been executed     
    Executed      = 4,
    /// drop_const could not been registered and will never
    /// be registered but the data is still valid for use
    Poisoned      = 5,
}
type OnExitRef = &'static (dyn OnExit + Sync);

/// Opaque type used for registration management
pub struct AtExitManaged {
    next:   Cell<Option<OnExitRef>>,
    once:   Once,
    status: AtomicU8,
}

/// Used to const init `managed` field of `AtExit`
pub const COMPLETE_INIT: AtExitManaged = AtExitManaged {
    next:   Cell::new(None),
    once:   Once::new(),
    status: AtomicU8::new(0),
};


trait OnExit {
    fn execute(&'static self);
    fn get_next(&'static self) -> Option<OnExitRef>;
}

impl<T: 'static + ConstDrop> OnExit for AtExit<T> {
    fn execute(&'static self) {
        //debug_assert!(self.status.get() == Status::Registered);
        self.managed
            .status
            .store(Status::Executing as u8, Ordering::Relaxed);
        self.data.const_drop();
        self.managed
            .status
            .store(Status::Executed as u8, Ordering::Release);
    }
    fn get_next(&'static self) -> Option<OnExitRef> {
        //protected by REGISTER mutex.
        unsafe { *self.managed.next.as_ptr() }
    }
}


static REGISTER: Mutex<Option<OnExitRef>> = Mutex::const_new(RawMutex::INIT, None);

#[destructor(0)]
extern "C" fn execute_at_exit() {
    let mut l = REGISTER.lock();
    let mut list: Option<OnExitRef> = l.take();
    drop(l);
    while let Some(on_exit) = list {
        on_exit.execute();
        list = on_exit.get_next().or_else(|| {
            let mut l = REGISTER.lock();
            l.take()
        });
    }
}

impl<T: Sync + ConstDrop> AtExit<T> {
    /// Return the current status
    ///
    /// Ordering may be Relaxed or Acquire. If the 
    /// status is Executed, this method call will synchronize
    /// with the end of the execution of const_drop.
    pub fn status(&self,order: Ordering) -> Status {
        unsafe { core::mem::transmute(self.managed.status.load(order)) }
    }

    /// Register the current object for call of on_thread_exit at
    /// thread destruction.
    ///
    /// Return an error if it is already registered or if registration
    /// on progress in an other thread.
    ///
    /// If the error returned is Registered or Executing or Executed or if
    /// the result if Ok(()) this call will synchronize with the registration
    /// (which even if the result is Ok(()) could actualy have happen in an other thread.
    ///
    /// If the result is Err(Status::Executed) this call also synchronize with the en
    /// of the execution of drop_const.
    pub fn register(&'static self) -> Result<(), Status> {
        match self.managed.once.state() {
            OnceState::New => {
                self.managed.once.call_once(|| {
                    let mut reg = REGISTER.lock();
                    self.managed.next.set(reg.take());
                    *reg = Some(self as &_);
                    self.managed
                        .status
                        .store(Status::Registered as u8, Ordering::Relaxed);
                });
                Ok(())
            }
            OnceState::InProgress => Err(Status::Registrating),
            OnceState::Done => Err(self.status(Ordering::Acquire)),
            OnceState::Poisoned => Err(Status::Poisoned),
        }
    }
}
unsafe impl<T: Sync> Sync for AtExit<T> {}
