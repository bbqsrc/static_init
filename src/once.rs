pub use parking_lot::{Once as PkOnce, OnceState};

use super::{Manager,ManagerBase, Phase, Static};
use core::cell::Cell;
use core::mem::forget;

#[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
pub use inited::GlobalOnce;
#[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
pub use uninited::GlobalOnce;

pub struct LocalOnce(Cell<OnceState>);

impl LocalOnce {
    #[inline(always)]
    pub const fn new() -> Self {
        Self(Cell::new(OnceState::New))
    }
}

impl Once for LocalOnce {
    #[inline(always)]
    fn call_once<F: FnOnce()>(&self, f: F) {
        match self.0.get() {
          OnceState::Done => (),
          OnceState::New => {
            assert_eq!(self.0.get(), OnceState::New);
            self.0.set(OnceState::InProgress);
            struct OnPanic<'a>(&'a Cell<OnceState>);
            impl<'a> Drop for OnPanic<'a> {
                fn drop(&mut self) {
                    self.0.set(OnceState::Poisoned)
                }
            }
            let guard = OnPanic(&self.0);
            f();
            forget(guard);
            self.0.set(OnceState::Done);
            }
        s => panic!("Bad call to call_once while state is {:?}",s),
        }
    }
    #[inline(always)]
    fn state(&self) -> OnceState {
        self.0.get()
    }
}

impl Once for PkOnce {
    #[inline(always)]
    fn call_once<F: FnOnce()>(&self, f: F) {
        self.call_once(f);
    }
    #[inline(always)]
    fn state(&self) -> OnceState {
        self.state()
    }
}

/// Trait with similar semantic as parking_lot::Once
pub trait Once {
    fn call_once<F: FnOnce()>(&self, f: F);
    fn state(&self) -> OnceState;
}

impl<U: 'static + Once> ManagerBase for U {
    #[inline(always)]
    fn phase(&self) -> Phase {
        match self.state() {
            OnceState::New => Phase::New,
            OnceState::Done => Phase::Initialized,
            OnceState::Poisoned => Phase::PostInitializationPanic,
            OnceState::InProgress => Phase::Initialization,
        }
    }
}
unsafe impl<U: 'static + Once, T: 'static + Static<Manager = Self>> Manager<T> for U {
    #[inline(always)]
    fn register(
        s: &T,
        init: impl FnOnce(&<T as Static>::Data) -> bool,
        _: impl FnOnce(&<T as Static>::Data),
    ) {
        Static::manager(s).call_once(|| {
            init(Static::data(s));
        })
    }
}

#[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
mod inited {

    use core::sync::atomic::{AtomicBool, Ordering};

    use super::PkOnce;

    use super::{Once, OnceState};

    static LAZY_INIT_ENSURED: AtomicBool = AtomicBool::new(false);

    #[static_init_macro::constructor(__lazy_init_finished)]
    extern "C" fn mark_inited() {
        LAZY_INIT_ENSURED.store(true, Ordering::Release);
    }

    #[inline(always)]
    pub(crate) fn global_inited_hint() -> bool {
        LAZY_INIT_ENSURED.load(Ordering::Acquire)
    }

    /// As [parking_lot::Once] but with once test shortcircuited after `main` has been called
    pub struct GlobalOnce(PkOnce);

    impl GlobalOnce {
        /// The target object must be a static declared
        /// with attribute `#[constructor(n)]` where `n > 0`
        /// on plateforms that support constructor priorities
        pub const unsafe fn new() -> Self {
            Self(PkOnce::new())
        }
    }

    impl Once for GlobalOnce {
        #[inline(always)]
        fn call_once<F: FnOnce()>(&self, f: F) {
            if global_inited_hint() { } else {
                self.0.call_once(f)
            }
        }
        #[inline(always)]
        fn state(&self) -> OnceState {
            self.0.state()
        }
    }
}

#[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
mod uninited {
    use super::{Once,PkOnce,OnceState};

    /// As [parking_lot::Once] but with once test shortcircuited after `main` has been called
    pub struct GlobalOnce(PkOnce);

    impl GlobalOnce {
        /// The target object must be a static declared
        /// with attribute `#[constructor(n)]` where `n > 0`
        /// on plateforms that support constructor priorities
        pub const unsafe fn new() -> Self {
            Self(PkOnce::new())
        }
    }

    impl Once for GlobalOnce {
        #[inline(always)]
        fn call_once<F: FnOnce()>(&self, f: F) {
            self.0.call_once(f)
        }
        #[inline(always)]
        fn state(&self) -> OnceState {
            self.0.state()
        }
    }
}
