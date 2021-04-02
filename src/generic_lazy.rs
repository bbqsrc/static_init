use crate::{
    Finaly, Generator, LazySequentializer, LockNature, LockResult, Phase, Sequential,
    Sequentializer, StaticInfo, Phased
};
use core::cell::UnsafeCell;
use core::hint::unreachable_unchecked;
use core::marker::PhantomData;
use core::mem::MaybeUninit;
use core::ops::{Deref,DerefMut};
use core::fmt::{Formatter,self,Display};

#[cfg(debug_mode)]
use crate::CyclicPanic;

/// Policy for lazy initialization
pub trait LazyPolicy {
    /// Shall the initialization be performed if the finalization callback failed to be registred
    const INIT_ON_REG_FAILURE: bool;
    /// shall the initialization be performed (tested at each access)
    fn shall_init(_: Phase) -> bool;
    fn is_accessible(_: Phase) -> bool;
}

/// Generic lazy interior data storage, uninitialized with interior mutability data storage
/// that call T::finaly when finalized
pub struct UnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T: Finaly> Finaly for UnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        //SAFETY: UnInited is only used as part of GenericLazy, that gives access
        //only if the Sequentializer is a Lazy Sequentializer
        //
        //So the lazy Sequentializer should only execute finaly if the object initialization
        //succeeded
        unsafe { &*self.get() }.finaly();
    }
}

/// Generic lazy interior data storage, uninitialized with interior mutability data storage
/// that call drop when finalized
pub struct DropedUnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T> Finaly for DropedUnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        //SAFETY: UnInited is only used as part of GenericLazy, that gives access
        //only if the Sequentializer is a Lazy Sequentializer
        //
        //So the lazy Sequentializer should only execute finaly if the object initialization
        //succeeded
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

/// Errors that happens when trying to get a readable access to a lazy
#[derive(Copy,Clone,Eq,PartialEq,Hash,Debug)]
pub struct AccessError {pub phase: Phase}

impl Display for AccessError {
    fn fmt(&self, ft: &mut Formatter<'_>) -> fmt::Result {
       write!(ft,"Error: inaccessible lazy in {}",self.phase)
    }
}

#[cfg(feature="parking_lot_core")]
impl std::error::Error for AccessError {}

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
// SAFETY: The synchronization is ensured by the Sequentializer
//  1. GenericLazy fullfill the requirement that its sequentializer is a field
//  of itself as is its target data.
//  2. The sequentializer ensure that the initialization is atomic
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
    ///
    /// # Safety
    ///
    /// The parameter M should be a lazy sequentializer that ensure that:
    ///  1. When finalize is called, no other shared reference to the inner data exist
    ///  2. The finalization is run only if the object was previously initialized
    pub const unsafe fn new(generator: F, sequentializer: M, value: T) -> Self {
        Self {
            value,
            generator,
            sequentializer,
            phantom: PhantomData,
            #[cfg(debug_mode)]
            _info: None,
        }
    }
    /// const initialize the lazy, the inner data may be in an uninitialized state, and store
    /// debug information in debug_mode
    ///
    /// # Safety
    ///
    /// The parameter M should be a lazy sequentializer that ensure that:
    ///  1. When finalize is called, no other shared reference to the inner data exist
    ///  2. The finalization is run only if the object was previously initialized
    pub const unsafe fn new_with_info(
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
}
impl<'a,T, F, M, S> GenericLazy<T, F, M, S>
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
    {
    /// Get a reference to the target
    ///
    /// # Safety
    ///
    /// Undefined behaviour if the referenced value has not been initialized
    pub unsafe fn get_unchecked(&'a self) -> &'a T::Target {
        &*self.value.get()
    }

    /// Get a reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn try_get(&'a self) -> Result<&'a T::Target,AccessError> {
        check_access::<*mut T::Target,S>(self.value.get(),Phased::phase(&self.sequentializer)).map(|ptr| unsafe{&*ptr})
    }

    /// Get a reference to the target
    ///
    /// # Panics
    ///
    /// Panic if the target is not in the correct phase
    pub fn get(&'a self) -> &'a T::Target {
        self.try_get().unwrap()
    }

    /// Get a mutable reference to the target
    ///
    /// # Safety
    ///
    /// Undefined behaviour if the referenced value has not been initialized
    pub unsafe fn get_mut_unchecked(&'a mut self) -> &'a mut T::Target {
        &mut *self.value.get()
    }

    /// Get a mutable reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn try_get_mut(&'a self) -> Result<&'a mut T::Target,AccessError> {
        check_access::<*mut T::Target,S>(self.value.get(),Phased::phase(&self.sequentializer)).map(|ptr| unsafe{&mut *ptr})
    }

    /// Get a reference to the target
    ///
    /// # Panics
    ///
    /// Panic if the target is not in the correct phase
    pub fn get_mut(&'a mut self) -> &'a mut T::Target {
        self.try_get_mut().unwrap()
    }

    /// Attempt initialization then get a reference to the target
    ///
    /// # Safety
    ///
    /// Undefined behaviour if the referenced value has not been initialized
    pub unsafe fn init_then_get_unchecked(&'a self) -> &'a T::Target {
        self.init();
        self.get_unchecked()
    }
    /// Attempt initialization then get a reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn init_then_try_get(&'a self) -> Result<&'a T::Target, AccessError> {
        let phase = self.init();
        check_access::<*mut T::Target,S>(self.value.get(), phase).map(|ptr| unsafe{&*ptr})
    }
    /// Attempt initialization then get a reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn init_then_get(&'a self) -> &'a T::Target {
        Self::init_then_try_get(self).unwrap()
    }
    /// Attempt initialization then get a mutable reference to the target
    ///
    /// # Safety
    ///
    /// Undefined behaviour if the referenced value has not been initialized
    pub unsafe fn init_then_get_mut_unchecked(&'a mut self) -> &'a mut T::Target {
        self.init();
        &mut *self.value.get()
    }
    /// Attempt initialization then get a mutable reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn init_then_try_get_mut(&'a mut self) -> Result<&'a mut T::Target, AccessError> {
        let phase = self.init();
        check_access::<* mut T::Target,S>(self.value.get(), phase).map(|ptr| unsafe{&mut *ptr})
    }
    /// Attempt initialization then get a mutable reference to the target, returning an error if the
    /// target is not in the correct phase.
    pub fn init_then_get_mut(&'a mut self) -> &'a mut T::Target {
        Self::init_then_try_get_mut(self).unwrap()
    }
    #[inline(always)]
    /// Potentialy initialize the inner data, returning the 
    /// phase reached at the end of the initialization attempt
    pub fn init(&'a self) -> Phase
    {
        may_debug(|| 
            LazySequentializer::init(
                self,
                S::shall_init,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&self.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            ),
            #[cfg(debug_mode)] &self._info
            )
    }
}

pub struct WriteGuard<T>(T);

impl<T> Deref for WriteGuard<T>
where
    T: Deref,
    <T as Deref>::Target: Deref,
    <<T as Deref>::Target as Deref>::Target: LazyData,
{
    type Target = <<<T as Deref>::Target as Deref>::Target as LazyData>::Target;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(*self.0).get() }
    }
}
impl<T> DerefMut for WriteGuard<T>
where
    T: Deref,
    <T as Deref>::Target: Deref,
    <<T as Deref>::Target as Deref>::Target: LazyData,
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *(*self.0).get() }
    }
}

impl<T> Phased for WriteGuard<T> where T:Phased {
    fn phase(this: &Self) -> Phase {
        Phased::phase(&this.0)
    }
}

pub struct ReadGuard<T>(T);

impl<T> Deref for ReadGuard<T>
where
    T: Deref,
    <T as Deref>::Target: Deref,
    <<T as Deref>::Target as Deref>::Target: LazyData,
{
    type Target = <<<T as Deref>::Target as Deref>::Target as LazyData>::Target;
    fn deref(&self) -> &Self::Target {
        unsafe { &*(*self.0).get() }
    }
}
impl<T,U> From<WriteGuard<T>> for ReadGuard<U>
 where U: From<T>
 {
     fn from(v: WriteGuard<T>) -> Self {
         Self(v.0.into())
     }
 }

impl<T> Phased for ReadGuard<T> where T:Phased {
    fn phase(this: &Self) -> Phase {
        Phased::phase(&this.0)
    }
}



//SAFETY: data and sequentialize are two fields of Self.
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

// SAFETY: The synchronization is ensured by the Sequentializer
//  1. GenericMutLazy fullfill the requirement that its sequentializer is a field
//  of itself as is its target data.
//  2. The sequentializer ensure that the initialization is atomic
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Sync for GenericMutLazy<T, F, M, S> where
    <T as LazyData>::Target: Send
{
}
// SAFETY: The synchronization is ensured by the Sequentializer
unsafe impl<T: LazyData, F: Sync, M: Sync, S> Send for GenericMutLazy<T, F, M, S> where
    <T as LazyData>::Target: Send
{
}

impl<T, F, M, S> GenericMutLazy<T, F, M, S> {
    /// const initialize the lazy, the inner data may be in an uninitialized state
    ///
    /// # Safety
    ///
    /// The parameter M should be a lazy sequentializer that ensure that:
    ///  1. When finalize is called, no other shared reference to the inner data exist
    ///  2. The finalization is run only if the object was previously initialized
    pub const unsafe fn new(generator: F, sequentializer: M, value: T) -> Self {
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
    ///
    /// # Safety
    ///
    /// The parameter M should be a lazy sequentializer that ensure that:
    ///  1. When finalize is called, no other shared reference to the inner data exist
    ///  2. The finalization is run only if the object was previously initialized
    pub const unsafe fn new_with_info(
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
}
impl<'a,T, F, M, S> GenericMutLazy<T, F, M, S> 
    where
        T: 'static + LazyData,
        M: 'static,
        M: LazySequentializer<'a, Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy,
        M::ReadGuard: Phased,
        M::WriteGuard: Phased,
    {
    /// Attempt to get a read lock the LazyData object (not the target), returning None
    /// if a unique lock is already held or in high contention cases.
    ///
    /// # Safety
    ///
    /// The obtained [ReadGuard] may reference an uninitialized target.
    #[inline(always)]
    pub unsafe fn fast_read_lock_unchecked(this: &'a Self) -> Option<ReadGuard<M::ReadGuard>>
    {
        <M as Sequentializer<'a, Self>>::try_lock(this, |_| LockNature::Read).map(|l|
            if let LockResult::Read(l) = l
            {
                ReadGuard(l)
            } else {
                unreachable_unchecked()
            })
    }
    /// Attempt to get a read lock the LazyData object (not the target), returning None
    /// if a unique lock is already held or in high contention cases.
    ///
    /// If the lock succeeds and the object is not in an accessible phase, some error is returned
    #[inline(always)]
    pub fn fast_try_read_lock(this: &'a Self) -> Option<Result<ReadGuard<M::ReadGuard>,AccessError>>
    {
        unsafe{Self::fast_read_lock_unchecked(this)}.map(checked_access::<ReadGuard<M::ReadGuard>,S>)
    }

    /// Attempt to get a read lock the LazyData object (not the target), returning None
    /// if a unique lock is already held or in high contention cases.
    ///
    /// # Panics
    ///
    /// Panics if the lock succeeds and the object is not in an accessible phase.
    #[inline(always)]
    pub fn fast_read_lock(this: &'a Self) -> Option<ReadGuard<M::ReadGuard>> {
        Self::fast_try_read_lock(this).map(|r| r.unwrap())
    }

    /// Get a read lock the LazyData object (not the target)
    ///
    /// # Safety
    ///
    /// The obtained [ReadGuard] may reference an uninitialized target.
    #[inline(always)]
    pub unsafe fn read_lock_unchecked(this: &'a Self) -> ReadGuard<M::ReadGuard>
    {
        if let LockResult::Read(l) =
            <M as Sequentializer<'a, Self>>::lock(this, |_| LockNature::Read)
        {
            ReadGuard(l)
        } else {
            unreachable_unchecked()
        }
    }

    /// Get a read lock the LazyData object (not the target)
    ///
    /// If the object is not in an accessible phase, some error is returned
    #[inline(always)]
    pub fn try_read_lock(this: &'a Self) -> Result<ReadGuard<M::ReadGuard>,AccessError>
    {
        checked_access::<ReadGuard<M::ReadGuard>,S>(unsafe{Self::read_lock_unchecked(this)})
    }

    /// Get a read lock the LazyData object (not the target).
    ///
    /// # Panics
    ///
    /// Panics if the lock succeeds and the object is not in an accessible phase.
    #[inline(always)]
    pub fn read_lock(this: &'a Self) -> ReadGuard<M::ReadGuard>
    {
        Self::try_read_lock(this).unwrap()
    }

    /// Attempt to get a write lock the LazyData object (not the target), returning None
    /// if a lock is already held or in high contention cases.
    ///
    /// # Safety
    ///
    /// The obtained [ReadGuard] may reference an uninitialized target.
    #[inline(always)]
    pub unsafe fn fast_write_lock_unchecked(this: &'a Self) -> Option<WriteGuard<M::WriteGuard>>
    {
       <M as Sequentializer<'a, Self>>::try_lock(this, |_| LockNature::Write).map(|l| 
            if let LockResult::Write(l) = l
            {
                WriteGuard(l)
            } else {
                unreachable_unchecked()
            })
    }

    /// Attempt to get a write lock the LazyData object (not the target), returning None
    /// if a lock is already held or in high contention cases.
    ///
    /// If the lock succeeds and the object is not in an accessible phase, some error is returned
    #[inline(always)]
    pub fn fast_try_write_lock(this: &'a Self) -> Option<Result<WriteGuard<M::WriteGuard>,AccessError>>
    {
        unsafe{Self::fast_write_lock_unchecked(this)}.map(checked_access::<WriteGuard<M::WriteGuard>,S>)
    }

    /// Attempt to get a write lock the LazyData object (not the target), returning None
    /// if a lock is already held or in high contention cases.
    ///
    /// # Panics
    ///
    /// Panics if the lock succeeds and the object is not in an accessible phase.
    #[inline(always)]
    pub fn fast_write_lock(this: &'a Self) -> Option<WriteGuard<M::WriteGuard>> {
        Self::fast_try_write_lock(this).map(|r| r.unwrap())
    }

    /// Get a write lock the LazyData object (not the target)
    ///
    /// # Safety
    ///
    /// The obtained [ReadGuard] may reference an uninitialized target.
    #[inline(always)]
    pub unsafe fn write_lock_unchecked(this: &'a Self) -> WriteGuard<M::WriteGuard>
    {
        if let LockResult::Write(l) = <M as Sequentializer<'a, Self>>::lock(this, |_| LockNature::Write)
        {
            WriteGuard(l)
        } else {
            unreachable_unchecked()
        }
    }

    /// Get a read lock the LazyData object (not the target)
    ///
    /// If the object is not in an accessible phase, an error is returned
    #[inline(always)]
    pub fn try_write_lock(this: &'a Self) -> Result<WriteGuard<M::WriteGuard>,AccessError>
    {
        checked_access::<WriteGuard<M::WriteGuard>,S>(unsafe{Self::write_lock_unchecked(this)})
    }

    /// Get a write lock the LazyData object (not the target).
    ///
    /// # Panics
    ///
    /// Panics if the lock succeeds and the object is not in an accessible phase.
    #[inline(always)]
    pub fn write_lock(this: &'a Self) -> WriteGuard<M::WriteGuard> {
        Self::try_write_lock(this).unwrap()
    }

    #[inline(always)]
    /// Initialize if necessary then return a read lock
    ///
    /// # Safety
    ///
    /// Undefined behaviour if after initialization the return object is not in an accessible
    /// state.
    pub unsafe fn init_then_read_lock_unchecked(this: &'a Self) -> ReadGuard<M::ReadGuard>
    {
       ReadGuard(may_debug(||
            LazySequentializer::init_then_read_guard(
                this,
                S::shall_init,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            ),
            #[cfg(debug_mode)] &this._info
            ))
    }

    /// Initialize if necessary then return a read lock
    ///
    /// Returns an error if after initialization the return object is not in an accessible
    /// state.
    #[inline(always)]
    pub fn init_then_try_read_lock(this: &'a Self) -> Result<ReadGuard<M::ReadGuard>,AccessError>
    {
        checked_access::<ReadGuard<M::ReadGuard>,S>(unsafe {Self::init_then_read_lock_unchecked(this)})
    }

    /// Initialize if necessary then return a read lock
    ///
    /// # Panics
    ///
    /// Panics if after initialization the return object is not in an accessible
    /// state.
    #[inline(always)]
    pub fn init_then_read_lock(this: &'a Self) -> ReadGuard<M::ReadGuard>
    {
        Self::init_then_try_read_lock(this).unwrap()
    }

    /// If necessary attempt to get a write_lock initilialize the object then turn the write
    /// lock into a read lock, otherwise attempt to get directly a read_lock. Attempt to take
    /// a lock may fail because other locks are held or because of contention.
    ///
    /// # Safety
    ///
    /// If the target is not accessible this may cause undefined behaviour.
    #[inline(always)]
    pub unsafe fn fast_init_then_read_lock_unchecked(this: &'a Self) -> Option<ReadGuard<M::ReadGuard>>
    {
        may_debug(||
            LazySequentializer::try_init_then_read_guard(
                this,
                S::shall_init,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            ),
            #[cfg(debug_mode)] &this._info
            ).map(ReadGuard)
    }


    #[inline(always)]
    /// If necessary attempt to get a write_lock initilialize the object then turn the write
    /// lock into a read lock, otherwise attempt to get directly a read_lock. Attempt to take
    /// a lock may fail because other locks are held or because of contention.
    ///
    /// If the target is not accessible some error is returned.
    pub fn fast_init_then_try_read_lock(this: &'a Self) -> Option<Result<ReadGuard<M::ReadGuard>,AccessError>>
    {
        unsafe {Self::fast_init_then_read_lock_unchecked(this)}.map(checked_access::<ReadGuard<M::ReadGuard>,S>)
    }

    #[inline(always)]
    /// If necessary attempt to get a write_lock initilialize the object then turn the write
    /// lock into a read lock, otherwise attempt to get directly a read_lock. Attempt to take
    /// a lock may fail because other locks are held or because of contention.
    ///
    /// # Panics 
    ///
    /// If the target is not accessible some error is returned.
    pub fn fast_init_then_read_lock(this: &'a Self) -> Option<ReadGuard<M::ReadGuard>> {
        Self::fast_init_then_try_read_lock(this).map(|r| r.unwrap())
    }

    #[inline(always)]
    /// Get a write locks, initialize the target if necessary then returns a readlock.
    ///
    /// # Safety
    ///
    /// If the target object is not accessible, this will cause undefined behaviour
    pub unsafe fn init_then_write_lock_unchecked(this: &'a Self) -> WriteGuard<M::WriteGuard>
    {
       WriteGuard(may_debug(|| LazySequentializer::init_then_write_guard(
                this,
                S::shall_init,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            ),
            #[cfg(debug_mode)] &this._info
            ))
    }

    #[inline(always)]
    /// Get a write locks, initialize the target if necessary then returns the write lock.
    ///
    /// If the target object is not accessible an error is returned.
    pub fn init_then_try_write_lock(this: &'a Self) -> Result<WriteGuard<M::WriteGuard>,AccessError>
    {
        checked_access::<WriteGuard<M::WriteGuard>,S>(unsafe{Self::init_then_write_lock_unchecked(this)})
    }
    #[inline(always)]
    /// Get a write locks, initialize the target if necessary then returns a write lock.
    ///
    /// Panics if the target object is not accessible.
    #[inline(always)]
    pub fn init_then_write_lock(this: &'a Self) -> WriteGuard<M::WriteGuard>
    {
        Self::init_then_try_write_lock(this).unwrap()
    }

    #[inline(always)]
    /// Attempt to get a write locks then initialize the target if necessary and returns the 
    /// writelock.
    ///
    /// # Safety
    ///
    /// Undefined behavior if the target object is not accessible.
    pub unsafe fn fast_init_then_write_lock_unchecked(this: &'a Self) -> Option<WriteGuard<M::WriteGuard>>
    {
       may_debug(|| LazySequentializer::try_init_then_write_guard(
                this,
                S::shall_init,
                |data: &T| {
                    // SAFETY
                    // This function is called only once within the init function
                    // Only one thread can ever get this mutable access
                    let d = Generator::generate(&this.generator);
                    unsafe { data.get().write(d) };
                },
                S::INIT_ON_REG_FAILURE,
            ),
            #[cfg(debug_mode)] &this._info
            ).map(WriteGuard)
    }
    /// Attempt to get a write locks then initialize the target if necessary and returns the 
    /// writelock.
    ///
    /// Returns an error if the target object is not accessible.
    #[inline(always)]
    pub fn fast_init_then_try_write_lock(this: &'a Self) -> Option<Result<WriteGuard<M::WriteGuard>,AccessError>>
    {
      Self::fast_init_then_write_lock(this).map(checked_access::<WriteGuard<M::WriteGuard>,S>)
    }
    /// Attempt to get a write locks then initialize the target if necessary and returns the 
    /// writelock.
    ///
    /// # Panics 
    ///
    /// Panics if the target object is not accessible.
    #[inline(always)]
    pub fn fast_init_then_write_lock(this: &'a Self) -> Option<WriteGuard<M::WriteGuard>> {
        Self::fast_init_then_try_write_lock(this).map(|r| r.unwrap())
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

//SAFETY: data and sequentialize are two fields of Self.
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

fn may_debug<R,F:FnOnce()->R>(f:F, #[cfg(debug_mode)] info: &Option<StaticInfo>) -> R{
        #[cfg(not(debug_mode))]
        {
            f()
        }
        #[cfg(debug_mode)]
        {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f())) {
                Ok(r) => r,
                Err(x) => {
                    if x.is::<CyclicPanic>() {
                        match info {
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

fn check_access<T, S: LazyPolicy> (l: T,phase: Phase) -> Result<T,AccessError> {
    if S::is_accessible(phase) {
        Ok(l)
    } else {
        Err(AccessError{phase})
    }
}

fn checked_access<T:Phased, S: LazyPolicy> (l: T) -> Result<T,AccessError> {
    let phase = Phased::phase(&l);
    check_access::<T,S>(l, phase)
}
