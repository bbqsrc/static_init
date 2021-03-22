use crate::{Finaly, Generator, LazySequentializer, Phase, Sequential, StaticInfo,Sequentializer,LockNature,LockResult};
use core::cell::UnsafeCell;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::hint::unreachable_unchecked;

#[cfg(debug_mode)]
use crate::CyclicPanic;

/// Policy for lazy initialization
pub trait LazyPolicy {
    /// Shall the initialization be performed if the finalization callback failed to be registred
    const INIT_ON_REG_FAILURE: bool;
    /// shall the initialization be performed (tested at each access)
    fn shall_proceed(_: Phase) -> bool;
    fn initialized_ok(_: Phase) -> bool;
}

/// Generic lazy interior data storage, uninitialized with interior mutability data storage
/// that call T::finaly when finalized
pub struct UnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T: Finaly> Finaly for UnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        unsafe { &*self.get() }.finaly();
    }
}

/// Generic lazy interior data storage, uninitialized with interior mutability data storage
/// that call drop when finalized
pub struct DropedUnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T> Finaly for DropedUnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        unsafe { self.get().drop_in_place() };
    }
}

/// Trait implemented by generic lazy inner data.
///
/// Dereferencement of generic lazy will return a reference to
/// the inner data returned by the get method
pub trait LazyData {
    type Target;
    const INIT: Self;
    fn get(&self) -> *mut Self::Target;
}

impl<T> LazyData for UnInited<T> {
    type Target = T;
    const INIT: Self = Self(UnsafeCell::new(MaybeUninit::uninit()));
    fn get(&self) -> *mut T {
        self.0.get() as *mut T
    }
}

impl<T> LazyData for DropedUnInited<T> {
    type Target = T;
    const INIT: Self = Self(UnsafeCell::new(MaybeUninit::uninit()));
    fn get(&self) -> *mut T {
        self.0.get() as *mut T
    }
}

/// A type that wrap a Sequentializer and a raw data, and that may
/// initialize the data, at each access depending on the LazyPolicy
/// provided as generic argument.
pub struct GenericLazy<T, F, M, S> {
    value:          T,
    generator:      F,
    sequentializer: M,
    phantom:        PhantomData<S>,
    #[cfg(debug_mode)]
    _info:          Option<StaticInfo>,
}
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Sync for GenericLazy<T, F, M, S> where
    <T as LazyData>::Target: Sync
{
}
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Send for GenericLazy<T, F, M, S> where
    <T as LazyData>::Target: Send
{
}

impl<T, F, M, S> GenericLazy<T, F, M, S> {
    /// const initialize the lazy, the inner data may be in an uninitialized state
    pub const unsafe fn new_static(generator: F, sequentializer: M, value: T) -> Self {
        Self {
            value,
            generator,
            sequentializer,
            phantom: PhantomData,
            #[cfg(debug_mode)]
            _info: None,
        }
    }
    /// const initialize the lazy, the inner data may be in an uninitialized state and
    /// store some debuging informations
    pub const unsafe fn new_static_with_info(
        generator: F,
        sequentializer: M,
        value: T,
        _info: StaticInfo,
    ) -> Self {
        Self {
            value,
            generator,
            sequentializer,
            phantom: PhantomData,
            #[cfg(debug_mode)]
            _info: Some(_info),
        }
    }
    #[inline(always)]
    ///get access to the sequentializer
    pub fn sequentializer(this: &Self) -> &M {
        &this.sequentializer
    }
    #[inline(always)]
    ///get a pointer to the raw data
    pub fn get_raw_data(this: &Self) -> &T {
        &this.value
    }
    #[inline(always)]
    /// potentialy initialize the inner data
    ///
    /// this method is called every time the generic lazy is dereferenced
    pub fn init(this: &Self)
    where
        T: 'static + LazyData,
        M: 'static,
        for<'a> M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
        #[cfg(not(debug_mode))]
        {
            LazySequentializer::init(
                this,
                S::shall_proceed,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            );
        }
        #[cfg(debug_mode)]
        {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                <M as Sequentializer<Self>>::init(
                    this,
                    S::shall_proceed,
                    |data: &T| {
                        // SAFETY
                        // This function is called only once within the init function
                        // Only one thread can ever get this mutable access
                        let d = Generator::generate(&this.generator);
                        unsafe { data.get().write(d) };
                    },
                    S::INIT_ON_REG_FAILURE,
                )
            })) {
                Ok(_) => (),
                Err(x) => {
                    if x.is::<CyclicPanic>() {
                        match &this._info {
                            Some(info) => panic!("Circular initialization of {:#?}", info),
                            None => panic!("Circular lazy initialization detected"),
                        }
                    } else {
                        std::panic::resume_unwind(x)
                    }
                }
            }
        }
    }
}

impl<
        M: 'static,
        T: 'static + LazyData,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    > Deref for GenericLazy<T, F, M, S>
where
    for<'a> M: LazySequentializer<'a, Self>,
{
    type Target = T::Target;
    #[inline(always)]
    fn deref(&self) -> &T::Target {
        Self::init(self);
        // SAFETY
        // This is safe as long as the object has been initialized
        // this is the contract ensured by init.
        unsafe { &*self.value.get() }
    }
}

impl<
        M: 'static,
        T: 'static + LazyData,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    > DerefMut for GenericLazy<T, F, M, S>
where
    for<'a> M: LazySequentializer<'a, Self>,
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T::Target {
        Self::init(self);
        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<
        F: 'static + Generator<T::Target>,
        T: 'static + LazyData,
        M: 'static,
        S: 'static + LazyPolicy,
    > Sequential for GenericLazy<T, F, M, S>
{
    type Data = T;
    type Sequentializer = M;
    #[inline(always)]
    fn sequentializer(this: &Self) -> &Self::Sequentializer {
        &this.sequentializer
    }
    #[inline(always)]
    fn data(this: &Self) -> &Self::Data {
        &this.value
    }
}

/// A type that wrap a Sequentializer and a raw data, and that may
/// initialize the data, at each access depending on the LazyPolicy
/// provided as generic argument.
pub struct GenericMutLazy<T, F, M, S> {
    value:          T,
    generator:      F,
    sequentializer: M,
    phantom:        PhantomData<S>,
    #[cfg(debug_mode)]
    _info:          Option<StaticInfo>,
}
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Sync for GenericMutLazy<T, F, M, S> where
    <T as LazyData>::Target: Send
{
}
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Send for GenericMutLazy<T, F, M, S> where
    <T as LazyData>::Target: Send
{
}

impl<T, F, M, S> GenericMutLazy<T, F, M, S> {
    /// const initialize the lazy, the inner data may be in an uninitialized state
    pub const unsafe fn new_static(generator: F, sequentializer: M, value: T) -> Self {
        Self {
            value,
            generator,
            sequentializer,
            phantom: PhantomData,
            #[cfg(debug_mode)]
            _info: None,
        }
    }
    /// const initialize the lazy, the inner data may be in an uninitialized state and
    /// store some debuging informations
    pub const unsafe fn new_static_with_info(
        generator: F,
        sequentializer: M,
        value: T,
        _info: StaticInfo,
    ) -> Self {
        Self {
            value,
            generator,
            sequentializer,
            phantom: PhantomData,
            #[cfg(debug_mode)]
            _info: Some(_info),
        }
    }
    #[inline(always)]
    ///get access to the sequentializer
    pub fn sequentializer(this: &Self) -> &M {
        &this.sequentializer
    }
    #[inline(always)]
    /// potentialy initialize the inner data
    ///
    /// this method is called every time the generic lazy is dereferenced
    pub fn read_lock<'a>(this: &'a Self) -> M::ReadGuard
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
            if let LockResult::Read(l) = <M as Sequentializer::<'a,Self>>::lock(
                this,
                |_| LockNature::Read,
            ) {
                l
            } else {
                unsafe{unreachable_unchecked()}
            }
        }
    #[inline(always)]
    /// potentialy initialize the inner data
    ///
    /// this method is called every time the generic lazy is dereferenced
    pub fn write_lock<'a>(this: &'a Self) -> M::WriteGuard
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
            if let LockResult::Write(l) = <M as Sequentializer::<'a,Self>>::lock(
                this,
                |_| LockNature::Write,
            ) {
                l
            } else {
                unsafe{unreachable_unchecked()}
            }
        }
    #[inline(always)]
    /// potentialy initialize the inner data
    ///
    /// this method is called every time the generic lazy is dereferenced
    pub fn init_then_read_lock<'a>(this: &'a Self) -> M::ReadGuard
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
        #[cfg(not(debug_mode))]
        {
            LazySequentializer::init_or_read_guard(
                this,
                S::shall_proceed,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            )
        }
        #[cfg(debug_mode)]
        {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                <M as Sequentializer<Self>>::init_or_read_guard(
                    this,
                    S::shall_proceed,
                    |data: &T| {
                        // SAFETY
                        // This function is called only once within the init function
                        // Only one thread can ever get this mutable access
                        let d = Generator::generate(&this.generator);
                        unsafe { data.get().write(d) };
                    },
                    S::INIT_ON_REG_FAILURE,
                )
            })) {
                Ok(r) => r,
                Err(x) => {
                    if x.is::<CyclicPanic>() {
                        match &this._info {
                            Some(info) => panic!("Circular initialization of {:#?}", info),
                            None => panic!("Circular lazy initialization detected"),
                        }
                    } else {
                        std::panic::resume_unwind(x)
                    }
                }
            }
        }
    }
    #[inline(always)]
    /// potentialy initialize the inner data
    ///
    /// this method is called every time the generic lazy is dereferenced
    pub fn init_then_write_lock<'a>(this: &'a Self) -> M::WriteGuard
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
        #[cfg(not(debug_mode))]
        {
            LazySequentializer::init_or_write_guard(
                this,
                S::shall_proceed,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            )
        }
        #[cfg(debug_mode)]
        {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                <M as Sequentializer<Self>>::init_or_write_guard(
                    this,
                    S::shall_proceed,
                    |data: &T| {
                        // SAFETY
                        // This function is called only once within the init function
                        // Only one thread can ever get this mutable access
                        let d = Generator::generate(&this.generator);
                        unsafe { data.get().write(d) };
                    },
                    S::INIT_ON_REG_FAILURE,
                )
            })) {
                Ok(r) => r,
                Err(x) => {
                    if x.is::<CyclicPanic>() {
                        match &this._info {
                            Some(info) => panic!("Circular initialization of {:#?}", info),
                            None => panic!("Circular lazy initialization detected"),
                        }
                    } else {
                        std::panic::resume_unwind(x)
                    }
                }
            }
        }
    }
}

impl<T, F, M, S> Deref for GenericMutLazy<T, F, M, S> {
    type Target = T;
    #[inline(always)]
    ///get a pointer to the raw data
    fn deref(&self) -> &T {
        &self.value
    }
}

unsafe impl<
        F: 'static + Generator<T::Target>,
        T: 'static + LazyData,
        M: 'static,
        S: 'static + LazyPolicy,
    > Sequential for GenericMutLazy<T, F, M, S>
{
    type Data = T;
    type Sequentializer = M;
    #[inline(always)]
    fn sequentializer(this: &Self) -> &Self::Sequentializer {
        &this.sequentializer
    }
    #[inline(always)]
    fn data(this: &Self) -> &Self::Data {
        &this.value
    }
}
