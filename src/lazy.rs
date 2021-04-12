use crate::phase_locker::{
    SyncPhaseGuard, SyncPhaseLocker, SyncReadPhaseGuard, UnSyncPhaseGuard, UnSyncPhaseLocker,
    UnSyncReadPhaseGuard,
};
use crate::{
    generic_lazy::{
        AccessError, DropedUnInited, GenericLazy, GenericLockedLazy, LazyData, LazyPolicy, Primed,
        ReadGuard, UnInited, WriteGuard,
    },
    lazy_sequentializer::UnSyncSequentializer,
    Finaly, Generator, GeneratorTolerance, Phase, Phased, StaticInfo, Uninit,
};

#[cfg(feature = "thread_local")]
use crate::exit_sequentializer::ThreadExitSequentializer;

use crate::{exit_sequentializer::ExitSequentializer, lazy_sequentializer::SyncSequentializer};

use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::cell::Cell;

pub struct InitializedChecker<T>(PhantomData<T>);

impl<Tol: GeneratorTolerance> LazyPolicy
    for InitializedChecker<Tol>
{
    #[inline(always)]
    fn shall_init(p: Phase) -> bool {
        if Tol::INIT_FAILURE {
            !p.intersects(Phase::INITIALIZED)
        } else {
            p.is_empty()
        }
    }
    #[inline(always)]
    fn is_accessible(p: Phase) -> bool {
        p.intersects(Phase::INITIALIZED)
    }
    #[inline(always)]
    fn post_init_is_accessible(p: Phase) -> bool {
        if Tol::INIT_FAILURE {
            Self::initialized_is_accessible(p)
        } else {
            Self::is_accessible(p)
        }
    }
    #[inline(always)]
    fn initialized_is_accessible(_: Phase) -> bool {
        true
    }
}


pub struct InitializedSoftFinalizedCheckerGeneric<T, const REG_ALWAYS: bool>(PhantomData<T>);

impl<Tol: GeneratorTolerance, const REG_ALWAYS: bool> LazyPolicy
    for InitializedSoftFinalizedCheckerGeneric<Tol, REG_ALWAYS>
{
    #[inline(always)]
    fn shall_init(p: Phase) -> bool {
        if Tol::INIT_FAILURE {
            !p.intersects(Phase::INITIALIZED)
        } else {
            p.is_empty()
        }
    }
    #[inline(always)]
    fn is_accessible(p: Phase) -> bool {
        p.intersects(Phase::INITIALIZED)
    }
    #[inline(always)]
    fn post_init_is_accessible(p: Phase) -> bool {
        if Tol::INIT_FAILURE && (REG_ALWAYS || Tol::FINAL_REGISTRATION_FAILURE) {
            debug_assert!(!REG_ALWAYS || p.intersects(Phase::REGISTERED));
            Self::initialized_is_accessible(p)
        } else {
            Self::is_accessible(p)
        }
    }
    #[inline(always)]
    fn initialized_is_accessible(_: Phase) -> bool {
        true
    }
}

pub struct InitializedHardFinalizedCheckerGeneric<T, const REG_ALWAYS: bool>(PhantomData<T>);

impl<Tol: GeneratorTolerance, const REG_ALWAYS: bool> LazyPolicy
    for InitializedHardFinalizedCheckerGeneric<Tol, REG_ALWAYS>
{
    #[inline(always)]
    fn shall_init(p: Phase) -> bool {
        if Tol::INIT_FAILURE {
            !p.intersects(Phase::INITIALIZED)
        } else {
            p.is_empty()
        }
    }
    #[inline(always)]
    fn is_accessible(p: Phase) -> bool {
        p.intersects(Phase::INITIALIZED) && Self::initialized_is_accessible(p)
    }
    #[inline(always)]
    fn post_init_is_accessible(p: Phase) -> bool {
        if Tol::INIT_FAILURE && (REG_ALWAYS || Tol::FINAL_REGISTRATION_FAILURE) {
            debug_assert!(!REG_ALWAYS || p.intersects(Phase::REGISTERED));
            Self::initialized_is_accessible(p)
        } else {
            Self::is_accessible(p)
        }
    }
    #[inline(always)]
    fn initialized_is_accessible(p: Phase) -> bool {
        !p.intersects(Phase::FINALIZED | Phase::FINALIZATION_PANICKED)
    }
}

/// Final registration always succeed for non thread local statics
type InitializedSoftFinalizedChecker<T> = InitializedSoftFinalizedCheckerGeneric<T, false>;

type InitializedHardFinalizedChecker<T> = InitializedHardFinalizedCheckerGeneric<T, false>;

/// Thread local final registration always succeed for thread local on glibc plateforms
#[cfg(all(feature = "thread_local",cxa_thread_at_exit))]
type InitializedSoftFinalizedTLChecker<T> = InitializedSoftFinalizedCheckerGeneric<T, true>;

#[cfg(all(feature = "thread_local",cxa_thread_at_exit))]
type InitializedHardFinalizedTLChecker<T> = InitializedHardFinalizedCheckerGeneric<T, true>;

#[cfg(all(feature = "thread_local",not(cxa_thread_at_exit)))]
type InitializedSoftFinalizedTLChecker<T> = InitializedSoftFinalizedCheckerGeneric<T, false>;

#[cfg(all(feature = "thread_local",not(cxa_thread_at_exit)))]
type InitializedHardFinalizedTLChecker<T> = InitializedHardFinalizedCheckerGeneric<T, false>;


/// Helper to access static lazy associated functions
pub trait LazyAccess: Sized {
    type Target;
    /// Initialize if necessary then return a reference to the target.
    ///
    /// # Panics
    ///
    /// Panic if previous attempt to initialize has panicked and the lazy policy does not
    /// tolorate further initialization attempt or if initialization
    /// panic.
    fn get(this: Self) -> Self::Target;
    /// Return a reference to the target if initialized otherwise return an error.
    fn try_get(this: Self) -> Result<Self::Target, AccessError>;
    /// The current phase of the static
    fn phase(this: Self) -> Phase;
    /// Initialize the static if there were no previous attempt to initialize it.
    fn init(this: Self) -> Phase;
}

macro_rules! impl_lazy {
    ($tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:path, $locker:ty $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker $(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?}
        impl_lazy! {@deref $tp,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (global $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker:ty $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe,'static}
        impl_lazy! {@deref_global $tp,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (static $tp:ident, $man:ident$(<$x:ident>)?, $checker: ident, $data:ty, $locker:ty $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))?) => {
        impl_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe,'static}
        impl_lazy! {@deref_static $tp,$data$(,T:$tr)?$(,G:$trg)?}
    };
    (@deref $tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: LazyData<Target=T>,
        G: Generator<T>,
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
                this.__private.only_init_then_get_mut()
            }
            #[inline(always)]
            pub fn try_get_mut(this: &mut Self) -> Result<&'_ mut T,AccessError> {
                this.__private.try_get_mut()
            }
        }
        impl<T, G> Deref for $tp<T, G>
        where $data: LazyData<Target=T>,
        G: Generator<T>,
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
        where $data: LazyData<Target=T>,
        G: Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            fn deref_mut(&mut self) -> &mut Self::Target {
                Self::get_mut(self)
            }
        }

        impl<'a,T,G> LazyAccess for &'a $tp<T,G>
            where $data: LazyData<Target=T>,
            G: Generator<T>,
            $(G:$trg, T:Sync,)?
            $(T:$tr,)?
            {
            type Target = &'a T;
             #[inline(always)]
             fn get(this: Self) -> &'a T {
                 $tp::get(this)
             }
             #[inline(always)]
             fn try_get(this: Self) -> Result<&'a T,AccessError>{
                 $tp::try_get(this)
             }
             #[inline(always)]
             fn phase(this: Self) -> Phase{
                 $tp::phase(this)
             }
             #[inline(always)]
             fn init(this: Self) -> Phase {
                 $tp::init(this)
             }
        }

    };
    (@deref_static $tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
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

        impl<T,G> LazyAccess for &'static $tp<T,G>
            where $data: 'static + LazyData<Target=T>,
            G: 'static + Generator<T>,
            $(G:$trg, T:Sync,)?
            $(T:$tr,)?
            {
            type Target = &'static T;
             #[inline(always)]
             fn get(this: Self) -> &'static T {
                 $tp::get(this)
             }
             #[inline(always)]
             fn try_get(this: Self) -> Result<&'static T,AccessError>{
                 $tp::try_get(this)
             }
             #[inline(always)]
             fn phase(this: Self) -> Phase{
                 $tp::phase(this)
             }
             #[inline(always)]
             fn init(this: Self) -> Phase {
                 $tp::init(this)
             }
        }

    };
    (@deref_global $tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
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
                    // set all LesserLazy are guaranteed to be initialized
                    // Moreover global lazy are never dropped
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
                    // set all LesserLazy are guaranteed to be initialized
                    // Moreover global lazy are never dropped
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
                // set all LesserLazy are guaranteed to be initialized
                Self::get(unsafe{as_static(self)})
            }
        }
        impl<T,G> LazyAccess for &'static $tp<T,G>
            where $data: 'static + LazyData<Target=T>,
            G: 'static + Generator<T>,
            $(G:$trg, T:Sync,)?
            $(T:$tr,)?
            {
            type Target = &'static T;
             #[inline(always)]
             fn get(this: Self) -> &'static T {
                 $tp::get(this)
             }
             #[inline(always)]
             fn try_get(this: Self) -> Result<&'static T,AccessError>{
                 $tp::try_get(this)
             }
             #[inline(always)]
             fn phase(this: Self) -> Phase{
                 $tp::phase(this)
             }
             #[inline(always)]
             fn init(this: Self) -> Phase{
                 $tp::init(this)
             }
        }

    };
    (@proc $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty,$locker:ty $(,T: $tr: ident)?$(,G: $trg:ident)?,$doc:literal $(cfg($attr:meta))? $(,$safe:ident)?$(,$static:lifetime)?) => {
        #[doc=$doc]
        $(#[cfg_attr(docsrs,doc(cfg($attr)))])?
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericLazy<$data, G, $man$(::<$x>)?, $checker::<G>>,
        }
        impl<T, G> Phased for $tp<T, G>
        where $data: $($static +)? LazyData<Target=T>,
        G: $($static +)? Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
        }

        impl<T, G> $tp<T, G> {
            #[inline(always)]
            /// Build a new static object
            ///
            /// # Safety
            ///
            /// This function may be unsafe if building any thing else than a thread local object
            /// or a static will be the cause of undefined behavior
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericLazy::new(f, $man::new(<$locker>::new(Phase::empty())),<$data>::INIT)},
                }
            }
            #[inline(always)]
            /// Build a new static object with debug information
            ///
            /// # Safety
            ///
            /// This function may be unsafe if building any thing else than a thread local object
            /// or a static will be the cause of undefined behavior
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLazy::new_with_info(f, $man::new(<$locker>::new(Phase::empty())), <$data>::INIT,info)},
                }
            }
        }

        impl<T, G> $tp<T, G>
        where $data: $($static +)? LazyData<Target=T>,
        G: $($static +)? Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            pub fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
            #[inline(always)]
            pub fn init(this: &$($static)? Self) -> Phase {
                GenericLazy::init(&this.__private)
            }
        }

    };
}

impl_lazy! {Lazy,SyncSequentializer<G>,InitializedChecker,UnInited::<T>,SyncPhaseLocker,
"A type that initialize it self only once on the first access"}

impl_lazy! {global LesserLazy,SyncSequentializer<G>,InitializedChecker,UnInited::<T>,SyncPhaseLocker,
"The actual type of statics attributed with #[dynamic(quasi_lazy)]. \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_lazy! {static LazyFinalize,ExitSequentializer<G>,InitializedSoftFinalizedChecker,UnInited::<T>,SyncPhaseLocker,T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static."
}

impl_lazy! {global LesserLazyFinalize,ExitSequentializer<G>,InitializedSoftFinalizedChecker,UnInited::<T>,SyncPhaseLocker,T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_lazy,finalize)]. \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_lazy! {UnSyncLazy,UnSyncSequentializer<G>,InitializedChecker,UnInited::<T>,UnSyncPhaseLocker,
"A version of [Lazy] whose reference can not be passed to other thread"
}

#[cfg(feature = "thread_local")]
impl_lazy! {static UnSyncLazyFinalize,ThreadExitSequentializer<G>,InitializedSoftFinalizedTLChecker,UnInited::<T>,UnSyncPhaseLocker,T:Finaly,
"The actual type of thread_local statics attributed with #[dynamic(lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static." cfg(feature="thread_local")
}
#[cfg(feature = "thread_local")]
impl_lazy! {static UnSyncLazyDroped,ThreadExitSequentializer<G>,InitializedHardFinalizedTLChecker,DropedUnInited::<T>,UnSyncPhaseLocker,
"The actual type of thread_local statics attributed with #[dynamic(lazy,drop)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable static." cfg(feature="thread_local")
}

use core::fmt::{self, Debug, Formatter};
macro_rules! non_static_debug {
    ($tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T:Debug, G> Debug for $tp<T, G>
            where $data: LazyData<Target=T>,
            G: Generator<T>,
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
        impl<T, G> $tp<T, Cell<Option<G>>> 
        where G: FnOnce() -> T
        {
            #[inline(always)]
            pub fn new(g: G) -> Self {
                Self::new_static(Cell::new(Some(g)))
            }
        }
        impl<T: Default> Default for $tp<T, fn() -> T> {
            #[inline(always)]
            fn default() -> Self {
                Self::new_static(T::default)
            }
        }
    };
}
non_static_impls! {Lazy,UnInited::<T>}
non_static_debug! {Lazy,UnInited::<T>}
non_static_impls! {UnSyncLazy,UnInited::<T>}
non_static_debug! {UnSyncLazy,UnInited::<T>}

impl<T, G> Drop for Lazy<T, G> {
    #[inline(always)]
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
    #[inline(always)]
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
    ($tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker:ty, $gdw: ident, $gd: ident $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?}
        impl_mut_lazy! {@lock $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
        impl_mut_lazy! {@uninited $tp, $man$(<$x>)?, $data, $locker}
    };
    (static $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker: ty, $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, 'static}
        impl_mut_lazy! {@lock $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)? , 'static}
        impl_mut_lazy! {@uninited $tp, $man$(<$x>)?, $data, $locker}
    };
    (const_static $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker: ty, $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, 'static}
        impl_mut_lazy! {@const_lock $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)? , 'static}
        impl_mut_lazy! {@uninited $tp, $man$(<$x>)?, $data, $locker}
    };
    (thread_local $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty,$locker: ty,  $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe}
        impl_mut_lazy! {@lock_thread_local $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
        impl_mut_lazy! {@uninited $tp, $man$(<$x>)?, $data, $locker, unsafe}
    };
    (global $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty,$locker: ty,  $gdw: ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe, 'static}
        impl_mut_lazy! {@lock_global $tp,$checker,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
        impl_mut_lazy! {@uninited $tp, $man$(<$x>)?, $data, $locker, unsafe}
    };
    (primed_static $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker:ty, $gdw: ident, $gd: ident $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, 'static}
        impl_mut_lazy! {@lock $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?, 'static}
        impl_mut_lazy! {@prime $tp, $man$(<$x>)?, $data, $locker}
        impl_mut_lazy! {@prime_static $tp, $checker, $data, $gdw, $gd$(,T:$tr)?$(,G:$trg)?}
    };
    (primed_thread_local $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty,$locker: ty,  $gdw: ident,$gd:ident  $(,T: $tr: ident)?$(,G: $trg:ident)?, $doc:literal $(cfg($attr:meta))?) => {
        impl_mut_lazy! {@proc $tp,$man$(<$x>)?,$checker,$data,$locker$(,T:$tr)?$(,G:$trg)?,$doc $(cfg($attr))?, unsafe}
        impl_mut_lazy! {@lock_thread_local $tp,$data,$gdw,$gd$(,T:$tr)?$(,G:$trg)?}
        impl_mut_lazy! {@prime $tp, $man$(<$x>)?, $data, $locker, unsafe}
        impl_mut_lazy! {@prime_thread_local $tp, $checker, $data, $gdw, $gd$(,T:$tr)?$(,G:$trg)?}
    };
    (@lock $tp:ident, $data:ty, $gdw: ident, $gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)? $(,$static:lifetime)?) => {
        impl<T, G> $tp<T, G>
        where $data: $($static+)? LazyData<Target=T>,
        G:$($static +)? Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Initialize if necessary and returns a read lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn read(&$($static)? self) -> ReadGuard<$gd::<'_,$data>> {
               GenericLockedLazy::init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_read(&$($static)? self) -> Option<ReadGuard<$gd::<'_,$data>>> {
               GenericLockedLazy::fast_init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&$($static)? self) -> Result<ReadGuard<$gd::<'_,$data>>,AccessError> {
               GenericLockedLazy::try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized or an [AccessError].
            /// Otherwise returns `None`
            pub fn fast_try_read(&$($static)? self) -> Option<Result<ReadGuard<$gd::<'_,$data>>,AccessError>> {
               GenericLockedLazy::fast_try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn write(&$($static)? self) -> WriteGuard<$gdw::<'_,$data>> {
               GenericLockedLazy::init_then_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_write(&$($static)? self) -> Option<WriteGuard<$gdw::<'_,$data>>> {
               GenericLockedLazy::fast_init_then_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_write(&$($static)? self) -> Result<WriteGuard<$gdw::<'_,$data>>,AccessError> {
               GenericLockedLazy::try_write_lock(&self.__private)
            }
            #[inline(always)]
            /// if the lazy is not already read or write locked: get a write lock if the lazy is initialized or an [AccessError] . Otherwise returns `None`
            pub fn fast_try_write(&$($static)? self) -> Option<Result<WriteGuard<$gdw::<'_,$data>>,AccessError>> {
               GenericLockedLazy::fast_try_write_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize the lazy if no previous attempt to initialized it where performed
            pub fn init(&$($static)? self) {
                GenericLockedLazy::init_then_write_lock(&self.__private);
            }
        }

    };
    (@const_lock $tp:ident, $data:ty, $gdw: ident, $gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)? $(,$static:lifetime)?) => {
        impl<T, G> $tp<T, G>
        where $data: $($static +)?  LazyData<Target=T>,
        G: $($static +)? Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Initialize if necessary and returns a read lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn read(&$($static)? self) -> ReadGuard<$gd::<'_,$data>> {
               GenericLockedLazy::init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            pub fn fast_read(&$($static)? self) -> Option<ReadGuard<$gd::<'_,$data>>> {
               GenericLockedLazy::fast_init_then_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&$($static)? self) -> Result<ReadGuard<$gd::<'_,$data>>,AccessError> {
               GenericLockedLazy::try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized or an [AccessError].
            /// Otherwise returns `None`
            pub fn fast_try_read(&$($static)? self) -> Option<Result<ReadGuard<$gd::<'_,$data>>,AccessError>> {
               GenericLockedLazy::fast_try_read_lock(&self.__private)
            }
            #[inline(always)]
            /// Initialize the lazy if no previous attempt to initialized it where performed
            pub fn init(&$($static)? self) {
                GenericLockedLazy::init_then_write_lock(&self.__private);
            }
        }

    };
    (@lock_thread_local $tp:ident, $data:ty,$gdw:ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {

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
            /// Panics if initialization panics or if initialization has panicked in a previous
            /// attempt to initialize.
            pub fn read(&self) -> ReadGuard<$gd::<'_,$data>> {
                GenericLockedLazy::init_then_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns some read lock if the lazy is not already write
            /// locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has
            /// panicked in a previous attempt to initialize.
            pub fn fast_read(&self) -> Option<ReadGuard<$gd::<'_,$data>>> {
               GenericLockedLazy::fast_init_then_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&self) -> Result<ReadGuard<$gd::<'_,$data>>,AccessError> {
               GenericLockedLazy::try_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized
            /// or an [AccessError]. Otherwise returns `None`
            pub fn fast_try_read(&self) -> Option<Result<ReadGuard<$gd::<'_,$data>>,AccessError>> {
               GenericLockedLazy::fast_try_read_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous
            /// attempt to initialize.
            pub fn write(&self) -> WriteGuard<$gdw::<'_,$data>> {
                GenericLockedLazy::init_then_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has
            /// panicked in a previous attempt to initialize.
            pub fn fast_write(&self) -> Option<WriteGuard<$gdw::<'_,$data>>> {
               GenericLockedLazy::fast_init_then_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_write(&self) -> Result<WriteGuard<$gdw::<'_,$data>>,AccessError> {
               GenericLockedLazy::try_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// if the lazy is not already read or write locked: get a write lock if the lazy is
            /// initialized or an [AccessError] . Otherwise returns `None`
            pub fn fast_try_write(&self) ->
               Option<Result<WriteGuard<$gdw::<'_,$data>>,AccessError>> {
               GenericLockedLazy::fast_try_write_lock(unsafe{as_static(&self.__private)})
            }
            #[inline(always)]
            /// Initialize the lazy if no previous attempt to initialized it where performed
            pub fn init(&self) -> Phase {
                let l = GenericLockedLazy::init_then_write_lock(unsafe{as_static(&self.__private)});
                Phased::phase(&l)
            }
        }

    };
    (@lock_global $tp:ident, $checker:ident, $data:ty,$gdw:ident,$gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {

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
            pub fn read(&'static self) -> ReadGuard<$gd::<'_,$data>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::read_lock_unchecked(&self.__private)};
                    assert!(<$checker::<G>>::initialized_is_accessible(Phased::phase(&l)));
                    l
                } else {
                    GenericLockedLazy::init_then_read_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns some read lock if the lazy is not
            /// already write locked. If the lazy is already write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has panicked in a previous attempt to initialize.
            #[inline(always)]
            pub fn fast_read(&'static self) -> Option<ReadGuard<$gd::<'_,$data>>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::fast_read_lock_unchecked(&self.__private)};
                    if let Some(l) = &l {
                        assert!(<$checker::<G>>::initialized_is_accessible(Phased::phase(l)));
                    }
                    l
                } else {
                    GenericLockedLazy::fast_init_then_read_lock(&self.__private)
                }
            }
            #[inline(always)]
            /// Get a read lock if the lazy is initialized or an [AccessError]
            pub fn try_read(&'static self) -> Result<ReadGuard<$gd::<'_,$data>>,AccessError> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::read_lock_unchecked(&self.__private)};
                    let p = Phased::phase(&l);
                    if <$checker::<G>>::initialized_is_accessible(p) {
                        Ok(l)
                    } else {
                        Err(AccessError{phase:p})
                    }
                } else {
                    GenericLockedLazy::try_read_lock(&self.__private)
                }
            }
            /// if the lazy is not already write locked: get a read lock if the lazy is initialized
            /// or an [AccessError]. Otherwise returns `None`
            #[inline(always)]
            pub fn fast_try_read(&'static self) -> Option<Result<ReadGuard<$gd::<'_,$data>>,AccessError>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::fast_read_lock_unchecked(&self.__private)};
                    l.map(|l| {
                        let p = Phased::phase(&l);
                        if <$checker::<G>>::initialized_is_accessible(p) {
                            Ok(l)
                        } else {
                            Err(AccessError{phase:p})
                        }
                    })
                } else {
                    GenericLockedLazy::fast_try_read_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns a write lock
            ///
            /// # Panic
            ///
            /// Panics if initialization panics or if initialization has panicked in a previous
            /// attempt to initialize.
            #[inline(always)]
            pub fn write(&'static self) -> WriteGuard<$gdw::<'_,$data>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::write_lock_unchecked(&self.__private)};
                    assert!(<$checker::<G>>::initialized_is_accessible(Phased::phase(&l)));
                    l
                } else {
                    GenericLockedLazy::init_then_write_lock(&self.__private)
                }
            }
            /// Initialize if necessary and returns some write lock if the lazy is not
            /// already write locked. If the lazy is already read or write locked it returns `None`
            ///
            /// # Panic
            ///
            /// If locks succeeds, panics if initialization panics or if initialization has
            /// panicked in a previous attempt to initialize.
            #[inline(always)]
            pub fn fast_write(&'static self) -> Option<WriteGuard<$gdw::<'_,$data>>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::fast_write_lock_unchecked(&self.__private)};
                    if let Some(l) = &l {
                        assert!(<$checker::<G>>::initialized_is_accessible(Phased::phase(l)));
                    }
                    l
                } else {
                    GenericLockedLazy::fast_init_then_write_lock(&self.__private)
                }
            }
            /// Get a read lock if the lazy is initialized or an [AccessError]
            #[inline(always)]
            pub fn try_write(&'static self) -> Result<WriteGuard<$gdw::<'_,$data>>,AccessError> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::write_lock_unchecked(&self.__private)};
                    let p = Phased::phase(&l);
                    if <$checker::<G>>::initialized_is_accessible(p) {
                        Ok(l)
                    } else {
                        Err(AccessError{phase:p})
                    }
                } else {
                    GenericLockedLazy::try_write_lock(&self.__private)
                }
            }
            /// if the lazy is not already read or write locked: get a write lock if the lazy is
            /// initialized or an [AccessError] . Otherwise returns `None`
            #[inline(always)]
            pub fn fast_try_write(&'static self) -> Option<Result<WriteGuard<$gdw::<'_,$data>>,AccessError>> {
                if inited::global_inited_hint() {
                    let l = unsafe{GenericLockedLazy::fast_write_lock_unchecked(&self.__private)};
                    l.map(|l| {
                        let p = Phased::phase(&l);
                        if <$checker::<G>>::initialized_is_accessible(p) {
                            Ok(l)
                        } else {
                            Err(AccessError{phase:p})
                        }
                    })
                } else {
                    GenericLockedLazy::fast_try_write_lock(&self.__private)
                }
            }
            /// Initialize the lazy if no previous attempt to initialized it where performed
            #[inline(always)]
            pub fn init(&'static self) -> Phase {
                let l = GenericLockedLazy::init_then_write_lock(&self.__private);
                Phased::phase(&l)
            }
        }

    };
    (@prime_static $tp:ident,$checker:ident, $data:ty, $gdw: ident, $gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Return a read lock to the initialized value or an error containing a read lock to
            /// the primed or post uninited value
            pub fn primed_read_non_initializing(&'static self) ->
               Result<ReadGuard<$gd::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe {GenericLockedLazy::read_lock_unchecked(&self.__private)};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l)
               }
            }
            #[inline(always)]
            /// Initialize if possible and either return a read lock to the initialized value or an
            /// error containing a read lock to the primed or post uninited value
            pub fn primed_read(&'static self) -> Result<ReadGuard<$gd::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe {GenericLockedLazy::init_then_read_lock_unchecked(&self.__private)};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l)
               }
            }
            #[inline(always)]
            /// Return a write lock that refers to the initialized value or an
            /// error containing a read lock that refers to the primed or post uninited value
            pub fn primed_write_non_initializing(&'static self) -> Result<WriteGuard<$gdw::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::write_lock_unchecked(&self.__private)};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l.into())
               }
            }
            #[inline(always)]
            /// Initialize if possible and either return a write lock that refers to the
            /// initialized value or an error containing a read lock that refers to the primed or
            /// post uninited value
            pub fn primed_write(&'static self) -> Result<WriteGuard<$gdw::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::init_then_write_lock_unchecked(&self.__private)};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l.into())
               }
            }
        }
    };
    (@prime_thread_local $tp:ident,$checker:ident, $data:ty, $gdw: ident, $gd:ident$(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Return a read lock to the initialized value or an
            /// error containing a read lock to the primed or post uninited value
            pub fn primed_read_non_initializing(&self) -> Result<ReadGuard<$gd::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::read_lock_unchecked(as_static(&self.__private))};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l)
               }
            }
            #[inline(always)]
            /// Initialize if possible and either return a read lock to the initialized value or an
            /// error containing a read lock to the primed or post uninited value
            pub fn primed_read(&self) -> Result<ReadGuard<$gd::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::init_then_read_lock_unchecked(as_static(&self.__private))};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l)
               }
            }
            #[inline(always)]
            /// Return a write lock that refers to the initialized value or an
            /// error containing a read lock that refers to the primed or post uninited value
            pub fn primed_write_non_initializing(&self) -> Result<WriteGuard<$gdw::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::write_lock_unchecked(as_static(&self.__private))};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l.into())
               }
            }
            #[inline(always)]
            /// Initialize if possible and either return a write lock that refers to the initialized value or an
            /// error containing a read lock that refers to the primed or post uninited value
            pub fn primed_write(&self) -> Result<WriteGuard<$gdw::<'_,$data>>,ReadGuard<$gd::<'_,$data>>> {
               let l = unsafe{GenericLockedLazy::init_then_write_lock_unchecked(as_static(&self.__private))};
               let p = Phased::phase(&l);
               if <$checker::<G>>::is_accessible(p) {
                   Ok(l)
               } else {
                   Err(l.into())
               }
            }
        }
    };
    (@uninited $tp:ident, $man:ident$(<$x:ident>)?, $data:ty, $locker: ty$(,$safe:ident)?) => {
        impl<T, G> $tp<T, G> {
            #[inline(always)]
            /// Build a new static object.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericLockedLazy::new(f, $man::new(<$locker>::new(Phase::empty())),<$data>::INIT)},
                }
            }
            #[inline(always)]
            /// Build a new static object with debug informations.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLockedLazy::new_with_info(f, $man::new(<$locker>::new(Phase::empty())), <$data>::INIT,info)},
                }
            }
        }
    };
    (@prime $tp:ident, $man:ident$(<$x:ident>)?, $data:ty, $locker: ty $(,$safe:ident)?) => {
        impl<T, G> $tp<T, G> {
            #[inline(always)]
            /// Build a new static object.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)? fn new_static(v: T, f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericLockedLazy::new(f, $man::new(<$locker>::new(Phase::empty())),<$data>::prime(v))},
                }
            }
            #[inline(always)]
            /// Build a new static object with debug informations.
            ///
            /// # Safety
            ///
            /// This function may be unsafe if build this object as anything else than
            /// a static or a thread local static would be the cause of undefined behavior
            pub const $($safe)?  fn new_static_with_info(v: T, f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLockedLazy::new_with_info(f, $man::new(<$locker>::new(Phase::empty())), <$data>::prime(v),info)},
                }
            }
        }
    };
    (@proc $tp:ident, $man:ident$(<$x:ident>)?, $checker:ident, $data:ty, $locker: ty $(,T: $tr: ident)?$(,G: $trg:ident)?
    ,$doc:literal $(cfg($attr:meta))? $(,$safe:ident)? $(,$static:lifetime)?) => {
        #[doc=$doc]
        $(#[cfg_attr(docsrs,doc(cfg($attr)))])?
        pub struct $tp<T, G = fn() -> T> {
            __private: GenericLockedLazy<$data, G, $man$(<$x>)?, $checker::<G>>,
        }
        impl<T, G> Phased for $tp<T, G>
        where T: $($static +)? LazyData,
        G: $($static +)? Generator<T>
        {
            #[inline(always)]
            fn phase(this: &Self) -> Phase {
                Phased::phase(&this.__private)
            }
        }

        impl<T, G> $tp<T, G>
        where $data: $($static +)? LazyData<Target=T>,
        G: $($static +)? Generator<T>,
        $(G:$trg, T:Send,)?
        $(T:$tr,)?
        {
            #[inline(always)]
            /// Returns the current phase and synchronize with the end
            /// of the transition to the returned phase.
            pub fn phase(&$($static)? self) -> Phase {
                Phased::phase(&self.__private)
            }
        }
    };
}

impl_mut_lazy! {LockedLazy,SyncSequentializer<G>,InitializedChecker,UnInited::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,
"A mutex that initialize its content only once on the first lock"}

impl_mut_lazy! {primed_static PrimedLockedLazy,SyncSequentializer<G>,InitializedChecker,Primed::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,
"The actual type of statics attributed with #[dynamic(primed)]"}

impl_mut_lazy! {global LesserLockedLazy,SyncSequentializer<G>,InitializedChecker,UnInited::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_mut_lazy! {static LockedLazyFinalize,ExitSequentializer<G>,InitializedSoftFinalizedChecker,UnInited::<T>,SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard, T:Finaly,G:Sync,
"The actual type of statics attributed with #[dynamic(mut_lazy,finalize)]"
}

impl_mut_lazy! {global LesserLockedLazyFinalize,ExitSequentializer<G>,InitializedSoftFinalizedChecker,UnInited::<T>,SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,T:Finaly, G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}
impl_mut_lazy! {static LockedLazyDroped,ExitSequentializer<G>,InitializedHardFinalizedChecker,DropedUnInited::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,G:Sync,
"The actual type of statics attributed with #[dynamic(mut_lazy,finalize)]"
}

impl_mut_lazy! {primed_static PrimedLockedLazyDroped,ExitSequentializer<G>,InitializedHardFinalizedChecker,Primed::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,T:Uninit, G:Sync,
"The actual type of statics attributed with #[dynamic(primed,drop)]"
}

impl_mut_lazy! {const_static ConstLockedLazyDroped,ExitSequentializer<G>,InitializedHardFinalizedChecker,DropedUnInited::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,G:Sync,
"The actual type of statics attributed with #[dynamic(mut_lazy,finalize)]"
}

impl_mut_lazy! {global LesserLockedLazyDroped,ExitSequentializer<G>,InitializedHardFinalizedChecker,DropedUnInited::<T>, SyncPhaseLocker, SyncPhaseGuard, SyncReadPhaseGuard,G:Sync,
"The actual type of statics attributed with #[dynamic(quasi_mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe because this kind of static \
can only safely be used through this attribute macros."
}

impl_mut_lazy! {UnSyncLockedLazy,UnSyncSequentializer<G>,InitializedChecker,UnInited::<T>,UnSyncPhaseLocker, UnSyncPhaseGuard,UnSyncReadPhaseGuard,
"A RefCell that initialize its content on the first access"
}

#[cfg(feature = "thread_local")]
impl_mut_lazy! {primed_thread_local UnSyncPrimedLockedLazy,UnSyncSequentializer<G>,InitializedChecker,Primed::<T>,UnSyncPhaseLocker, UnSyncPhaseGuard,UnSyncReadPhaseGuard,
"The actual type of thread_local statics attributed with #[dynamic(primed)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}
#[cfg(feature = "thread_local")]
impl_mut_lazy! {primed_thread_local UnSyncPrimedLockedLazyDroped,ThreadExitSequentializer<G>,InitializedHardFinalizedTLChecker,Primed::<T>,UnSyncPhaseLocker, UnSyncPhaseGuard,UnSyncReadPhaseGuard, T:Uninit,
"The actual type of thread_local statics attributed with #[dynamic(primed,drop)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}

#[cfg(feature = "thread_local")]
impl_mut_lazy! {thread_local UnSyncLockedLazyFinalize,ThreadExitSequentializer<G>,InitializedSoftFinalizedTLChecker,UnInited::<T>,UnSyncPhaseLocker, UnSyncPhaseGuard,UnSyncReadPhaseGuard,T:Finaly,
"The actual type of thread_local statics attributed with #[dynamic(mut_lazy,finalize)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}
#[cfg(feature = "thread_local")]
impl_mut_lazy! {thread_local UnSyncLockedLazyDroped,ThreadExitSequentializer<G>,InitializedHardFinalizedTLChecker,DropedUnInited::<T>,UnSyncPhaseLocker, UnSyncPhaseGuard,UnSyncReadPhaseGuard,
"The actual type of thread_local statics attributed with #[dynamic(mut_lazy,drop)] \
\
The method (new)[Self::new] is unsafe as the object must be a non mutable thread_local static." cfg(feature="thread_local")
}

macro_rules! non_static_mut_debug {
    ($tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T:Debug, G> Debug for $tp<T, G>
            where $data: LazyData<Target=T>,
            G: Generator<T>,
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
non_static_impls! {LockedLazy,UnInited::<T>}
non_static_mut_debug! {LockedLazy,UnInited::<T>}
non_static_impls! {UnSyncLockedLazy,UnInited::<T>}
non_static_mut_debug! {UnSyncLockedLazy,UnInited::<T>}

impl<T: Send, G: Generator<T>> LockedLazy<T, G> {
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.__private.only_init_then_get_mut()
    }
    #[inline(always)]
    pub fn try_get_mut(&mut self) -> Result<&mut T, AccessError> {
        self.__private.try_get_mut()
    }
}
impl<T, G: Generator<T>> UnSyncLockedLazy<T, G> {
    #[inline(always)]
    pub fn get_mut(&mut self) -> &mut T {
        self.__private.only_init_then_get_mut()
    }
    #[inline(always)]
    pub fn try_get_mut(&mut self) -> Result<&mut T, AccessError> {
        self.__private.try_get_mut()
    }
}

impl<T, G> Drop for LockedLazy<T, G> {
    #[inline(always)]
    fn drop(&mut self) {
        if Phased::phase(GenericLockedLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe { (&*self.__private).get().drop_in_place() }
        }
    }
}
impl<T, G> Drop for UnSyncLockedLazy<T, G> {
    #[inline(always)]
    fn drop(&mut self) {
        if Phased::phase(GenericLockedLazy::sequentializer(&self.__private))
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
    static _X: Lazy<u32, fn() -> u32> = Lazy::new_static(|| 22);

    #[test]
    fn test() {
        assert_eq!(*_X, 22);
    }
}

#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_lazy {
    use super::LesserLazy;
    static _X: LesserLazy<u32, fn() -> u32> = unsafe { LesserLazy::new_static(|| 22) };
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
    use super::LesserLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: LesserLazyFinalize<A, fn() -> A> =
        unsafe { LesserLazyFinalize::new_static(|| A(22)) };
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
    use super::LockedLazy;
    static _X: LockedLazy<u32, fn() -> u32> = LockedLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}

#[cfg(test)]
mod test_primed_mut_lazy_droped {
    use super::PrimedLockedLazyDroped;
    use crate::Uninit;
    struct A(u32);
    impl Uninit for A {
        fn uninit(&mut self) {
            self.0 = 0
        }
    }
    static _X: PrimedLockedLazyDroped<A> = PrimedLockedLazyDroped::new_static(A(42), || A(22));
    #[test]
    fn test() {
        match _X.primed_read_non_initializing() {
            Ok(_) => panic!("Unexpected"),
            Err(l) => assert_eq!(l.0, 42),
        }
        assert_eq!(_X.read().0, 22);
        _X.write().0 = 33;
        assert_eq!(_X.read().0, 33);
    }
}

#[cfg(test)]
mod test_primed_mut_lazy {
    use super::PrimedLockedLazy;
    static _X: PrimedLockedLazy<u32> = PrimedLockedLazy::new_static(42, || 22);
    #[test]
    fn test() {
        match _X.primed_read_non_initializing() {
            Ok(_) => panic!("Unexpected"),
            Err(l) => assert_eq!(*l, 42),
        }
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}

#[cfg(feature = "test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy {
    use super::LesserLockedLazy;
    static _X: LesserLockedLazy<u32, fn() -> u32> = unsafe { LesserLockedLazy::new_static(|| 22) };
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(test)]
mod test_mut_lazy_finalize {
    use super::LockedLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: LockedLazyFinalize<A, fn() -> A> = LockedLazyFinalize::new_static(|| A(22));
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
    use super::LesserLockedLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: LesserLockedLazyFinalize<A, fn() -> A> =
        unsafe { LesserLockedLazyFinalize::new_static(|| A(22)) };
    #[test]
    fn test() {
        assert!((*_X.read()).0 == 22);
        *_X.write() = A(33);
        assert_eq!((*_X.read()).0, 33);
    }
}
#[cfg(test)]
mod test_mut_lazy_dropped {
    use super::LockedLazyDroped;
    static _X: LockedLazyDroped<u32, fn() -> u32> = LockedLazyDroped::new_static(|| 22);
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
    use super::LesserLockedLazyDroped;
    static _X: LesserLockedLazyDroped<u32, fn() -> u32> =
        unsafe { LesserLockedLazyDroped::new_static(|| 22) };
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
    use super::UnSyncLockedLazy;
    #[thread_local]
    static _X: UnSyncLockedLazy<u32, fn() -> u32> = UnSyncLockedLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}

#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_primed_lazy {
    use super::UnSyncPrimedLockedLazy;
    #[thread_local]
    static _X: UnSyncPrimedLockedLazy<u32> =
        unsafe { UnSyncPrimedLockedLazy::new_static(42, || 22) };
    #[test]
    fn test() {
        match _X.primed_read_non_initializing() {
            Ok(x) => panic!("Unexpected {}", *x),
            Err(l) => assert_eq!(*l, 42),
        }
        assert_eq!(*_X.read(), 22);
        *_X.write() = 33;
        assert_eq!(*_X.read(), 33);
    }
}
#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_primed_lazy_droped {
    use super::UnSyncPrimedLockedLazyDroped;
    use crate::Uninit;
    struct A(u32);
    impl Uninit for A {
        fn uninit(&mut self) {
            self.0 = 0
        }
    }
    #[thread_local]
    static _X: UnSyncPrimedLockedLazyDroped<A> =
        unsafe { UnSyncPrimedLockedLazyDroped::new_static(A(42), || A(22)) };
    #[test]
    fn test() {
        match _X.primed_read_non_initializing() {
            Ok(_) => panic!("Unexpected"),
            Err(l) => assert_eq!(l.0, 42),
        }
        assert_eq!(_X.read().0, 22);
        _X.write().0 = 33;
        assert_eq!(_X.read().0, 33);
    }
}

#[cfg(test)]
#[cfg(feature = "thread_local")]
mod test_unsync_mut_lazy_finalize {
    use super::UnSyncLockedLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: UnSyncLockedLazyFinalize<A, fn() -> A> =
        unsafe { UnSyncLockedLazyFinalize::new_static(|| A(22)) };
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
    use super::UnSyncLockedLazyDroped;
    #[thread_local]
    static _X: UnSyncLockedLazyDroped<u32, fn() -> u32> =
        unsafe { UnSyncLockedLazyDroped::new_static(|| 22) };
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
