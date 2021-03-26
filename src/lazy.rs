use crate::mutex::{SyncPhaseGuard, SyncReadPhaseGuard, UnSyncPhaseGuard, UnSyncReadPhaseGuard,PhaseGuard};
use crate::{Finaly,
    generic_lazy::{GenericLazy, GenericMutLazy, LazyData, LazyPolicy, UnInited, DropedUnInited},
    splited_sequentializer::UnSyncSequentializer,
    Generator, LazySequentializer, Phase, Phased, Sequential, Sequentializer,
    SplitedLazySequentializer, StaticInfo,
    mutex::{LockNature,LockResult},
};

#[cfg(feature="thread_local")]
use crate::{at_exit::ThreadExitSequentializer};

use crate::{at_exit::ExitSequentializer, splited_sequentializer::SyncSequentializer};

use core::ops::{Deref, DerefMut};

pub struct InitializedChecker;

impl LazyPolicy for InitializedChecker {
    const INIT_ON_REG_FAILURE: bool = false;
    #[inline(always)]
    fn initialized_ok(_: Phase) -> bool {
        true
    }
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
    #[inline(always)]
    fn initialized_ok(p: Phase) -> bool {
        !p.intersects(Phase::FINALIZED)
    }
}
//pub struct InitializedRearmingChecker;
//impl LazyPolicy for InitializedRearmingChecker {
//    const INIT_ON_REG_FAILURE: bool = false;
//    #[inline(always)]
//    fn shall_proceed(p: Phase) -> bool {
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
            type ReadGuard = $gd_r<'a,T>;
            type WriteGuard = $gd<'a,T>;
            #[inline(always)]
            fn lock(s: &'a T, shall_proceed: impl Fn(Phase) -> LockNature) -> LockResult<$gd_r<'a,T>,$gd<'a, T>> {
                <$sub as Sequentializer<T>>::lock(s, shall_proceed)
            }
        }
        // SAFETY: ensured by $sub
        unsafe impl<'a, T: 'a + Sequential<Sequentializer = Self>> LazySequentializer<'a, T> for $typ {
            #[inline(always)]
            fn init(
                s: &'a T,
                on_uninited: impl Fn(Phase) -> bool,
                init: impl FnOnce(&'a <T as Sequential>::Data),
                init_on_reg_failure: bool,
            ) {
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
            ) -> Self::ReadGuard{
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
            ) -> Self::WriteGuard{
                <$sub as SplitedLazySequentializer<T>>::init_then_write_guard(
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
    (@deref $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)? $(, Static: $assert_static:ident)?) => {
        impl<T, G> Deref for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            type Target = T;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                &*self.__private
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
                &mut *self.__private
            }
        }
    };
    (@deref_static $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)? $(, Static: $assert_static:ident)?) => {
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
                 let st: &'static GenericLazy<$data, G, $man, $checker> = unsafe{&*(&self.__private as *const _)};
                 GenericLazy::init(st);
                 unsafe{&* GenericLazy::get_raw_data(&self.__private).get()}
            }
        }

    };
    (@deref_global $tp:ident, $man:ty, $checker:ty, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G> Deref for $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            type Target = T;
            #[inline(always)]
            fn deref(&self) -> &Self::Target {
                if inited::global_inited_hint() {
                    // SAFETY The object is initialized a program start-up as long
                    // as it is constructed through the macros #[dynamic(quasi_lazy)]
                    unsafe{&* (GenericLazy::get_raw_data(&self.__private).get())}
                    } else {
                        // SAFETY The object is required to have 'static lifetime by construction
                        let st: &'static GenericLazy<$data, G, $man, $checker> = unsafe{&*(&self.__private as *const _)};
                        GenericLazy::init(st);
                        unsafe{&* GenericLazy::get_raw_data(&self.__private).get()}
                    }
                }
            }

        //impl<T, G> DerefMut for $tp<T, G>
        //where $data: 'static + LazyData<Target=T>,
        //G: 'static + Generator<T>,
        //$(G:$trg, T:Sync,)?
        //$(T:$tr,)?
        //{
        //    #[inline(always)]
        //    fn deref_mut(&mut self) -> &mut Self::Target {
        //        if inited::global_inited_hint() {
        //            unsafe{&mut * (GenericLazy::get_raw_data(&self.__private).get())}
        //            } else {
        //            &mut *self.__private
        //        }
        //    }
        //}
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
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericLazy::new_static(f, <$man>::new(),<$data>::INIT)},
                }
            }
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericLazy::new_static_with_info(f, <$man>::new(), <$data>::INIT,info)},
                }
            }
        }

        impl<T, G> $tp<T, G>
        where $data: 'static + LazyData<Target=T>,
        G: 'static + Generator<T>,
        $(G:$trg, T:Sync,)?
        $(T:$tr,)?
        {
            //TODO: method => associated function
            #[inline(always)]
            pub fn phase(this: &Self) -> Phase {
                Phased::phase(Sequential::sequentializer(&this.__private))
            }
            #[inline(always)]
            pub fn init(this: &$($static)? Self) {
                GenericLazy::init(&this.__private);
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

use core::fmt::{self,Debug,Formatter};
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
    ($tp:ident, $data:ty $(,T: $tr: ident)?$(,G: $trg:ident)?) => {
        impl<T, G>  $tp<T, G> {
            pub const fn new(g: G) -> Self {
                Self::new_static(g)
            }
        }
        impl<T:Default> Default for $tp<T, fn()->T>
        {
            fn default() -> Self {
                Self::new(T::default)
            }
        }
    }
}
non_static_impls!{Lazy,UnInited::<T>}
non_static_debug!{Lazy,UnInited::<T>}
non_static_impls!{UnSyncLazy,UnInited::<T>}
non_static_debug!{UnSyncLazy,UnInited::<T>}

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
            pub fn read_lock(&$($static)? self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
               ReadGuard(GenericMutLazy::init_then_read_lock(&self.__private))
            }
            #[inline(always)]
            pub fn write_lock(&$($static)? self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
               WriteGuard(GenericMutLazy::init_then_write_lock(&self.__private))
            }
            #[inline(always)]
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
            pub fn read_lock(&self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                let st :&'static Self = unsafe {&*(self as *const _)};
                ReadGuard(GenericMutLazy::init_then_read_lock(&st.__private))
            }
            #[inline(always)]
            pub fn write_lock(&self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                let st :&'static Self = unsafe {&*(self as *const _)};
                WriteGuard(GenericMutLazy::init_then_write_lock(&st.__private))
            }
            #[inline(always)]
            pub fn init(&self) {
                let st :&'static Self = unsafe {&*(self as *const _)};
                GenericMutLazy::init_then_write_lock(&st.__private);
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
            pub fn read_lock(&'static self) -> ReadGuard<$gd::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                if inited::global_inited_hint() {
                    let l =GenericMutLazy::read_lock(&self.__private);
                    assert!(<$checker>::initialized_ok($gd::phase(&l)));
                    ReadGuard(l)
                    } else {
                    ReadGuard(GenericMutLazy::init_then_read_lock(&self.__private))
                }
            }
            #[inline(always)]
            pub fn write_lock(&'static self) -> WriteGuard<$gdw::<'_,GenericMutLazy<$data, G, $man, $checker>>> {
                if inited::global_inited_hint() {
                    let l = GenericMutLazy::write_lock(&self.__private);
                    assert!(<$checker>::initialized_ok($gdw::phase(&l)));
                    WriteGuard(l)
                    } else {
                    WriteGuard(GenericMutLazy::init_then_write_lock(&self.__private))
                }
            }
            #[inline(always)]
            pub fn init(&'static self) {
                GenericMutLazy::init_then_write_lock(&self.__private);
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
            pub const $($safe)? fn new_static(f: G) -> Self {
                #[allow(unused_unsafe)]
                Self {

                    __private: unsafe{GenericMutLazy::new_static(f, <$man>::new(),<$data>::INIT)},
                }
            }
            pub const $($safe)?  fn new_static_with_info(f: G, info: StaticInfo) -> Self {
                #[allow(unused_unsafe)]
                Self {
                    __private: unsafe{GenericMutLazy::new_static_with_info(f, <$man>::new(), <$data>::INIT,info)},
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
                    write!(f,"{:?}",*self.read_lock())
                }
            }
        }
    }
}
non_static_impls!{MutLazy,UnInited::<T>}
non_static_mut_debug!{MutLazy,UnInited::<T>}
non_static_impls!{UnSyncMutLazy,UnInited::<T>}
non_static_mut_debug!{UnSyncMutLazy,UnInited::<T>}

impl<T, G> Drop for MutLazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericMutLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe {
                (&*self.__private)
                    .get()
                    .drop_in_place()
            }
        }
    }
}
impl<T, G> Drop for UnSyncMutLazy<T, G> {
    fn drop(&mut self) {
        if Phased::phase(GenericMutLazy::sequentializer(&self.__private))
            .intersects(Phase::INITIALIZED)
        {
            unsafe {
                (&*self.__private)
                    .get()
                    .drop_in_place()
            }
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

#[cfg(feature="test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_lazy {
    use super::QuasiLazy;
    static _X: QuasiLazy<u32, fn() -> u32> = unsafe {
        QuasiLazy::new_static(|| {
            22
        })
    };
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
#[cfg(feature="test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_lazy_finalize {
    use crate::Finaly;
    use super::QuasiLazyFinalize;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: QuasiLazyFinalize<A, fn() -> A> =
        unsafe { QuasiLazyFinalize::new_static(|| A(22)) };
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
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
    }
}
#[cfg(feature="test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy {
    use super::QuasiMutLazy;
    static _X: QuasiMutLazy<u32, fn() -> u32> = unsafe{QuasiMutLazy::new_static(|| 22)};
    #[test]
    fn test() {
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
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
        assert!((*_X.read_lock()).0 == 22);
        *_X.write_lock() = A(33);
        assert_eq!((*_X.read_lock()).0, 33);
    }
}
#[cfg(feature="test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy_finalize {
    use super::QuasiMutLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    static _X: QuasiMutLazyFinalize<A, fn() -> A> = unsafe{QuasiMutLazyFinalize::new_static(|| A(22))};
    #[test]
    fn test() {
        assert!((*_X.read_lock()).0 == 22);
        *_X.write_lock() = A(33);
        assert_eq!((*_X.read_lock()).0, 33);
    }
}
#[cfg(test)]
mod test_mut_lazy_dropped {
    use super::MutLazyDroped;
    static _X: MutLazyDroped<u32, fn() -> u32> = MutLazyDroped::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
    }
}
#[cfg(feature="test_no_global_lazy_hint")]
#[cfg(test)]
mod test_quasi_mut_lazy_dropped {
    use super::QuasiMutLazyDroped;
    static _X: QuasiMutLazyDroped<u32, fn() -> u32> = unsafe{QuasiMutLazyDroped::new_static(|| 22)};
    #[test]
    fn test() {
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
    }
}
#[cfg(test)]
#[cfg(feature="thread_local")]
mod test_unsync_mut_lazy {
    use super::UnSyncMutLazy;
    #[thread_local]
    static _X: UnSyncMutLazy<u32, fn() -> u32> = UnSyncMutLazy::new_static(|| 22);
    #[test]
    fn test() {
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
    }
}
#[cfg(test)]
#[cfg(feature="thread_local")]
mod test_unsync_mut_lazy_finalize {
    use super::UnSyncMutLazyFinalize;
    use crate::Finaly;
    #[derive(Debug)]
    struct A(u32);
    impl Finaly for A {
        fn finaly(&self) {}
    }
    #[thread_local]
    static _X: UnSyncMutLazyFinalize<A, fn() -> A> = unsafe{UnSyncMutLazyFinalize::new_static(|| A(22))};
    #[test]
    fn test() {
        assert!((*_X.read_lock()).0 == 22);
        *_X.write_lock() = A(33);
        assert_eq!((*_X.read_lock()).0, 33);
    }
}
#[cfg(test)]
#[cfg(feature="thread_local")]
mod test_unsync_mut_lazy_droped {
    use super::UnSyncMutLazyDroped;
    #[thread_local]
    static _X: UnSyncMutLazyDroped<u32, fn() -> u32> = unsafe{UnSyncMutLazyDroped::new_static(|| 22)};
    #[test]
    fn test() {
        assert_eq!(*_X.read_lock(), 22);
        *_X.write_lock() = 33;
        assert_eq!(*_X.read_lock(), 33);
    }
}
