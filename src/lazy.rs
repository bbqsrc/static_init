use crate::mutex::{
     SyncPhaseGuard, SyncReadPhaseGuard, UnSyncPhaseGuard, UnSyncReadPhaseGuard,
};
use crate::{
    generic_lazy::{DropedUnInited, GenericLazy, GenericMutLazy, LazyData, LazyPolicy, UnInited, AccessError,ReadGuard,WriteGuard},
    mutex::{LockNature, LockResult},
    splited_sequentializer::UnSyncSequentializer,
    Finaly, Generator, LazySequentializer, Phase, Phased, Sequential, Sequentializer,
    SplitedLazySequentializer, StaticInfo,
};

#[cfg(feature = "thread_local")]
use crate::at_exit::ThreadExitSequentializer;

use crate::{at_exit::ExitSequentializer, splited_sequentializer::SyncSequentializer};

use core::ops::{Deref, DerefMut};

pub struct InitializedChecker;

impl LazyPolicy for InitializedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn is_accessible(p: Phase) -> bool {
        p.intersects(Phase::INITIALIZED)
    }
    #[inline(always)]
    fn shall_init(p: Phase) -> bool {
        p.is_empty()
    }
}

pub struct InitializedAndNonFinalizedChecker;
impl LazyPolicy for InitializedAndNonFinalizedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn shall_init(p: Phase) -> bool {
        p.is_empty()
    }
    #[inline(always)]
    fn is_accessible(p: Phase) -> bool {
        !p.intersects(Phase::FINALIZED) && p.intersects(Phase::INITIALIZED)
    }
}
//pub struct InitializedRearmingChecker;
//impl LazyPolicy for InitializedRearmingChecker {
//    const INIT_ON_REG_FAILURE: bool = false;
//    #[inline(always)]
//    fn lock_nature(p: Phase) -> bool {
//        if p.intersects(Phase::INITIALIZED) && !p.intersects(Phase::FINALIZED) {
//            false
//        } else {
//            assert!(!p.intersects(Phase::INITIALIZATION_SKIPED));
//            true
//        }
//    }
//}

macro_rules! init_only {
    ($typ:ident, $sub:ty, $gd:ident, $gd_r:ident) => {
        init_only! {$typ,$sub,$gd,$gd_r,<$sub>::new()}
    };

    ($typ:ident, $sub:ty, $gd:ident, $gd_r:ident, $init:expr) => {
        pub struct $typ($sub);

        impl $typ {
            #[inline(always)]
            pub const fn new() -> Self {
                Self($init)
            }
        }

        impl AsRef<$sub> for $typ {
            #[inline(always)]
            fn as_ref(&self) -> &$sub {
                &self.0
            }
        }

        impl Phased for $typ {
            #[inline(always)]
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.0)
            }
        }

        // SAFETY: ensured by $sub
        unsafe impl<'a, T: 'a + Sequential<Sequentializer = Self>> Sequentializer<'a, T> for $typ {
            type ReadGuard = $gd_r<'a, T>;
            type WriteGuard = $gd<'a, T>;
            #[inline(always)]
            fn lock(
                s: &'a T,
                lock_nature: impl Fn(Phase) -> LockNature,
            ) -> LockResult<$gd_r<'a, T>, $gd<'a, T>> {
                <$sub as Sequentializer<T>>::lock(s, lock_nature)
            }
            #[inline(always)]
            fn try_lock(
                s: &'a T,
                lock_nature: impl Fn(Phase) -> LockNature,
            ) -> Option<LockResult<$gd_r<'a, T>, $gd<'a, T>>> {
                <$sub as Sequentializer<T>>::try_lock(s, lock_nature)
            }
        }
        // SAFETY: ensured by $sub
        unsafe impl<'a, T: 'a + Sequential<Sequentializer = Self>> LazySequentializer<'a, T>
            for $typ
        {
            #[inline(always)]
            fn init(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Phase {
                <$sub as SplitedLazySequentializer<T>>::init(
                    s,
                    on_uninited,
                    init,
                    |_| true,
                    init_on_reg_failure,
                )
            }
            #[inline(always)]
            fn init_then_read_guard(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Self::ReadGuard {
                <$sub as SplitedLazySequentializer<T>>::init_then_read_guard(
                    s,
                    on_uninited,
                    init,
                    |_| true,
                    init_on_reg_failure,
                )
            }
            #[inline(always)]
            fn init_then_write_guard(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Self::WriteGuard {
                <$sub as SplitedLazySequentializer<T>>::init_then_write_guard(
                    s,
                    on_uninited,
                    init,
                    |_| true,
                    init_on_reg_failure,
                )
            }
            #[inline(always)]
            fn try_init_then_read_guard(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Option<Self::ReadGuard> {
                <$sub as SplitedLazySequentializer<T>>::try_init_then_read_guard(
                    s,
                    on_uninited,
                    init,
                    |_| true,
                    init_on_reg_failure,
                )
            }
            #[inline(always)]
            fn try_init_then_write_guard(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) -> Option<Self::WriteGuard> {
                <$sub as SplitedLazySequentializer<T>>::try_init_then_write_guard(
                    s,
                    on_uninited,
                    init,
                    |_| true,
                    init_on_reg_failure,
                )
            }
        }
    };
}
init_only! {StartUpInitedNonFinalizedSyncSequentializer,SyncSequentializer, SyncPhaseGuard, SyncReadPhaseGuard}

init_only! {NonFinalizedSyncSequentializer,SyncSequentializer, SyncPhaseGuard, SyncReadPhaseGuard}

init_only! {NonFinalizedUnSyncSequentializer,UnSyncSequentializer, UnSyncPhaseGuard, UnSyncReadPhaseGuard}

macro_rules! impl_lazy {
    ($tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?}
        impl_lazy! {@deref $tp,$man,$checker,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (global $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe,'static}
        impl_lazy! {@deref_global $tp,$man,$checker,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (static $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe,'static}
        impl_lazy! {@deref_static $tp,$man,$checker,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (@deref $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            pub fn get(this: &Self) -> &T {
                this.__private.init_then_get()
            }
            #[inline(always)]
            pub fn try_get(this: &Self) -> Result<&'_ T,AccessError> {
                this.__private.try_get()
            }
            #[inline(always)]
            pub fn get_mut(this: &mut Self) -> &mut T {
                this.__private.init_then_get_mut()
            }
            #[inline(always)]
            pub fn try_get_mut(this: &mut Self) -> Result<&'_ mut T,AccessError> {
                this.__private.try_get_mut()
            }
        }
        impl<T, G> Deref for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            type Target = T;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                Self::get(self)
            }
        }

        impl<T, G> DerefMut for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                Self::get_mut(self)
            }
        }
    };
    (@deref_static $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            pub fn get(this: &'static Self) -> &'static T {
                 // SAFETY The object is required to have 'static lifetime by construction
                 this.__private.init_then_get()
            }
            #[inline(always)]
            pub fn try_get(this: &'static Self) -> Result<&'static T,AccessError> {
                 // SAFETY The object is required to have 'static lifetime by construction
                 this.__private.init_then_try_get()
            }
        }
        impl<T, G> Deref for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            type Target = T;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                 // SAFETY The object is required to have 'static lifetime by construction
                 Self::get(unsafe{as_static(self)})
            }
        }

    };
    (@deref_global $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            pub fn try_get(this: &'static Self) -> Result<&'static T, AccessError> {
                if inited::global_inited_hint() {
                    // SAFETY The object is initialized a program start-up as long
                    // as it is constructed through the macros #[dynamic(quasi_lazy)]
                    // If initialization failed, the program terminates before the
                    // global_inited_hint is set. So if the global_initied_hint is
                    // set all QuasiLazy are guaranteed to be initialized
                    // TODO: get_unchecked
                    Ok(unsafe{this.__private.get_unchecked()})
                } else {
                    this.__private.init_then_try_get()
                }
            }
            #[inline(always)]
            pub fn get(this: &'static Self) -> &'static T {
                if inited::global_inited_hint() {
                    // SAFETY The object is initialized a program start-up as long
                    // as it is constructed through the macros #[dynamic(quasi_lazy)]
                    // If initialization failed, the program terminates before the
                    // global_inited_hint is set. So if the global_initied_hint is
                    // set all QuasiLazy are guaranteed to be initialized
                    unsafe{this.__private.get_unchecked()}
                } else {
                    this.__private.init_then_get()
                }
            }
        }
        impl<T, G> Deref for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            type Target = T;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                // SAFETY The object is initialized a program start-up as long
                // as it is constructed through the macros #[dynamic(quasi_lazy)]
                // If initialization failed, the program terminates before the
                // global_inited_hint is set. So if the global_initied_hint is
                // set all QuasiLazy are guaranteed to be initialized
                Self::get(unsafe{as_static(self)})
            }
        }

    };
    (@proc $tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))? $(,$safe:ident)?$(,$static:lifetime)?) => {
        #[doc=$doc]
        $(#[cfg_attr(docsrs,doc(cfg($attr)))])?
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericLazy<$data, G, $man, $checker>,
        }
        impl<T, G> Phased for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
        }

        impl<T, G> $tp<T, G> {
            /// Build a new static object
            ///
            /// # Safety
            /// 
            /// This function may be unsafe if building any thing else than a thread local object
            /// or a static will be the cause of undefined behavior
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericLazy::new(f, <$man>::new(),<$data>::INIT)},
                }
            }
            /// Build a new static object with debug information
            ///
            /// # Safety
            /// 
            /// This function may be unsafe if building any thing else than a thread local object
            /// or a static will be the cause of undefined behavior
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLazy::new_with_info(f, <$man>::new(), <$data>::INIT,info)},
                }
            }
        }

        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            pub fn phase(this: &Self) -> Phase {
                Phased::phase(Sequential::sequentializer(&this.__private))
            }
            #[inline(always)]
            pub fn init(this: &$($static)? Self) -> Phase {
                GenericLazy::init(&this.__private)
            }
        }
    };
}

impl_lazy! {Lazy,NonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>,
"A type that initialize it self only once on the first access"}

impl_lazy! {global QuasiLazy,StartUpInitedNonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>,
"The actual type of statics attributed with #[dynamic(quasi_lazy)]. \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_lazy! {static LazyFinalize,ExitSequentializer,InitializedChecker,UnInited::<T>,T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static."
}

impl_lazy! {global QuasiLazyFinalize,ExitSequentializer,InitializedChecker,UnInited::<T>,T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_lazy,finalize)]. \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_lazy! {UnSyncLazy,NonFinalizedUnSyncSequentializer,InitializedChecker,UnInited::<T>,
"A version of [Lazy] whose reference can not be passed to other thread"
}

#[cfg(feature = "thread_local")]
impl_lazy! {static UnSyncLazyFinalize,ThreadExitSequentializer,InitializedChecker,UnInited::<T>,T:Finaly,
"The actual type of thread_local statics attributed with #[dynamic(lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static." cfg(feature="thread_local")
}
#[cfg(feature = "thread_local")]
impl_lazy! {static UnSyncLazyDroped,ThreadExitSequentializer,InitializedAndNonFinalizedChecker,DropedUnInited::<T>,
"The actual type of thread_local statics attributed with #[dynamic(lazy,drop)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static." cfg(feature="thread_local")
}

use core::fmt::{self, Debug, Formatter};
macro_rules! non_static_debug {
    ($tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T:Debug, G> Debug for $tp<T, G>
            where $data: 'static + LazyData<Target=T>,
            G: 'static + Generator<T>,
            $(G:$trg, T:Sync,)?
            $(T:$tr,)?
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                if ($tp::phase(self) & Phase::INITIALIZED).is_empty() {
                    write!(f,"UnInitialized")
                } else {
                    write!(f,"{:?}",**self)
                }
            }
        }
    }
}
macro_rules! non_static_impls {
    ($tp:ident, $data:ty $(,T: $tr:ident)? $(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G> {
            pub const fn new(g: G) -> Self {
                Self::new_static(g)
            }
        }
        impl<T: Default> Default for $tp<T, fn() -> T> {
            fn default() -> Self {
                Self::new(T::default)
            }
        }
    };
}
non_static_impls! {Lazy,UnInited::<T>}
non_static_debug! {Lazy,UnInited::<T>}
non_static_impls! {UnSyncLazy,UnInited::<T>}
non_static_debug! {UnSyncLazy,UnInited::<T>}

impl<T, G> Drop for Lazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe {
                GenericLazy::get_raw_data(&self.__private)
                    .get()
                    .drop_in_place()
            }
        }
    }
}
impl<T, G> Drop for UnSyncLazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe {
                GenericLazy::get_raw_data(&self.__private)
                    .get()
                    .drop_in_place()
            }
        }
    }
}

macro_rules! impl_mut_lazy {
    ($tp:ident, $man:ty, $checker:ty, $data:ty, $gdw: ident, $gd: ident $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?}
        impl_mut_lazy! {@lock $tp,$man,$checker,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
    };
    (static $tp:ident, $man:ty, $checker:ty, $data:ty, $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, 'static}
        impl_mut_lazy! {@lock $tp,$man,$checker,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)? , 'static}
    };
    (thread_local $tp:ident, $man:ty, $checker:ty, $data:ty, $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe}
        impl_mut_lazy! {@lock_thread_local $tp,$man,$checker,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
    };
    (global $tp:ident, $man:ty, $checker:ty, $data:ty, $gdw: ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man,$checker,$data,<$man>::new()$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe, 'static}
        impl_mut_lazy! {@lock_global $tp,$man,$checker,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
    };
    (@lock $tp:ident, $man:ty, $checker:ty, $data:ty, $gdw: ident, $gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)? $(,$static:lifetime)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Initialize if necessary and returns a read lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn read(&$($static)? self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
               GenericMutLazy::init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_read(&$($static)? self) -> Option<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
               GenericMutLazy::fast_init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&$($static)? self) -> Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
               GenericMutLazy::try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized or an [AccessError]. Otherwise returns `None`
            pub fn fast_try_read(&$($static)? self) -> Option<Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
               GenericMutLazy::fast_try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn write(&$($static)? self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
               GenericMutLazy::init_then_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_write(&$($static)? self) -> Option<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
               GenericMutLazy::fast_init_then_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_write(&$($static)? self) -> Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
               GenericMutLazy::try_write_lock(&self.__private)
            }
            #[inline(always)]
            /// if the lazy is not already read or write locked: get a write lock if the lazy is initialized or an [AccessError] . Otherwise returns `None`
            pub fn fast_try_write(&$($static)? self) -> Option<Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
               GenericMutLazy::fast_try_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize the lazy if no previous attempt to initialized it where performed
            pub fn init(&$($static)? self) {
                GenericMutLazy::init_then_write_lock(&self.__private);
            }
        }

    };
    (@lock_thread_local $tp:ident, $man:ty, $checker:ty, $data:ty,$gdw:ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {

        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>$(+$tr)?,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Initialize if necessary and returns a read lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn read(&self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                GenericMutLazy::init_then_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_read(&self) -> Option<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
               GenericMutLazy::fast_init_then_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&self) -> Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
               GenericMutLazy::try_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized or an [AccessError]. Otherwise returns `None`
            pub fn fast_try_read(&self) -> Option<Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
               GenericMutLazy::fast_try_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn write(&self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                GenericMutLazy::init_then_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_write(&self) -> Option<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
               GenericMutLazy::fast_init_then_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_write(&self) -> Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
               GenericMutLazy::try_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// if the lazy is not already read or write locked: get a write lock if the lazy is initialized or an [AccessError] . Otherwise returns `None`
            pub fn fast_try_write(&self) -> Option<Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
               GenericMutLazy::fast_try_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize the lazy if no previous attempt to initialized it where performed
            pub fn init(&self) -> Phase {
                let l = GenericMutLazy::init_then_write_lock(unsafe{as_static(&self.__private)});
                Phased::phase(&l)
            }
        }

    };
    (@lock_global $tp:ident, $man:ty, $checker:ty, $data:ty,$gdw:ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {

        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>$(+$tr)?,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Initialize if necessary and returns a read lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn read(&'static self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericMutLazy::read_lock_unchecked(&self.__private)};
                    l
                } else {
                    GenericMutLazy::init_then_read_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            #[inline(always)]
            pub fn fast_read(&'static self) -> Option<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
                if inited::global_inited_hint() {
                    unsafe{GenericMutLazy::fast_read_lock_unchecked(&self.__private)}
                } else {
                    GenericMutLazy::fast_init_then_read_lock(&self.__private)
                }
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&'static self) -> Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
                if inited::global_inited_hint() {
                    Ok(unsafe{GenericMutLazy::read_lock_unchecked(&self.__private)})
                } else {
                    GenericMutLazy::try_read_lock(&self.__private)
                }
            }
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized or an [AccessError]. Otherwise returns `None`
            #[inline(always)]
            pub fn fast_try_read(&'static self) -> Option<Result<ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
                if inited::global_inited_hint() {
                    unsafe{GenericMutLazy::fast_read_lock_unchecked(&self.__private)}.map(Ok)
                } else {
                    GenericMutLazy::fast_try_read_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            #[inline(always)]
            pub fn write(&'static self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                if inited::global_inited_hint() {
                    unsafe{GenericMutLazy::write_lock_unchecked(&self.__private)}
                } else {
                    GenericMutLazy::init_then_write_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            #[inline(always)]
            pub fn fast_write(&'static self) -> Option<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>> {
                if inited::global_inited_hint() {
                    unsafe{GenericMutLazy::fast_write_lock_unchecked(&self.__private)}
                } else {
                    GenericMutLazy::fast_init_then_write_lock(&self.__private)
                }
            }
            /// Get a read lock if the lazy is initialized or an [AccessError]
            #[inline(always)]
            pub fn try_write(&'static self) -> Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError> {
                if inited::global_inited_hint() {
                    Ok(unsafe{GenericMutLazy::write_lock_unchecked(&self.__private)})
                } else {
                    GenericMutLazy::try_write_lock(&self.__private)
                }
            }
            /// if the lazy is not already read or write locked: get a write lock if the lazy is initialized or an [AccessError] . Otherwise returns `None`
            #[inline(always)]
            pub fn fast_try_write(&'static self) -> Option<Result<WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>>,AccessError>> {
                if inited::global_inited_hint() {
                    unsafe{GenericMutLazy::fast_write_lock_unchecked(&self.__private)}.map(Ok)
                } else {
                    GenericMutLazy::fast_try_write_lock(&self.__private)
                }
            }
            /// Initialize the lazy if no previous attempt to initialized it where performed
            #[inline(always)]
            pub fn init(&'static self) -> Phase {
                let l = GenericMutLazy::init_then_write_lock(&self.__private);
                Phased::phase(&l)
            }
        }

    };
    (@proc $tp:ident, $man:ty, $checker:ty, $data:ty, $init:expr$(,T: $tr: ident)?$(,G: $trg:ident)?
    ,$doc:literal $(cfg($attr:meta))? $(,$safe:ident)? $(,$static:lifetime)?) => {
        #[doc=$doc]
        $(#[cfg_attr(docsrs,doc(cfg($attr)))])?
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericMutLazy<$data, G, $man, $checker>,
        }
        impl<T, G> Phased for $tp<T, G>
        where T: 'static + LazyData,
        G: 'static + Generator<T>
        {
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
        }

        impl<T, G> $tp<T, G> {
            /// Build a new static object.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericMutLazy::new(f, <$man>::new(),<$data>::INIT)},
                }
            }
            /// Build a new static object with debug informations.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericMutLazy::new_with_info(f, <$man>::new(), <$data>::INIT,info)},
                }
            }
        }

        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Returns the current phase and synchronize with the end
            /// of the transition to the returned phase.
            pub fn phase(&$($static)? self) -> Phase {
                Phased::phase(Sequential::sequentializer(&self.__private))
            }
        }
    };
}

impl_mut_lazy! {MutLazy,NonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,
"A mutex that initialize its content only once on the first lock"}

impl_mut_lazy! {global QuasiMutLazy,StartUpInitedNonFinalizedSyncSequentializer,InitializedChecker,UnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_mut_lazy! {static MutLazyFinalize,ExitSequentializer,InitializedChecker,UnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(mut_lazy,finalize)]"
}

impl_mut_lazy! {global QuasiMutLazyFinalize,ExitSequentializer,InitializedChecker,UnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,T:Finaly, G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}
impl_mut_lazy! {static MutLazyDroped,ExitSequentializer,InitializedAndNonFinalizedChecker,DropedUnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,G:Sync,
"The actual type of statics attributed with #[dynamic(mut_lazy,finalize)]"
}

impl_mut_lazy! {global QuasiMutLazyDroped,ExitSequentializer,InitializedChecker,DropedUnInited::<T>, SyncPhaseGuard, SyncReadPhaseGuard,G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_mut_lazy! {UnSyncMutLazy,NonFinalizedUnSyncSequentializer,InitializedChecker,UnInited::<T>, UnSyncPhaseGuard,UnSyncReadPhaseGuard,
"A RefCell that initialize its content on the first access"
}

#[cfg(feature = "thread_local")]
impl_mut_lazy! {thread_local UnSyncMutLazyFinalize,ThreadExitSequentializer,InitializedChecker,UnInited::<T>, UnSyncPhaseGuard,UnSyncReadPhaseGuard,T:Finaly,
"The actual type of thread_local statics attributed with #[dynamic(mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}
#[cfg(feature = "thread_local")]
impl_mut_lazy! {thread_local UnSyncMutLazyDroped,ThreadExitSequentializer,InitializedAndNonFinalizedChecker,DropedUnInited::<T>, UnSyncPhaseGuard,UnSyncReadPhaseGuard,
"The actual type of thread_local statics attributed with #[dynamic(mut_lazy,drop)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}

macro_rules! non_static_mut_debug {
    ($tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T:Debug, G> Debug for $tp<T, G>
            where $data: 'static + LazyData<Target=T>,
            G: 'static + Generator<T>,
            $(G:$trg, T:Sync,)?
            $(T:$tr,)?
        {
            fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
                if ($tp::phase(self) & Phase::INITIALIZED).is_empty() {
                    write!(f,"UnInitialized")
                } else {
                    write!(f,"{:?}",*self.read())
                }
            }
        }
    }
}
non_static_impls! {MutLazy,UnInited::<T>}
non_static_mut_debug! {MutLazy,UnInited::<T>}
non_static_impls! {UnSyncMutLazy,UnInited::<T>}
non_static_mut_debug! {UnSyncMutLazy,UnInited::<T>}

impl<T, G> Drop for MutLazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericMutLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe { (&*self.__private).get().drop_in_place() }
        }
    }
}
impl<T, G> Drop for UnSyncMutLazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericMutLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe { (&*self.__private).get().drop_in_place() }
        }
    }
}

#[cfg(all(support_priority, not(feature = "test_no_global_lazy_hint")))]
mod inited {

    use core::sync::atomic::{AtomicBool, Ordering};

    static LAZY_INIT_ENSURED: AtomicBool = AtomicBool::new(false);

    #[static_init_macro::constructor(__lazy_init_finished)]
    extern "C" fn mark_inited() {
        LAZY_INIT_ENSURED.store(true, Ordering::Release);
    }

    #[inline(always)]
    pub(super) fn global_inited_hint() -> bool {
        LAZY_INIT_ENSURED.load(Ordering::Acquire)
    }
}
#[cfg(not(all(support_priority, not(feature = "test_no_global_lazy_hint"))))]
mod inited {
    #[inline(always)]
    pub(super) const fn global_inited_hint() -> bool {
        false
    }
}

#[cfg(test)]
mod test_lazy {
    use super::Lazy;
    static _X: Lazy<u32, fn() -> u32> = Lazy::new(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}

#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_lazy {
    use super::QuasiLazy;
    static _X: QuasiLazy<u32, fn() -> u32> = unsafe { QuasiLazy::new_static(|| 22) };
    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}
#[cfg(all(test, feature = "thread_local"))]
mod test_local_lazy {
    use super::UnSyncLazy;
    #[thread_local]
    static _X: UnSyncLazy<u32, fn() -> u32> = UnSyncLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}
#[cfg(test)]
mod test_lazy_finalize {
    use super::LazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: LazyFinalize<A, fn() -> A> = unsafe { LazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_lazy_finalize {
    use super::QuasiLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: QuasiLazyFinalize<A, fn() -> A> = unsafe { QuasiLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
#[cfg(all(test, feature = "thread_local"))]
mod test_local_lazy_finalize {
    use super::UnSyncLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: UnSyncLazyFinalize<A, fn() -> A> =
        unsafe { UnSyncLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert_eq!((*_X).0, 22);
    }
}
#[cfg(all(test, feature = "thread_local"))]
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

#[cfg(test)]
mod test_mut_lazy {
    use super::MutLazy;
    static _X: MutLazy<u32, fn() -> u32> = MutLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy {
    use super::QuasiMutLazy;
    static _X: QuasiMutLazy<u32, fn() -> u32> = unsafe { QuasiMutLazy::new_static(|| 22) };
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(test)]
mod test_mut_lazy_finalize {
    use super::MutLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: MutLazyFinalize<A, fn() -> A> = MutLazyFinalize::new_static(|| A(22));
    #[test]
    fn test() {
        assert!((*_X.read()).0 == 22);
        *_X.write() = A(33);
        assert_eq!((*_X.read()).0, 33);
    }
}
#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy_finalize {
    use super::QuasiMutLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: QuasiMutLazyFinalize<A, fn() -> A> =
        unsafe { QuasiMutLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert!((*_X.read()).0 == 22);
        *_X.write() = A(33);
        assert_eq!((*_X.read()).0, 33);
    }
}
#[cfg(test)]
mod test_mut_lazy_dropped {
    use super::MutLazyDroped;
    static _X: MutLazyDroped<u32, fn() -> u32> = MutLazyDroped::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy_dropped {
    use super::QuasiMutLazyDroped;
    static _X: QuasiMutLazyDroped<u32, fn() -> u32> =
        unsafe { QuasiMutLazyDroped::new_static(|| 22) };
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_lazy {
    use super::UnSyncMutLazy;
    #[thread_local]
    static _X: UnSyncMutLazy<u32, fn() -> u32> = UnSyncMutLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_lazy_finalize {
    use super::UnSyncMutLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: UnSyncMutLazyFinalize<A, fn() -> A> =
        unsafe { UnSyncMutLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert!((*_X.read()).0 == 22);
        *_X.write() = A(33);
        assert_eq!((*_X.read()).0, 33);
    }
}
#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_lazy_droped {
    use super::UnSyncMutLazyDroped;
    #[thread_local]
    static _X: UnSyncMutLazyDroped<u32, fn() -> u32> =
        unsafe { UnSyncMutLazyDroped::new_static(|| 22) };
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}

#[inline(always)]
/// # Safety
/// v must refer to a static
unsafe fn as_static<T>(v: &T) -> &'static T {
    &*(v as *const _)
}
