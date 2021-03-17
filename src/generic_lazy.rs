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
        unsafe { &*(*self.0.get()).as_mut_ptr() }.finaly();
    }
}

pub struct GenericLazy<T, F, M, S> {
    value:     UnInited<T>,
    generator: F,
    manager:   M,
    phantom: PhantomData<S>,
    #[cfg(debug_mode)]
    _info:     Option<StaticInfo>,
}
unsafe impl<T: Sync, F: Sync, M: Sync, S> Sync for GenericLazy<T, F, M,S> {}

impl<T, F, M,S> GenericLazy<T, F, M,S> {
    pub const unsafe fn new_static(generator: F, manager: M) -> Self {
        Self {
            value: UnInited(UnsafeCell::new(MaybeUninit::uninit())),
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
        _info: StaticInfo,
    ) -> Self {
        Self {
            value: UnInited(UnsafeCell::new(MaybeUninit::uninit())),
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
        T: 'static,
        M: 'static + Manager<Self>,
        F: 'static + Generator<T>,
        S: 'static + LazyPolicy
    {
        #[cfg(not(debug_mode))]
        {
        Manager::register(
            self,
            S::shall_proceed,
            |data: &UnInited<T>| {
                // SAFETY
                // This function is called only once within the register function
                // Only one thread can ever get this mutable access
                let d = Generator::generate(&self.generator);
                unsafe { (*data.0.get()).as_mut_ptr().write(d) };
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
            |data: &UnInited<T>| {
                // SAFETY
                // This function is called only once within the register function
                // Only one thread can ever get this mutable access
                let d = Generator::generate(&self.generator);
                unsafe { (*data.0.get()).as_mut_ptr().write(d) };
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

impl<M: 'static+Manager<Self>, T: 'static, F: 'static + Generator<T>, S:'static+LazyPolicy> Deref
    for GenericLazy<T, F, M, S>
{
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        self.register();
        // SAFETY
        // This is safe as long as the object has been initialized
        // this is the contract ensured by register.
        unsafe { &*(*self.value.0.get()).as_ptr() }
    }
}

impl<M: 'static+Manager<Self>, T: 'static, F: 'static + Generator<T>, S:'static+LazyPolicy> DerefMut
    for GenericLazy<T, F, M,S>
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.register();
        unsafe { &mut *(*self.value.0.get()).as_mut_ptr() }
    }
}

unsafe impl<F: 'static + Generator<T>, T: 'static, M: 'static, S:'static+LazyPolicy> Static
    for GenericLazy<T, F, M, S>
{
    type Data = UnInited<T>;
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

