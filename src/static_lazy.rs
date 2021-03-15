use super::StaticInfo;
use core::cell::{Cell, UnsafeCell};
use core::mem::{forget, MaybeUninit};
use core::ops::{Deref, DerefMut};

use crate::at_exit::{Status,AtExitTrait};

pub use parking_lot::{Once as PkOnce, OnceState};

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
        fn call_once<F: FnOnce()>(&self, f: F) {
            if !global_inited_hint() {
                self.0.call_once(f)
            }
        }
        fn state(&self) -> OnceState {
            self.0.state()
        }
    }
}


#[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
mod uninited {

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
        fn call_once<F: FnOnce()>(&self, f: F) {
            self.0.call_once(f)
        }
        fn state(&self) -> OnceState {
            self.0.state()
        }
    }
}
#[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
use inited::GlobalOnce;
#[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
use uninited::GlobalOnce;

struct LocalOnce(Cell<OnceState>);

impl LocalOnce {
    pub const fn new() -> Self {
        Self(Cell::new(OnceState::New))
    }
}

impl Once for LocalOnce {
    fn call_once<F: FnOnce()>(&self, f: F) {
        if !(self.0.get() == OnceState::Done) {
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
    }
    fn state(&self) -> OnceState {
        self.0.get()
    }
}

impl Once for PkOnce {
    fn call_once<F: FnOnce()>(&self, f: F) {
        self.call_once(f);
    }
    fn state(&self) -> OnceState {
        self.state()
    }
}
struct NotOnce;
impl Once for NotOnce {
    fn call_once<F: FnOnce()>(&self, f: F) {
        f()
    }
    fn state(&self) -> OnceState {
       OnceState::New 
    }
}


pub trait Generator<T> {
    fn generate(_: &Self) -> T;
}

impl<T: FnOnce() -> T> Generator<T> for T {
    fn generate(this: &Self) -> T {
        this()
    }
}

pub trait StaticAccessor<T> {
    fn access(this: &Self, _: &T);
}

pub trait StaticAccessorMut<T> {
    fn access_mut(this: &Self, _: &mut T);
}

impl<T> StaticAccessor<T> for fn(&T) {
    fn access(this: &Self, data: &T) {
        this(data)
    }
}
impl<T> StaticAccessorMut<T> for fn(&mut T) {
    fn access_mut(this: &Self, data: &mut T) {
        this(data)
    }
}
impl<T: StaticAccessor<T>> StaticAccessorMut<T> for T {
    fn access_mut(this: &Self, data: &mut T) {
        StaticAccessor::access(this, data)
    }
}

/// Trait with similar semantic as parking_lot::Once
pub trait Once {
    fn call_once<F: FnOnce()>(&self, f: F);
    fn state(&self) -> OnceState;
}

/// Trait for type that will perform an
/// an action only once the first time
/// the method access_once is called.
trait AccessOnceTrait {
    /// with behavior similar to Once
    fn access_once(this: &Self);
    fn state(this: &Self) -> Status;
}

/// Will call object of type F
/// when `access_once` is called, and will do
/// it only once.
///
/// The access will be throught type F implementation
/// of StaticAccessor.
pub struct AccessOnce<T, F, O> {
    value:    T,
    once:     O,
    accessor: F,
}

/// Will call object of type F
/// when `access_once` is called, and will do
/// it only once.
///
/// The access will be throught type F implementation
/// of StaticAccessorMut.
///
/// The type implement interior mutability so that
/// call to `access_mut` see a mutable variable
pub struct AccessOnceMut<T, F, O> {
    value:    UnsafeCell<T>,
    once:     O,
    accessor: F,
}
impl<T, F, O> AccessOnce<T, F, O> {
    const fn new(value: T, accessor: F, once: O) -> Self {
        Self {
            value,
            once,
            accessor,
        }
    }
}
impl<T, F, O> AccessOnceMut<T, F, O> {
    const fn new(value: T, accessor: F, once: O) -> Self {
        Self {
            value: UnsafeCell::new(value),
            once,
            accessor,
        }
    }
}

impl<T, F, O> Deref for AccessOnce<T, F, O> {
    type Target = T;
    fn deref(&self) -> &T {
        &self.value
    }
}
impl<T, F, O> DerefMut for AccessOnce<T, F, O> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.value
    }
}
impl<T, F, O> Deref for AccessOnceMut<T, F, O> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.value.get() }
    }
}
impl<T, F, O> DerefMut for AccessOnceMut<T, F, O> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.value.get() }
    }
}

impl<T, F: StaticAccessor<T>, O: Once> AccessOnceTrait for AccessOnce<T, F, O> {
    fn access_once(this: &Self) {
        this.once
            .call_once(|| StaticAccessor::access(&this.accessor, &this.value));
    }
    fn state(this: &Self) -> Status {
        this.once.state().into()
    }
}

impl<T, F: StaticAccessorMut<T>, O: Once> AccessOnceTrait for AccessOnceMut<T, F, O> {
    fn access_once(this: &Self) {
        let accessor = &this.accessor;
        let value = &this.value;
        this.once
            .call_once(|| StaticAccessorMut::access_mut(accessor, unsafe { &mut *value.get() }));
    }
    fn state(this: &Self) -> Status {
        this.once.state()
    }
}

/// Will call object of type F
/// when `access_once` is called, and will do
/// it only once.
///
/// The access will be throught type F implementation
/// of StaticAccessor.
pub struct AccessOnceAndRegister<T, F> {
    value:    T,
    accessor: F,
}

impl<T, F> AccessOnceAndRegister<T, F> {
    const fn new(value: T, accessor: F) -> Self {
        Self {
            value,
            accessor,
        }
    }
}

impl<T:Deref, F> Deref for AccessOnceAndRegister<T, F> {
    type Target = <T as Deref>::Target;
    fn deref(&self) -> &Self::Target {
        &*self.value
    }
}

impl<T:DerefMut, F> DerefMut for AccessOnceAndRegister<T, F> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut *self.value
    }
}

impl<T: AtExitTrait, F: StaticAccessor<T>> AccessOnceTrait for AccessOnceAndRegister<T, F> {
    fn access_once(this: &Self) {
        this.value.register(|| StaticAccessor::access(&this.accessor, &this.value));
    }
    fn state(this: &Self) -> Status {
        this.value.status()
    }
}





/// Will perform the access to the variable
/// when dereferenced
struct OnFirstAccess<T>(T);

impl<T, F, O> OnFirstAccess<AccessOnceMut<T, F, O>> {
    pub const fn new_mut(value: T, generator: F, once: O) -> Self {
        Self(AccessOnceMut::new(value, generator, once))
    }
}
impl<T, F, O> OnFirstAccess<AccessOnce<T, F, O>> {
    pub const fn new(value: T, generator: F, once: O) -> Self {
        Self(AccessOnce::new(value, generator, once))
    }
}
impl<T: AccessOnceTrait> OnFirstAccess<T> {
    pub fn state(this: &Self) -> OnceState {
        AccessOnceTrait::state(&this.0)
    }
}

impl<T: AccessOnceTrait + Deref> Deref for OnFirstAccess<T> {
    type Target = <T as Deref>::Target;
    fn deref(&self) -> &Self::Target {
        AccessOnceTrait::access_once(&self.0);
        &*self.0
    }
}
impl<T: AccessOnceTrait + DerefMut> DerefMut for OnFirstAccess<T> {
    fn deref_mut(&mut self) -> &mut <T as Deref>::Target {
        AccessOnceTrait::access_once(&mut self.0);
        &mut *self.0
    }
}

impl<T: Deref> Deref for ConstAccessOnly<T> {
    type Target = <T as Deref>::Target;
    fn deref(&self) -> &Self::Target {
        &*self.0
    }
}

/// Will initialize the variable
/// when dereferenced
struct GeneratorToMutAccess<F>(F);

impl<T, F: Generator<T>> StaticAccessorMut<MaybeUninit<T>> for GeneratorToMutAccess<F> {
    fn access_mut(this: &Self, data: &mut MaybeUninit<T>) {
        *data = MaybeUninit::new(Generator::generate(&this.0));
    }
}

pub struct GenerateOnFirstAccess<T, F, O>(
    OnFirstAccess<AccessOnceMut<MaybeUninit<T>, GeneratorToMutAccess<F>, O>>,
);

impl<T, F, O> GenerateOnFirstAccess<T, F, O> {
    pub const fn new(generator: F, once: O) -> Self {
        Self(OnFirstAccess::new_mut(
            MaybeUninit::uninit(),
            GeneratorToMutAccess(generator),
            once,
        ))
    }
}
impl<T, F: Generator<T>, O: Once> GenerateOnFirstAccess<T, F, O> {
    fn state(this: &Self) -> OnceState {
        OnFirstAccess::state(&this.0)
    }
}

impl<T, F: Generator<T>, O: Once> Deref for GenerateOnFirstAccess<T, F, O> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*(*self.0).as_ptr() }
    }
}
impl<T, F: Generator<T>, O: Once> DerefMut for GenerateOnFirstAccess<T, F, O> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *(*self.0).as_mut_ptr() }
    }
}

/// Gives access to T targets but only
/// throug Deref not DerefMut.
pub struct ConstAccessOnly<T>(T);

impl<T> ConstAccessOnly<T> {
    /// Create a new `ConstAccessOnly`
    pub const fn new(value: T) -> Self {
        ConstAccessOnly(value)
    }
    /// Get access to the inner type.
    pub const fn inner(this: &Self) -> &T {
        &this.0
    }
}

macro_rules! impl_lazy {
    ($name:ident, $once:ty $(,$unsafe:ident)?) => {
        pub struct $name<T,F> (GenerateOnFirstAccess<T,F,$once>, #[cfg(debug_mode)] Option<StaticInfo>);

        impl <T,F> $name<T,F> {
            pub const $($unsafe)? fn new(f: F) -> Self {
                Self(GenerateOnFirstAccess::new(f, <$once>::new()),
                #[cfg(debug_mode)]
                None
                )
            }
        }
        impl <T,F> $name<T,F> {
            pub const $($unsafe)? fn new_with_info(f: F, _info: StaticInfo) -> Self {
                Self(GenerateOnFirstAccess::new(f, <$once>::new()),
                #[cfg(debug_mode)]
                Some
                )
            }
        }
        impl<T,F: Generator<T>> $name<T,F> {
            pub fn state(this: &Self) -> OnceState {
                GenerateOnFirstAccess::state(&this.0)
            }
        }

        impl<T,F: Generator<T>> Deref for $name<T,F> {
            type Target = T;
            fn deref(&self) -> &T {
                #[cfg(debug_mode)]
                assert_ne!($name::state(self), OnceState::InProgress,
                "Recursive initialization of static {:#?}",self.1);
                &*self.0
            }
        }
        impl<T,F: Generator<T>> DerefMut for $name<T,F> {
            fn deref_mut(&mut self) -> &mut T {
                #[cfg(debug_mode)]
                assert_ne!($name::state(self), OnceState::InProgress,
                "Recursive initialization of static {:#?}",self.1);
                &mut *self.0
            }
        }
    }
}

impl_lazy! {Lazy,PkOnce}

impl_lazy! {GlobalLazy,GlobalOnce, unsafe}

impl_lazy! {LocalLazy,LocalOnce}

use crate::at_exit::{AtExit};
//use crate::ConstDrop;
//
///// Will initialize the variable
///// when dereferenced
//struct GeneratorToMutAccessAndRegister<F, const FAILLIBLE: bool>(F);
//
//impl<T: ConstDrop + Sync + 'static, F: Generator<T>, const FAILLIBLE: bool> StaticAccessorMut<UnguardedAtExit<MaybeUninit<T>>>
//    for GeneratorToMutAccessAndRegister<F, FAILLIBLE>
//{
//    fn access_mut(this: &Self, data: &mut UnguardedAtExit<MaybeUninit<T>>) {
//        data.data = MaybeUninit::new(Generator::generate(&this.0));
//        if !FAILLIBLE && !unsafe{data.register()}.is_ok() {
//            unsafe{data.data.as_mut_ptr().drop_in_place()};
//            panic!("Enable to register destructor at exit.")
//        }
//    }
//}
//
//pub struct GenerateOnFirstAccessAndRegister<T:'static, F, O>(
//    OnFirstAccess<AccessOnceMut<UnguardedAtExit<MaybeUninit<T>>, GeneratorToMutAccessAndRegister<F, false>, O>>,
//);
//
//impl<T, F, O> GenerateOnFirstAccessAndRegister<T, F, O> {
//    pub const fn new(generator: F, once: O) -> Self {
//        Self(OnFirstAccess::new_mut(
//            UnguardedAtExit{data:MaybeUninit::uninit(),managed: UNGUARDED_COMPLETE_INIT},
//            GeneratorToMutAccessAndRegister(generator),
//            once,
//        ))
//    }
//}
//impl<T: 'static + Sync, F: Generator<T>, O: Once>
//    GenerateOnFirstAccessAndRegister<T, F, O>
//{
//    fn state(this: &Self) -> OnceState {
//        OnFirstAccess::state(&this.0)
//    }
//}
//
//impl<T:'static + Sync, F: Generator<T>, O: Once> Deref for GenerateOnFirstAccessAndRegister<T, F, O> {
//    type Target = T;
//    fn deref(&self) -> &T {
//        unsafe { &*(*self.0).data.as_ptr() }
//    }
//}
//impl<T: 'static + Sync, F: Generator<T>, O: Once> DerefMut for GenerateOnFirstAccessAndRegister<T, F, O> {
//    fn deref_mut(&mut self) -> &mut T {
//        unsafe { &mut *(*self.0).data.as_mut_ptr() }
//    }
//}
