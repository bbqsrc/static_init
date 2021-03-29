mod futex;

mod spin_wait;

mod sync_phase_locker;
pub(crate) use sync_phase_locker::Mutex;
pub use sync_phase_locker::{SyncPhaseGuard, SyncPhasedLocker, SyncReadPhaseGuard};

mod unsync_phase_locker;
pub use unsync_phase_locker::UnSyncPhaseLocker;
pub use unsync_phase_locker::{UnSyncPhaseGuard, UnSyncReadPhaseGuard};


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
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
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

