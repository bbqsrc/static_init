// Copyright 2021 Olivier Kannengieser
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![cfg_attr(not(any(feature = "parking_lot_core", debug_mode)), no_std)]
#![cfg_attr(all(elf, feature = "thread_local"), feature(linkage))]
#![cfg_attr(
    feature = "thread_local",
    feature(thread_local),
    feature(cfg_target_thread_local)
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[doc(hidden)]
/// # Details and implementation documentation.
///
/// ## Mac
///   - [MACH_O specification](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
///   - GCC source code gcc/config/darwin.c indicates that priorities are not supported.
///
///   Initialization functions pointers are placed in section "__DATA,__mod_init_func" and
///   "__DATA,__mod_term_func"
///
///   std::env is not initialized in any constructor.
///
/// ## ELF plateforms:
///  - `info ld`
///  - linker script: `ld --verbose`
///  - [ELF specification](https://docs.oracle.com/cd/E23824_01/html/819-0690/chapter7-1.html#scrolltoc)
///
///  The runtime will run fonctions pointers of section ".init_array" at startup and function
///  pointers in ".fini_array" at program exit. The linker place in the target object file
///  sectio .init_array all sections from the source objects whose name is of the form
///  .init_array.NNNNN in lexicographical order then the .init_array sections of those same source
///  objects. It does equivalently with .fini_array and .fini_array.NNNN sections.
///
///  Usage can be seen in gcc source gcc/config/pru.c
///
///  Resources of libstdc++ are initialized with priority 65535-100 (see gcc source libstdc++-v3/c++17/default_resource.h)
///  The rust standard library function that capture the environment and executable arguments is
///  executed at priority 65535-99 on gnu platform variants. On other elf plateform they are not accessbile in any constructors. Nevertheless
///  one can read into /proc/self directory to retrieve the command line.
///  Some callbacks constructors and destructors with priority 65535 are
///  registered by rust/rtlibrary.
///  Static C++ objects are usually initialized with no priority (TBC). lib-c resources are
///  initialized by the C-runtime before any function in the init_array (whatever the priority) are executed.
///
/// ## Windows
///
///   std::env is initialized before any constructors.
///
///  - [this blog post](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
///
///  At start up, any functions pointer between sections ".CRT$XIA" and ".CRT$XIZ"
///  and then any functions between ".CRT$XCA" and ".CRT$XCZ". It happens that the C library
///  initialization functions pointer are placed in ".CRT$XIU" and C++ statics functions initialization
///  pointers are placed in ".CRT$XCU". At program finish the pointers between sections
///  ".CRT$XPA" and ".CRT$XPZ" are run first then those between ".CRT$XTA" and ".CRT$XTZ".
///
///  Some reverse engineering was necessary to find out a way to implement
///  constructor/destructor priority.
///
///  Contrarily to what is reported in this blog post, msvc linker
///  only performs a lexicographicall ordering of section whose name
///  is of the form "\<prefix\>$\<suffix\>" and have the same \<prefix\>.
///  For example "RUST$01" and "RUST$02" will be ordered but those two
///  sections will not be ordered with "RHUM" section.
///
///  Moreover, it seems that section name of the form \<prefix\>$\<suffix\> are
///  not limited to 8 characters.
///
///  So static initialization function pointers are placed in section ".CRT$XCU" and
///  those with a priority `p` in `format!(".CRT$XCTZ{:05}",65535-p)`. Destructors without priority
///  are placed in ".CRT$XPU" and those with a priority in `format!(".CRT$XPTZ{:05}",65535-p)`.
///
mod details {}

/// A trait for objects that are intinded to transition between phasis.
///
/// A type that implement [`Sequential`] ensured that its `data` will traverse a sequence of
/// [phases](Phase). The phase does not participates to the value of the type. The phase describes
/// the *lifetime* of the object: initialized, droped,...
///
/// # Safety
///
/// The trait is unsafe because the implementor should ensure that the reference returned by
/// [`sequentializer`](Self::sequentializer) and the reference returned by [`data`](Self::data) refer to two subobject of a same object.
///
pub unsafe trait Sequential {
    type Data;
    type Sequentializer;
    fn sequentializer(this: &Self) -> &Self::Sequentializer;
    fn data(this: &Self) -> &Self::Data;
}

/// Trait for objects that know in which [phase](Phase) they are.
pub trait Phased {
    /// return the current phase
    fn phase(this: &Self) -> Phase;
}

impl<T> Phased for T
where
    T: Sequential,
    T::Sequentializer: Phased,
{
    fn phase(this: &Self) -> Phase {
        Phased::phase(Sequential::sequentializer(this))
    }
}

/// A type that implement Sequentializer aims at [phase](Phase) sequencement.
///
/// The method [`Sequential::sequentializer`] should return an object that implement
/// this trait.
///
/// # Safety
///
/// The trait is unsafe because the lock should ensure the following lock semantic:
///  - if the implementor also implement Sync, the read/write lock semantic should be synchronized
///  and if no lock is taken, the call to lock should synchronize with the end of the phase
///  transition that put the target object in its current phase.
///  - if the implementor is not Sync then the lock should panic if any attempt is made
///    to take another lock while a write lock is alive or to take a write lock while there
///    is already a read_lock.(the lock should behave as a RefCell).
pub unsafe trait Sequentializer<'a, T: Sequential>: 'static + Sized + Phased {
    type ReadGuard;
    type WriteGuard;
    /// Lock the phases of an object in order to ensure atomic phase transition.
    ///
    /// The nature of the lock depend on the phase in which is the object, and is determined
    /// by the `lock_nature` argument.
    fn lock(
        target: &'a T,
        lock_nature: impl Fn(Phase) -> LockNature,
    ) -> LockResult<Self::ReadGuard, Self::WriteGuard>;
}

/// A [`LazySequentializer`] sequentialize the [phases](Phase) of a target object to ensure
/// atomic initialization and finalization.
///
/// # Safety
///
/// The trait is unsafe because the implementor must ensure that:
///
///  - if the implementor also implement Sync, the read/write lock semantic should be synchronized
///  and if no lock is taken, the call to lock should synchronize with the end of the phase
///  transition that put the target object in its current phase.
///  - if the implementor is not Sync then the lock should panic if any attempt is made
///    to take another lock while a write lock is alive or to take a write lock while there
///    is already a read_lock.(the lock should behave as a RefCell).
pub unsafe trait LazySequentializer<'a, T: Sequential<Sequentializer = Self>>:
    Sequentializer<'a, T>
{
    /// if `shall_init` return true for the target [`Sequential`] object, it initialize
    /// the data of the target object using `init`
    ///
    /// The implementor may also proceed to registration of the finalizing method (drop)
    /// in order to drop the object on the occurence of singular event (thread exit, or program
    /// exit). If this registration fails and if `init_on_reg_failure` is `true` then the object
    /// will be initialized, otherwise it will not.
    fn init(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        init_on_reg_failure: bool,
    );
    /// Similar to [init](Self::init) but returns a lock that prevents the phase of the object
    /// to change (Read Lock). The returned lock may be shared.
    fn init_then_read_guard(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        init_on_reg_failure: bool,
    ) -> Self::ReadGuard;
    /// Similar to [init](Self::init) but returns a lock that prevents the phase of the object
    /// to change accepts through the returned lock guard (Write Lock). The lock is exculisive.
    fn init_then_write_guard(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        init_on_reg_failure: bool,
    ) -> Self::WriteGuard;
}

//TODO: doc here

/// A [SplitedLazySequentializer] sequentialize the [phase](Phase) of an object to
/// ensure atomic initialization and finalization.
///
/// A sequentializer that implement this trait is not able to register the finalization
/// for latter call on program exit or thread exit.
///
/// # Safety
///
/// The trait is unsafe because the implementor must ensure that:
///
///  - either the implementor is Sync and the initialization is performed atomically
///  - or the implementor is not Sync and any attempt to perform an initialization while
///    an initialization is running will cause a panic.
pub unsafe trait SplitedLazySequentializer<'a, T: Sequential>:
    Sequentializer<'a, T>
{
    /// if `shall_init` return true for the target [`Sequential`] object, it initialize
    /// the data of the target object using `init`
    ///
    /// Before initialization of the object, the fonction `reg` is call with the target
    /// object as argument. This function should proceed to registration of the
    /// [finalize_callback](Self::finalize_callback) for latter call at program exit or
    /// thread exit.
    fn init(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    );
    fn init_then_read_guard(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Self::ReadGuard;
    fn init_then_write_guard(
        target: &'a T,
        shall_init: impl Fn(Phase) -> bool,
        init: impl FnOnce(&'a <T as Sequential>::Data),
        reg: impl FnOnce(&'a T) -> bool,
        init_on_reg_failure: bool,
    ) -> Self::WriteGuard;
    /// A callback that is intened to be stored by the `reg` argument of `init` method.
    fn finalize_callback(s: &T, f: impl FnOnce(&T::Data));
}

/// Generates a value of type `T`
pub trait Generator<T> {
    fn generate(&self) -> T;
}

impl<U, T: Fn() -> U> Generator<U> for T {
    fn generate(&self) -> U {
        self()
    }
}

/// A Drop replacement that does not change the state of the object
pub trait Finaly {
    fn finaly(&self);
}

#[cfg_attr(docsrs, doc(cfg(debug_mode)))]
#[cfg(debug_mode)]
#[doc(hidden)]
#[derive(Debug)]
/// Used to passe errors
pub struct CyclicPanic;

/// phases and bits to manipulate them;
pub mod phase {
    use bitflags::bitflags;
    pub(crate) const WPARKED_BIT: u32 =  0b1000_0000_0000_0000_0000_0000_0000_0000;
    pub(crate) const PARKED_BIT: u32 =   0b0100_0000_0000_0000_0000_0000_0000_0000;
    pub(crate) const LOCKED_BIT: u32 =   0b0010_0000_0000_0000_0000_0000_0000_0000; //Or READER overflow
    pub(crate) const READER_BITS: u32 =  0b0000_1111_1111_1111_1111_1000_0000_0000;
    pub(crate) const READER_OVERF: u32 = 0b0001_0000_0000_0000_0000_0000_0000_0000;
    pub(crate) const READER_UNITY: u32 = 0b0000_0000_0000_0000_0000_1000_0000_0000;

    bitflags! {
        /// The lifetime phase of an object, this indicate weither the object was initialized
        /// finalized (droped),...
        ///
        /// The registration phase is a phase that precede the initialization phase and is meant
        /// to register a callback that will proceed to the finalization (drop) of the object at
        /// program exit or thread exit. Depending on the plateform this registration may fail.
        pub struct Phase: u32 {
            const INITIALIZED               = 0b0000_0000_0000_0000_0000_0000_0000_0001;
            const INITIALIZATION            = 0b0000_0000_0000_0000_0000_0000_0000_0010;
            const INITIALIZATION_PANICKED   = 0b0000_0000_0000_0000_0000_0000_0000_0100;
            const INITIALIZATION_SKIPED     = 0b0000_0000_0000_0000_0000_0000_0000_1000;

            const REGISTERED                = 0b0000_0000_0000_0000_0000_0000_0001_0000;
            const REGISTRATION              = 0b0000_0000_0000_0000_0000_0000_0010_0000;
            const REGISTRATION_PANICKED     = 0b0000_0000_0000_0000_0000_0000_0100_0000;
            const REGISTRATION_REFUSED      = 0b0000_0000_0000_0000_0000_0000_1000_0000;

            const FINALIZED                 = 0b0000_0000_0000_0000_0000_0001_0000_0000;
            const FINALIZATION              = 0b0000_0000_0000_0000_0000_0010_0000_0000;
            const FINALIZATION_PANICKED     = 0b0000_0000_0000_0000_0000_0100_0000_0000;
        }
    }
}
#[doc(inline)]
pub use phase::Phase;

#[doc(inline)]
pub use static_init_macro::constructor;

#[doc(inline)]
pub use static_init_macro::destructor;

#[doc(inline)]
pub use static_init_macro::dynamic;

/// Provides policy types for implementation of various lazily initialized types.
pub mod generic_lazy;

/// Provides two lazy sequentializers, one that is Sync, and the other that is not Sync, that are
/// able to sequentialize the target object initialization but cannot register its finalization
/// callback.
pub mod splited_sequentializer;

#[cfg(any(elf, mach_o, coff))]
/// Provides two lazy sequentializers, one that will finalize the target object at program exit and
/// the other at thread exit.
pub mod at_exit;

/// Provides various implementation of lazily initialized types
pub mod lazy;
#[doc(inline)]
pub use lazy::{MutLazy,Lazy};
#[doc(inline)]
pub use lazy::{UnSyncMutLazy,UnSyncLazy};

#[cfg(any(elf, mach_o, coff))]
/// Provides types for statics that are meant to run code before main start or after it exit.
pub mod raw_static;

/// Provides PhaseLockers, that are phase tagged *adaptative* read-write lock types: during the lock loop the nature of the lock that
/// is attempted to be taken variates depending on the phase.
///
/// The major difference with a RwLock is that decision to read lock, write lock are to not lock
/// is taken within the lock loop: on each attempt to take the lock (when unparking for exemple)
/// the mutex may change its locking strategy or abandon any further attempt to take the lock.
///
/// The algorithm is as efficient as parking_lot `RwLock` because it
/// is an adaptation of the algorithm provided by parking_lot::RwLock. (TODO: bring more parts
/// of parking_lot RwLock algorithm to those mutex)
pub mod mutex;
pub use mutex::{LockNature, LockResult, PhaseGuard};

#[derive(Debug)]
#[doc(hidden)]
pub enum InitMode {
    Const,
    Lazy,
    QuasiLazy,
    ProgramConstructor(u16),
}

#[derive(Debug)]
#[doc(hidden)]
pub enum FinalyMode {
    None,
    Drop,
    Finalize,
    ProgramDestructor(u16),
}

#[derive(Debug)]
#[doc(hidden)]
pub struct StaticInfo {
    pub variable_name: &'static str,
    pub file_name:     &'static str,
    pub line:          u32,
    pub column:        u32,
    pub init_mode:     InitMode,
    pub drop_mode:     FinalyMode,
}
