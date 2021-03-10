use super::destructor;
use core::cell::Cell;
use core::sync::atomic::{AtomicU8, Ordering};
use parking_lot::{lock_api::RawMutex as _, Mutex, Once, OnceState, RawMutex};


/// At type to store a handler
/// that is called with `data` as argument
/// if the function register has been called
/// before main exit.
/// 
/// The handler is called just after main exit.
///
/// This is equivalent to a local variable declared
/// in main body but that can be declared as a static
/// variable.
///
/// Contrarily to `libc::at_exit` registration of functions does not
/// involve any memory allocation.
pub struct AtExit<T: 'static> {
    pub data:    T,
    pub on_exit: fn(&'static T),
    pub managed: AtExitManaged,
}

#[derive(Copy, Clone, Eq, PartialEq)]
#[repr(u8)]
pub enum Status {
    NonRegistered = 0,
    Registrating  = 1,
    Registered    = 2,
    Executing     = 3,
    Executed      = 4,
    Poisoned      = 5,
}
type OnExitRef = &'static (dyn OnExit + Sync);

pub struct AtExitManaged {
    next:   Cell<Option<OnExitRef>>,
    once:   Once,
    status: AtomicU8,
}

pub const COMPLETE_INIT: AtExitManaged = AtExitManaged {
    next:   Cell::new(None),
    once:   Once::new(),
    status: AtomicU8::new(0),
};

trait OnExit {
    fn execute(&'static self);
    fn get_next(&'static self) -> Option<OnExitRef>;
}

impl<T: 'static> OnExit for AtExit<T> {
    fn execute(&'static self) {
        //debug_assert!(self.status.get() == Status::Registered);
        self.managed
            .status
            .store(Status::Executing as u8, Ordering::Relaxed);
        (self.on_exit)(&self.data);
        self.managed
            .status
            .store(Status::Executed as u8, Ordering::Relaxed);
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

impl<T: Sync> AtExit<T> {
    /// Return the current status
    pub fn status(&self) -> Status {
        unsafe { core::mem::transmute(self.managed.status.load(Ordering::Relaxed)) }
    }

    /// Register the current object for call of on_thread_exit at
    /// thread destruction.
    ///
    /// Return an error if it is already registered or if registration
    /// on progress in an other thread.
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
            OnceState::Done => Err(self.status()),
            OnceState::Poisoned => Err(Status::Poisoned),
        }
    }
}
unsafe impl<T: Sync> Sync for AtExit<T> {}
