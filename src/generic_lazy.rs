//use crate::{AtThreadExit, LocalManager};
use crate::{Finaly, Generator, Manager, Recoverer, Static, StaticInfo, Phase};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};
use core::marker::PhantomData;

pub trait PhaseChecker {
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

impl<M: Manager<T>, T: Static<Manager = M>, S:PhaseChecker> Deref for RegisterOnFirstAccess<T,S> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        Manager::register(&self.value, S::shall_proceed, |_| true, |_| {});
        &self.value
    }
}

impl<M: Manager<T>, T: Static<Manager = M>, S:PhaseChecker> DerefMut for RegisterOnFirstAccess<T,S> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        Manager::register(&self.value, S::shall_proceed, |_| true, |_| {});
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

pub struct GenericLazy<T, F, G, M, S> {
    value:     UnInited<T>,
    generator: F,
    recover:   G,
    manager:   M,
    phantom: PhantomData<S>,
    #[cfg(debug_mode)]
    _info:     StaticInfo,
}
unsafe impl<T: Sync, F: Sync, G: Sync, M: Sync, S> Sync for GenericLazy<T, F, G, M,S> {}

impl<T, F, G, M,S> GenericLazy<T, F, G, M,S> {
    pub const unsafe fn new_static(generator: F, recover: G, manager: M) -> Self {
        Self {
            value: UnInited(UnsafeCell::new(MaybeUninit::uninit())),
            generator,
            recover,
            manager,
            phantom:PhantomData,
        }
    }
    pub const unsafe fn new_static_with_info(
        generator: F,
        recover: G,
        manager: M,
        _info: StaticInfo,
    ) -> Self {
        Self {
            value: UnInited(UnsafeCell::new(MaybeUninit::uninit())),
            generator,
            recover,
            manager,
            phantom:PhantomData,
            #[cfg(debug_mode)]
            _info,
        }
    }
    #[inline(always)]
    pub fn register(&self)
    where
        T: 'static,
        M: 'static + Manager<Self>,
        F: 'static + Generator<T>,
        G: 'static + Recoverer<T>,
        S: 'static + PhaseChecker
    {
        #[cfg(debug_mode)]
        match Static::phase(self) {
            Phase::Initialization | Phase::FinalyRegistration => {
                panic!("Circular lazy initialization of {:#?}", self._info)
            }
            _ => (),
        }
        Manager::register(
            self,
            S::shall_proceed,
            |data: &UnInited<T>| {
                // SAFETY
                // This function is called only once within the register function
                // Only one thread can ever get this mutable access
                let d = Generator::generate(&self.generator);
                unsafe { (*data.0.get()).as_mut_ptr().write(d) };
                true
            },
            |data| Recoverer::recover(&self.recover, unsafe { &*(*data.0.get()).as_ptr() }),
        );
    }
}

impl<M: 'static+Manager<Self>, T: 'static, F: 'static + Generator<T>, G: 'static + Recoverer<T>,S:'static+PhaseChecker> Deref
    for GenericLazy<T, F, G, M, S>
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

impl<M: 'static+Manager<Self>, T: 'static, F: 'static + Generator<T>, G: 'static + Recoverer<T>,S:'static+PhaseChecker> DerefMut
    for GenericLazy<T, F, G, M,S>
{
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        self.register();
        unsafe { &mut *(*self.value.0.get()).as_mut_ptr() }
    }
}

unsafe impl<F: 'static + Generator<T>, T: 'static, M: 'static, G: 'static + Recoverer<T>, S:'static+PhaseChecker> Static
    for GenericLazy<T, F, G, M, S>
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

