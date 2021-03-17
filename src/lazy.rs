use crate::{Manager, ManagerBase, Static,OnceManager,GlobalManager,LocalManager,ExitManager,ThreadExitManager, GenericLazy, LazyPolicy, UnInited,DropedUnInited, LazyData, Phase,StaticInfo};

pub struct NonPoisonedChecker;

impl LazyPolicy for NonPoisonedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn shall_proceed(p: Phase) -> bool {
        if p.initialized() {
            false
        } else {
            assert!(!p.initialization_skiped());
            true
        }
    }
}

pub struct NonFinalizedChecker;
impl LazyPolicy for NonFinalizedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn shall_proceed(p: Phase) -> bool {
        if p.initialized() {
            assert!(!p.finalized());
            false
        } else {
            assert!(!p.initialization_skiped());
            true
        }
    }
}

macro_rules! init_only {
    ($typ:ident, $sub:ty) => {
        init_only! {$typ,$sub,<$sub>::new()}
    };

    ($typ:ident, $sub:ty, $init:expr) => {
        pub struct $typ($sub);

        impl $typ {
            pub const fn new() -> Self {
                Self($init)
            }
        }

        impl AsRef<$sub> for $typ {
            fn as_ref(&self) -> &$sub {
                &self.0
            }
        }

        impl ManagerBase for $typ {
            fn phase(&self) -> Phase {
                self.0.phase()
            }
        }

        unsafe impl<T: Static<Manager = Self>> Manager<T> for $typ {
            fn register(
                s: &T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&<T as Static>::Data),
                init_on_reg_failure: bool,
            ) -> bool {
                <$sub as OnceManager<T>>::register(s, on_uninited, init, |_| true, init_on_reg_failure)
            }
        }
    };
}

init_only! {GlobalInitOnlyManager,GlobalManager<true>}

init_only! {LazyInitOnlyManager,GlobalManager<false>, GlobalManager::new_lazy()}

init_only! {LocalInitOnlyManager,LocalManager}

use core::ops::{Deref, DerefMut};
macro_rules! impl_lazy {
    ($tp:ident, $man:ty, $checker:ty, $data:ty) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()}
    };
    (unsafe $tp:ident, $man:ty, $checker:ty, $data:ty) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new(), unsafe}
    };
    ($tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,$init}
    };
    (unsafe $tp:ident, $man:ty, $checker:ty, $data:ty,$init:expr) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,$init, unsafe}
    };
    (@proc $tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr $(,$safe:ident)?) => {
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericLazy<$data, G, $man, $checker>,
        }

        impl<T, G> Deref for $tp<T, G>
        where
            GenericLazy<$data, G, $man, $checker>: Deref,
        {
            type Target = <GenericLazy<$data, G, $man, $checker> as Deref>::Target;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                &*self.__private
            }
        }

        impl<T, G> DerefMut for $tp<T, G>
        where
            GenericLazy<$data, G, $man, $checker>: DerefMut,
        {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut *self.__private
            }
        }
        impl<T, G> $tp<T, G> {
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    
                    __private: unsafe{GenericLazy::new_static(f, $init,<$data>::INIT)},
                }
            }
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLazy::new_static_with_info(f, $init, <$data>::INIT,info)},
                }
            }
        }

        impl<T, G> $tp<T, G>
        where
            GenericLazy<$data, G, $man, $checker>: Deref + Static,
            <GenericLazy<$data, G, $man, $checker> as Static>::Manager: ManagerBase,
        {
            #[inline(always)]
            pub fn phase(&self) -> Phase {
                Static::manager(&self.__private).phase()
            }
            #[inline(always)]
            pub fn register(&self) {
                &*self.__private;
            }
        }
    };
}
impl_lazy! {Lazy,LazyInitOnlyManager,NonPoisonedChecker,UnInited::<T>}
impl_lazy! {unsafe QuasiLazy,GlobalInitOnlyManager,NonPoisonedChecker,UnInited::<T>}
impl_lazy! {unsafe LazyFinalize,ExitManager<false>,NonPoisonedChecker,UnInited::<T>,ExitManager::new_lazy()}
impl_lazy! {unsafe QuasiLazyFinalize,ExitManager<true>,NonPoisonedChecker,UnInited::<T>}
impl_lazy! {LocalLazy,LocalInitOnlyManager,NonPoisonedChecker,UnInited::<T>}
impl_lazy! {unsafe LocalLazyFinalize,ThreadExitManager,NonPoisonedChecker,UnInited::<T>}
impl_lazy! {unsafe LocalLazyDroped,ThreadExitManager,NonFinalizedChecker,DropedUnInited::<T>}

impl<T,G> Drop for Lazy<T,G> { 
    fn drop(&mut self) {
        if self.__private.manager().phase().initialized() {
           unsafe {self.__private.get_raw_data().get().drop_in_place()}
        }
    }
}
impl<T,G> Drop for LocalLazy<T,G> { 
    fn drop(&mut self) {
        if self.__private.manager().phase().initialized() {
           unsafe {self.__private.get_raw_data().get().drop_in_place() }
        }
    }
}

#[cfg(test)]
mod test_lazy {
    use super::Lazy;
    static _X: Lazy<u32, fn() -> u32> = Lazy::new_static(|| 22);
    #[test]
    fn test() {
        _X.register();
        assert_eq!(*_X, 22);
    }
}

//#[cfg(test)]
//mod test_quasi_lazy {
//    use super::QuasiLazy;
//    static _X: QuasiLazy<u32, fn() -> u32> = unsafe {
//        QuasiLazy::new_static(|| {
//            22
//        })
//    };
//    #[test]
//    fn test() {
//        assert_eq!(*_X, 22);
//    }
//}
#[cfg(test)]
mod test_local_lazy {
    use super::LocalLazy;
    #[thread_local]
    static _X: LocalLazy<u32, fn() -> u32> =  LocalLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}
#[cfg(test)]
mod test_lazy_finalize {
    use crate::Finaly;
    use super::LazyFinalize;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: LazyFinalize<A, fn() -> A> = unsafe{LazyFinalize::new_static(|| A(22))};
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
//#[cfg(test)]
//mod test_quasi_lazy_finalize {
//    use crate::Finaly;
//    use super::QuasiLazyFinalize;
//    #[derive(Debug)]
//    struct A(u32);
//    impl Finaly for A {
//        fn finaly(&self) {}
//    }
//    static _X: QuasiLazyFinalize<A, fn() -> A> =
//        unsafe { QuasiLazyFinalize::new_static(|| A(22)) };
//    #[test]
//    fn test() {
//        assert_eq!((*_X).0, 22);
//    }
//}
#[cfg(test)]
mod test_local_lazy_finalize {
    use crate::Finaly;
    use super::LocalLazyFinalize;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: LocalLazyFinalize<A, fn() -> A> = unsafe { LocalLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
#[cfg(test)]
mod test_droped_local_lazy_finalize {
    use super::LocalLazyDroped;
    #[derive(Debug)]
    struct A(u32);
    #[thread_local]
    static _X: LocalLazyDroped<A> = unsafe { LocalLazyDroped::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!(_X.0, 22);
    }
}
