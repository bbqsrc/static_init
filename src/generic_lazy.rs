use crate::{Finaly, Generator, Manager, Recoverer, Static, StaticInfo};
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ops::{Deref, DerefMut};

pub struct RegisterOnFirstAccess<T> {
    value: T,
}

impl<T> RegisterOnFirstAccess<T> {
    pub const fn new(value: T) -> Self {
        Self { value }
    }
}

impl<M: Manager<T>, T: Static<Manager = M>> Deref for RegisterOnFirstAccess<T> {
    type Target = T;
    fn deref(&self) -> &T {
        Manager::register(&self.value, |_| true, |_| {});
        &self.value
    }
}

impl<M: Manager<T>, T: Static<Manager = M>> DerefMut for RegisterOnFirstAccess<T> {
    fn deref_mut(&mut self) -> &mut T {
        Manager::register(&self.value, |_| true, |_| {});
        &mut self.value
    }
}

pub struct UnInited<T>(UnsafeCell<MaybeUninit<T>>);

impl<T: Finaly> Finaly for UnInited<T> {
    fn finaly(&self) {
        unsafe { &*(*self.0.get()).as_mut_ptr() }.finaly();
    }
}

pub struct GenericLazy<T, F, G, M> {
    value:     UnInited<T>,
    generator: F,
    recover:   G,
    manager:   M,
    #[cfg(debug_mode)]
    _info:     StaticInfo,
}
unsafe impl<T: Sync, F: Sync, G: Sync, M: Sync> Sync for GenericLazy<T, F, G, M> {}

impl<T, F, G, M> GenericLazy<T, F, G, M> {
    pub const unsafe fn new_static(generator: F, recover: G, manager: M) -> Self {
        Self {
            value: UnInited(UnsafeCell::new(MaybeUninit::uninit())),
            generator,
            recover,
            manager,
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
            #[cfg(debug_mode)]
            _info,
        }
    }
    fn register(&self)
    where
        T: 'static,
        M: 'static + Manager<Self>,
        F: 'static + Generator<T>,
        G: 'static + Recoverer<T>,
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

impl<M: Manager<Self>, T: 'static, F: 'static + Generator<T>, G: 'static + Recoverer<T>> Deref
    for GenericLazy<T, F, G, M>
{
    type Target = T;
    fn deref(&self) -> &T {
        self.register();
        unsafe { &*(*self.value.0.get()).as_ptr() }
    }
}

impl<M: Manager<Self>, T: 'static, F: 'static + Generator<T>, G: 'static + Recoverer<T>> DerefMut
    for GenericLazy<T, F, G, M>
{
    fn deref_mut(&mut self) -> &mut T {
        self.register();
        unsafe { &mut *(*self.value.0.get()).as_mut_ptr() }
    }
}

unsafe impl<F: 'static + Generator<T>, T: 'static, M: 'static, G: 'static + Recoverer<T>> Static
    for GenericLazy<T, F, G, M>
{
    type Data = UnInited<T>;
    type Manager = M;
    fn manager(this: &Self) -> &Self::Manager {
        &this.manager
    }
    fn data(this: &Self) -> &Self::Data {
        &this.value
    }
}
