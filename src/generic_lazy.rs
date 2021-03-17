//use crate::{AtThreadExit, LocalManager};
use crate::{Finaly, Generator, Manager, Static, StaticInfo, Phase};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

#[cfg(debug_mode)]
use crate::CyclicPanic;

pub trait LazyPolicy {
    const INIT_ON_REG_FAILURE:bool;
    fn shall_proceed(_:Phase) -> bool;
}

pub struct RegisterOnFirstAccess<T,S> {
    value: T,
    phantom: PhantomData<S>,
}

impl<T,S> RegisterOnFirstAccess<T,S> {
    pub const fn new(value: T) -> Self {
        Self { value , phantom:PhantomData}
    }
}

impl<M: Manager<T>, T: Static<Manager = M>, S:LazyPolicy> Deref for RegisterOnFirstAccess<T,S> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        Manager::register(&self.value, S::shall_proceed, |_| (),true);
        &self.value
    }
}

impl<M: Manager<T>, T: Static<Manager = M>, S:LazyPolicy> DerefMut for RegisterOnFirstAccess<T,S> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        Manager::register(&self.value, S::shall_proceed, |_| (),true);
        &mut self.value
    }
}

pub struct UnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T: Finaly> Finaly for UnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        unsafe { &*self.get() }.finaly();
    }
}

pub struct DropedUnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T> Finaly for DropedUnInited<T> {
    #[inline(always)]
    fn finaly(&self) {
        unsafe { self.get().drop_in_place()};
    }
}

pub trait LazyData {
    type Target;
    const INIT: Self;
    fn get(&self) -> *mut Self::Target;
}

unsafe impl<T:Sync> Sync for UnInited<T> {}
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


pub struct GenericLazy<T, F, M, S> {
    value:     T,
    generator: F,
    manager:   M,
    phantom: PhantomData<S>,
    #[cfg(debug_mode)]
    _info:     Option<StaticInfo>,
}
unsafe impl<T: Sync, F: Sync, M: Sync, S> Sync for GenericLazy<T, F, M,S> {}

impl<T, F, M,S> GenericLazy<T, F, M,S> {
    pub const unsafe fn new_static(generator: F, manager: M, value: T) -> Self {
        Self {
            value,
            generator,
            manager,
            phantom:PhantomData,
            #[cfg(debug_mode)]
            _info: None
        }
    }
    pub const unsafe fn new_static_with_info(
        generator: F,
        manager: M,
        value: T,
        _info: StaticInfo,
    ) -> Self {
        Self {
            value,
            generator,
            manager,
            phantom:PhantomData,
            #[cfg(debug_mode)]
            _info:Some(_info),
        }
    }
    #[inline(always)]
    pub fn register(&self)
    where
        T: 'static + LazyData,
        M: 'static + Manager<Self>,
        F: 'static + Generator<T::Target>,
        S: 'static + LazyPolicy
    {
        #[cfg(not(debug_mode))]
        {
        Manager::register(
            self,
            S::shall_proceed,
            |data: &T| {
                // SAFETY
                // This function is called only once within the register function
                // Only one thread can ever get this mutable access
                let d = Generator::generate(&self.generator);
                unsafe { data.get().write(d) };
            },
            S::INIT_ON_REG_FAILURE,
        );
        }
        #[cfg(debug_mode)]
        {
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| 
        Manager::register(
            self,
            S::shall_proceed,
            |data: &T| {
                // SAFETY
                // This function is called only once within the register function
                // Only one thread can ever get this mutable access
                let d = Generator::generate(&self.generator);
                unsafe { data.get().write(d) };
            },
            S::INIT_ON_REG_FAILURE,
        )
        )) {
            Ok(_) => (),
            Err(x) => if x.is::<CyclicPanic>() { 
                match &self._info {
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

impl<M: 'static+Manager<Self>, T: 'static + LazyData, F: 'static + Generator<T::Target>, S:'static+LazyPolicy> Deref
    for GenericLazy<T, F, M, S>
{
    type Target = T::Target;
    #[inline(always)]
    fn deref(&self) -> &T::Target {
        self.register();
        // SAFETY
        // This is safe as long as the object has been initialized
        // this is the contract ensured by register.
        unsafe { &*self.value.get() }
    }
}

impl<M: 'static+Manager<Self>, T: 'static + LazyData, F: 'static + Generator<T::Target>, S:'static+LazyPolicy> DerefMut
    for GenericLazy<T, F, M,S>
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T::Target {
        self.register();
        unsafe { &mut *self.value.get() }
    }
}

unsafe impl<F: 'static + Generator<T::Target>, T: 'static + LazyData, M: 'static, S:'static+LazyPolicy> Static
    for GenericLazy<T, F, M, S>
{
    type Data = T;
    type Manager = M;
    #[inline(always)]
    fn manager(this: &Self) -> &Self::Manager {
        &this.manager
    }
    #[inline(always)]
    fn data(this: &Self) -> &Self::Data {
        &this.value
    }
}

