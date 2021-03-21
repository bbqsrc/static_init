use crate::{Sequentializer, Phased, Sequential,LazySequentializer, SplitedLazySequentializer,splited_sequentializer::UnSyncSequentializer, generic_lazy::{GenericLazy, LazyPolicy, UnInited, LazyData}, Phase,StaticInfo};
use crate::mutex::{SyncPhaseGuard,UnSyncPhaseGuard};

#[cfg(feature="thread_local")]
use crate::{at_exit::ThreadExitSequentializer,generic_lazy::DropedUnInited};

#[cfg(feature="global_once")]
use crate::{splited_sequentializer::SyncSequentializer,at_exit::ExitSequentializer};

pub struct InitializedChecker;

impl LazyPolicy for InitializedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn shall_proceed(p: Phase) -> bool {
        if p.intersects(Phase::INITIALIZED) {
            false
        } else {
            assert!(!p.intersects(Phase::INITIALIZATION_SKIPED));
            true
        }
    }
}

pub struct InitializedAndNonFinalizedChecker;
impl LazyPolicy for InitializedAndNonFinalizedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn shall_proceed(p: Phase) -> bool {
        if p.intersects(Phase::INITIALIZED) {
            assert!(!p.intersects(Phase::FINALIZED));
            false
        } else {
            assert!(!p.intersects(Phase::INITIALIZATION_SKIPED));
            true
        }
    }
}

macro_rules! init_only {
    ($typ:ident, $sub:ty, $gd:ident) => {
        init_only! {$typ,$sub,$gd,<$sub>::new()}
    };

    ($typ:ident, $sub:ty, $gd:ident, $init:expr) => {
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

        impl Phased for $typ {
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.0)
            }
        }

        impl<'a,T: 'a+Sequential<Sequentializer = Self>> Sequentializer<'a,T> for $typ {
            type Guard = Option<$gd<'a,T>>;
            fn lock(
                s: &'a T,
                shall_proceed: impl Fn(Phase) -> bool,
            ) -> Self::Guard {
                <$sub as Sequentializer<T>>::lock(s, shall_proceed)
            }
        }

        impl<'a,T: 'a+Sequential<Sequentializer = Self>> LazySequentializer<'a,T> for $typ {
            fn init(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&<T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Self::Guard {
                <$sub as SplitedLazySequentializer<T>>::init(s, on_uninited, init, |_| true, init_on_reg_failure)
            }
        }
    };
}

#[cfg(feature="global_once")]
init_only! {StartUpInitedNonFinalizedSyncSequentializer,SyncSequentializer<true>, SyncPhaseGuard}

#[cfg(feature="global_once")]
init_only! {NonFinalizedSyncSequentializer,SyncSequentializer<false>, SyncPhaseGuard, SyncSequentializer::new_lazy()}

init_only! {NonFinalizedUnSyncSequentializer,UnSyncSequentializer, UnSyncPhaseGuard}

use core::ops::{Deref, DerefMut};
macro_rules! impl_lazy {
    ($tp:ident, $man:ty, $checker:ty, $data:ty, $doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new(),$doc $(cfg($attr))?}
    };
    (unsafe $tp:ident, $man:ty, $checker:ty, $data:ty,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new(),$doc $(cfg($attr))?, unsafe}
    };
    ($tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,$init,$doc $(cfg($attr))?}
    };
    (unsafe $tp:ident, $man:ty, $checker:ty, $data:ty,$init:expr,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,$init, $doc $(cfg($attr))?,unsafe}
    };
    (@proc $tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr,$doc:literal $(cfg($attr:meta))? $(,$safe:ident)?) => {
        #[doc=$doc]
        $(#[cfg_attr(docsrs,doc(cfg($attr)))])?
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericLazy<$data, G, $man, $checker>,
        }
        impl<T, G> Phased for $tp<T, G>
        where
            GenericLazy<$data, G, $man, $checker>: Phased,
        {
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
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
            GenericLazy<$data, G, $man, $checker>: Deref + Sequential,
            <GenericLazy<$data, G, $man, $checker> as Sequential>::Sequentializer: Phased,
        {
            //TODO: method => associated function
            #[inline(always)]
            pub fn phase(&self) -> Phase {
                Phased::phase(Sequential::sequentializer(&self.__private))
            }
            #[inline(always)]
            pub fn register(&self) {
                &*self.__private;
            }
        }
    };
}

#[cfg(feature="global_once")]
impl_lazy! {Lazy,NonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>,
"A type that initialize it self only once on the first access" cfg(feature="global_once")}

#[cfg(feature="global_once")]
impl_lazy! {unsafe QuasiLazy,StartUpInitedNonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>,
"The actual type of statics attributed with #[dynamic(quasi_lazy)]" cfg(feature="global_once")
}

#[cfg(feature="global_once")]
impl_lazy! {unsafe LazyFinalize,ExitSequentializer<false>,InitializedChecker,UnInited::<T>,ExitSequentializer::new_lazy(),
"The actual type of statics attributed with #[dynamic(lazy,finalize)]" cfg(feature="global_once")
}

#[cfg(feature="global_once")]
impl_lazy! {unsafe QuasiLazyFinalize,ExitSequentializer<true>,InitializedChecker,UnInited::<T>,
"The actual type of statics attributed with #[dynamic(quasi_lazy,finalize)]" cfg(feature="global_once")
}

impl_lazy! {UnSyncLazy,NonFinalizedUnSyncSequentializer,InitializedChecker,UnInited::<T>,
"A version of [Lazy] whose reference can not be passed to other thread"
}

#[cfg(feature="thread_local")]
impl_lazy! {unsafe UnSyncLazyFinalize,ThreadExitSequentializer,InitializedChecker,UnInited::<T>,
"The actual type of thread_local statics attributed with #[dynamic(lazy,finalize)]" cfg(feature="thread_local")
}
#[cfg(feature="thread_local")]
impl_lazy! {unsafe UnSyncLazyDroped,ThreadExitSequentializer,InitializedAndNonFinalizedChecker,DropedUnInited::<T>,
"The actual type of thread_local statics attributed with #[dynamic(lazy,drop)]" cfg(feature="thread_local")
}

#[cfg(feature="global_once")]
impl<T,G> Drop for Lazy<T,G> { 
    fn drop(&mut self) {
        if Phased::phase(GenericLazy::sequentializer(&self.__private)).intersects(Phase::INITIALIZED) {
           unsafe {GenericLazy::get_raw_data(&self.__private).get().drop_in_place()}
        }
    }
}
impl<T,G> Drop for UnSyncLazy<T,G> { 
    fn drop(&mut self) {
        if Phased::phase(GenericLazy::sequentializer(&self.__private)).intersects(Phase::INITIALIZED) {
           unsafe {GenericLazy::get_raw_data(&self.__private).get().drop_in_place() }
        }
    }
}

#[cfg(feature="global_once")]
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

#[cfg(feature="global_once")]
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
#[cfg(all(test,feature="thread_local"))]
mod test_local_lazy {
    use super::UnSyncLazy;
    #[thread_local]
    static _X: UnSyncLazy<u32, fn() -> u32> =  UnSyncLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}
#[cfg(feature="global_once")]
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
#[cfg(feature="global_once")]
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
#[cfg(all(test,feature="thread_local"))]
mod test_local_lazy_finalize {
    use crate::Finaly;
    use super::UnSyncLazyFinalize;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: UnSyncLazyFinalize<A, fn() -> A> = unsafe { UnSyncLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
#[cfg(all(test,feature="thread_local"))]
mod test_droped_local_lazy_finalize {
    use super::UnSyncLazyDroped;
    #[derive(Debug)]
    struct A(u32);
    #[thread_local]
    static _X: UnSyncLazyDroped<A> = unsafe { UnSyncLazyDroped::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!(_X.0, 22);
    }
}
