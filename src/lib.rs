// Copyright 2021 Olivier Kannengieser
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![cfg_attr(not(feature = "lazy"), no_std)]
#![allow(clippy::missing_safety_doc)]
//! Non const static initialization, and program constructor/destructor code.
//!
//! # Lesser Lazy Statics
//!
//! This crate provides *lazy statics* on all plateforms.
//!
//! On unixes and windows *lesser lazy statics* are *lazy* during program startup phase
//! (before `main` is called). Once main is called, those statics are all guaranteed to be
//! initialized and any access to them is as fast as any access to regular const initialized
//! statics. Benches sho that usual lazy statics, as those provided by `std::lazy::*` or from
//! [lazy_static][1] crate, suffer from a 2ns access penalty.
//!
//! *Lesser lazy statics* can optionaly be dropped at program destruction
//! (after main exit but before the program stops).
//!
//! *Lesser lazy statics* require the standard library and are enabled by default
//! crate features `lazy` and `lazy_drop`.
//! ```rust
//! use static_init::{dynamic};
//!
//! #[dynamic(lazy)]
//! static L1: Vec<i32> = vec![1,2,3];
//!
//! #[dynamic(lazy,drop)]
//! static mut L2: Vec<i32> = L1.clone();
//! #
//! # assert_eq!(L1[0], 1);
//! # unsafe {
//! #     assert_eq!(L2[1], 2);
//! #     L2[1] = 42;
//! #     assert_eq!(L2[1], 42);
//! #     }
//! #     
//! ```
//! # Dynamic statics: statics initialized at program startup
//!
//! On plateforms that support it (unixes, mac, windows), this crate provides *dynamic statics*: statics that are
//! initialized at program startup. This feature is `no_std`.
//!
//! ```rust
//! use static_init::{dynamic};
//!
//! #[dynamic]
//! static D1: Vec<i32> = unsafe {vec![1,2,3]};
//! #
//! # assert_eq!(D1[0], 1);
//! ```
//! As can be seen above, the initializer expression of those statics must be an unsafe
//! block. The reason is that during startup phase, accesses to *dynamic statics* may cause
//! *undefined behavior*: *dynamic statics* may be in a zero initialized state.
//!
//! To prevent such hazardeous accesses, on unixes and window plateforms, a priority can be
//! specified. Dynamic static initializations with higher priority are sequenced before dynamic
//! static initializations with lower priority. Dynamic static initializations with the same
//! priority are underterminately sequenced.
//!
//! ```rust
//! use static_init::{dynamic};
//!
//! // D2 initialization is sequenced before D1 initialization
//! #[dynamic]
//! static mut D1: Vec<i32> = unsafe {D2.clone()};
//!
//! #[dynamic(10)]
//! static D2: Vec<i32> = unsafe {vec![1,2,3]};
//! #
//! # unsafe{assert_eq!(D1[0], 1)};
//! ```
//!
//! *Dynamic statics* can be dropped at program destruction phase: they are dropped after main
//! exit:
//!
//! ```rust
//! use static_init::{dynamic};
//!
//! // D2 initialization is sequenced before D1 initialization
//! // D1 drop is sequenced before D2 drop.
//! #[dynamic(init,drop)]
//! static mut D1: Vec<i32> = unsafe {D2.clone()};
//!
//! #[dynamic(10,drop)]
//! static D2: Vec<i32> = unsafe {vec![1,2,3]};
//! ```
//! The priority act on drop in reverse order. *Dynamic statics* drops with a lower priority are
//! sequenced before *dynamic statics* drops with higher priority.
//!
//! # Constructor and Destructor
//!
//! On plateforms that support it (unixes, mac, windows), this crate provides a way to declare
//! *constructors*: a function called before main is called. This feature is `no_std`.
//!
//! ```rust
//! use static_init::{constructor};
//!
//! //called before main
//! #[constructor]
//! unsafe extern "C" fn some_init() {}
//! ```
//!
//! Constructors also support priorities. Sequencement rules applies also between constructor calls and
//! between *dynamic statics* initialization and *constructor* calls.
//!
//! *destructors* are called at program destruction. They also support priorities.
//!
//! ```rust
//! use static_init::{constructor, destructor};
//!
//! //called before some_init
//! #[constructor(10)]
//! unsafe extern "C" fn pre_init() {}
//!
//! //called before main
//! #[constructor]
//! unsafe extern "C" fn some_init() {}
//!
//! //called after main
//! #[destructor]
//! unsafe extern "C" fn first_destructor() {}
//!
//! //called after first_destructor
//! #[destructor(10)]
//! unsafe extern "C" fn last_destructor() {}
//! ```
//!
//! # Debuging initialization order
//!
//! If the feature `debug_order` or `debug_core` is enabled or when the crate is compiled with `debug_assertions`,
//! attempts to access `dynamic statics` that are uninitialized or whose initialization is
//! undeterminately sequenced with the access will cause a panic with a message specifying which
//! statics was tentatively accessed and how to change this *dynamic static* priority to fix this
//! issue.
//!
//! Run `cargo test` in this crate directory to see message examples.
//!
//! All implementations of lazy statics may suffer from circular initialization dependencies. Those
//! circular dependencies will cause either a dead lock or an infinite loop. If the feature `debug_lazy` or `debug_order` is
//! enabled, atemp are made to detect those circular dependencies. In most case they will be detected.
//!
//! # Thread Local Support
//!
//! Variable declared with `#[dynamic(lazy)]` can also be declared `#[thread_local]`. These
//! variable will behave as regular *lazy statics*.
//! ```ignore
//! #[thread_local]
//! #[dynamic(lazy)]
//! static mut X: Vec<i32> = vec![1,2,3];
//! ```
//! These variables can also be droped on thread exit.
//! ```ignore
//! #[thread_local]
//! #[dynamic(lazy,drop)]
//! static X: Vec<i32> = vec![1,2,3];
//!
//! assert!(unsafe{X[1] == 2});
//! ```
//!
//! Accessing a thread local *lazy statics* that should drop during the phase where thread_locals are
//! droped may cause *undefined behavior*. For this reason any access to a thread local lazy static
//! that is dropped will require an unsafe block, even if the static is const.
//!
//! [1]: https://crates.io/crates/lazy_static

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
mod details {}

use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

#[doc(inline)]
pub use static_init_macro::constructor;

#[doc(inline)]
pub use static_init_macro::destructor;

#[doc(inline)]
pub use static_init_macro::dynamic;

union StaticBase<T> {
    k: (),
    v: ManuallyDrop<T>,
}

#[derive(Debug)]
#[doc(hidden)]
pub struct StaticInfo {
    pub variable_name: &'static str,
    pub file_name:     &'static str,
    pub line:          u32,
    pub column:        u32,
    pub init_priority: i32,
    pub drop_priority: i32,
}

#[cfg(any(feature = "debug_core", debug_assertions))]
use core::sync::atomic::{AtomicI32, Ordering};

#[cfg(any(feature = "debug_core", debug_assertions))]
static CUR_INIT_PRIO: AtomicI32 = AtomicI32::new(i32::MIN);

#[cfg(any(feature = "debug_core", debug_assertions))]
static CUR_DROP_PRIO: AtomicI32 = AtomicI32::new(i32::MIN);

/// The actual type of mutable *dynamic statics*.
///
/// It implements `Deref<Target=T>` and `DerefMut`.
///
/// All associated functions are only usefull for the implementation of
/// the `dynamic` proc macro attribute
pub struct Static<T>(
    StaticBase<T>,
    #[cfg(any(feature = "debug_core", debug_assertions))] StaticInfo,
    #[cfg(any(feature = "debug_core", debug_assertions))] AtomicI32,
);

#[cfg(any(feature = "debug_core", debug_assertions))]
#[doc(hidden)]
#[inline]
pub fn __set_init_prio(v: i32) {
    CUR_INIT_PRIO.store(v, Ordering::Relaxed);
}
#[cfg(not(any(feature = "debug_core", debug_assertions)))]
#[doc(hidden)]
#[inline(always)]
pub fn __set_init_prio(_: i32) {}

//As a trait in order to avoid noise;
impl<T> Static<T> {
    #[inline]
    pub const fn uninit(_info: StaticInfo) -> Self {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        {
            Self(StaticBase { k: () }, _info, AtomicI32::new(0))
        }
        #[cfg(not(any(feature = "debug_core", debug_assertions)))]
        {
            Self(StaticBase { k: () })
        }
    }
    #[inline]
    pub const fn from(v: T, _info: StaticInfo) -> Self {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        {
            Static(
                StaticBase {
                    v: ManuallyDrop::new(v),
                },
                _info,
                AtomicI32::new(1),
            )
        }
        #[cfg(not(any(feature = "debug_core", debug_assertions)))]
        {
            Static(StaticBase {
                v: ManuallyDrop::new(v),
            })
        }
    }

    #[inline]
    pub unsafe fn set_to(this: &mut Self, v: T) {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        {
            this.0.v = ManuallyDrop::new(v);
            this.2.store(1, Ordering::Relaxed);
        }
        #[cfg(not(any(feature = "debug_core", debug_assertions)))]
        {
            this.0.v = ManuallyDrop::new(v);
        }
    }

    #[inline]
    pub unsafe fn drop(this: &mut Self) {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        {
            CUR_DROP_PRIO.store(this.1.drop_priority, Ordering::Relaxed);
            ManuallyDrop::drop(&mut this.0.v);
            CUR_DROP_PRIO.store(i32::MIN, Ordering::Relaxed);
            this.2.store(2, Ordering::Relaxed);
        }
        #[cfg(not(any(feature = "debug_core", debug_assertions)))]
        {
            ManuallyDrop::drop(&mut this.0.v);
        }
    }
}

#[cfg(any(feature = "debug_core", debug_assertions))]
#[inline]
fn check_access(info: &StaticInfo, status: i32) {
    if status == 0 {
        core::panic!(
            "Attempt to access variable {:#?} before it is initialized during initialization \
             priority {}. Tip: increase init priority of this static to a value larger than \
             {prio} (attribute syntax: `#[dynamic(init=<prio>)]`)",
            info,
            prio = CUR_INIT_PRIO.load(Ordering::Relaxed)
        )
    }
    if status == 2 {
        core::panic!(
            "Attempt to access variable {:#?} after it was destroyed during destruction priority \
             {prio}. Tip increase drop priority of this static to a value larger than {prio} \
             (attribute syntax: `#[dynamic(drop=<prio>)]`)",
            info,
            prio = CUR_DROP_PRIO.load(Ordering::Relaxed)
        )
    }
    let init_prio = CUR_INIT_PRIO.load(Ordering::Relaxed);
    let drop_prio = CUR_DROP_PRIO.load(Ordering::Relaxed);
    if init_prio == info.init_priority {
        core::panic!(
            "This access to variable {:#?} is not sequenced after construction of this static. \
             Tip increase init priority of this static to a value larger than {prio} (attribute \
             syntax: `#[dynamic(init=<prio>)]`)",
            info,
            prio = init_prio
        )
    }
    if drop_prio == info.drop_priority {
        core::panic!(
            "This access to variable {:#?} is not sequenced before to its drop. Tip increase drop \
             priority of this static to a value larger than {prio} (attribute syntax: \
             `#[dynamic(drop=<prio>)]`)",
            info,
            prio = drop_prio
        )
    }
    if !(drop_prio < info.drop_priority || init_prio < info.init_priority) {
        core::panic!(
            "Unexpected initialization order while accessing {:#?} from init priority {} and drop \
             priority {}. This is a bug of `static_init` library, please report \"
           the issue inside `static_init` repository.",
            info,
            init_prio,
            drop_prio
        )
    }
}

impl<T> Deref for Static<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        check_access(&self.1, self.2.load(Ordering::Relaxed));
        unsafe { &*self.0.v }
    }
}
impl<T> DerefMut for Static<T> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut T {
        #[cfg(any(feature = "debug_core", debug_assertions))]
        check_access(&self.1, self.2.load(Ordering::Relaxed));
        unsafe { &mut *self.0.v }
    }
}

/// The actual type of non mutable *dynamic statics*.
///
/// It implements `Deref<Target=T>`.
///
/// All associated functions are only usefull for the implementation of
/// the `dynamic` proc macro attribute
pub struct ConstStatic<T>(UnsafeCell<Static<T>>);

impl<T> ConstStatic<T> {
    #[inline]
    pub const fn uninit(info: StaticInfo) -> Self {
        Self(UnsafeCell::new(Static::uninit(info)))
    }
    #[inline]
    pub const fn from(v: T, info: StaticInfo) -> Self {
        Self(UnsafeCell::new(Static::from(v, info)))
    }
    #[inline]
    pub unsafe fn set_to(this: &Self, v: T) {
        Static::set_to(&mut (*this.0.get()), v)
    }
    #[inline]
    pub unsafe fn drop(this: &Self) {
        Static::drop(&mut *this.0.get());
    }
}

unsafe impl<T: Send> Send for ConstStatic<T> {}
unsafe impl<T: Sync> Sync for ConstStatic<T> {}

impl<T> Deref for ConstStatic<T> {
    type Target = T;
    #[inline(always)]
    fn deref(&self) -> &T {
        unsafe { &**self.0.get() }
    }
}

#[cfg(feature = "lazy")]
mod global_lazy {
    use super::StaticInfo;
    use core::cell::Cell;
    use core::cell::UnsafeCell;
    use core::fmt;
    use core::mem::MaybeUninit;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::Ordering;

    #[cfg(not(feature = "debug_lazy"))]
    use core::hint::unreachable_unchecked;

    #[cfg(not(feature = "debug_lazy"))]
    use parking_lot::Once;

    #[cfg(feature = "debug_lazy")]
    use parking_lot::{
        lock_api::GetThreadId, lock_api::RawMutex as _, RawMutex, RawThreadId, ReentrantMutex,
    };

    #[cfg(feature = "debug_lazy")]
    use core::sync::atomic::AtomicBool;

    #[cfg(feature = "debug_lazy")]
    use core::num::NonZeroUsize;

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "emscripten",
        target_os = "haiku",
        target_os = "l4re",
        target_os = "fuchsia",
        target_os = "redox",
        target_os = "vxworks",
        target_os = "windows"
    ))]
    mod inited {

        use core::sync::atomic::{AtomicBool, Ordering};

        pub static LAZY_INIT_ENSURED: AtomicBool = AtomicBool::new(false);

        #[static_init_macro::constructor(__lazy_init_finished)]
        unsafe extern "C" fn mark_inited() {
            LAZY_INIT_ENSURED.store(true, Ordering::Release);
        }
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "emscripten",
        target_os = "haiku",
        target_os = "l4re",
        target_os = "fuchsia",
        target_os = "redox",
        target_os = "vxworks",
        target_os = "windows"
    ))]
    use inited::LAZY_INIT_ENSURED;

    #[cfg(not(any(
        target_os = "linux",
        target_os = "android",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd",
        target_os = "solaris",
        target_os = "illumos",
        target_os = "emscripten",
        target_os = "haiku",
        target_os = "l4re",
        target_os = "fuchsia",
        target_os = "redox",
        target_os = "vxworks",
        target_os = "windows"
    )))]
    const LAZY_INIT_ENSURED: bool = false;

    #[cfg(feature = "debug_lazy")]
    struct DebugLazyState<F> {
        initer:   Cell<Option<NonZeroUsize>>,
        function: Cell<Option<F>>,
    }

    /// The type of *lazy statics*.
    ///
    /// Statics that are initialized on first access.
    pub struct Lazy<T, F = fn() -> T> {
        value:        UnsafeCell<MaybeUninit<T>>,
        #[cfg(not(feature = "debug_lazy"))]
        initer:       Once,
        #[cfg(not(feature = "debug_lazy"))]
        init_exp:     Cell<Option<F>>,
        #[cfg(feature = "debug_lazy")]
        inited:       AtomicBool,
        #[cfg(feature = "debug_lazy")]
        debug_initer: ReentrantMutex<DebugLazyState<F>>,
        #[cfg(feature = "debug_lazy")]
        info:         Option<StaticInfo>,
    }

    /// The type of *lesser lazy statics*.
    ///
    /// For statics that are initialized either on first access
    /// or just before `main` is called.
    #[derive(Debug)]
    pub struct GlobalLazy<T, F = fn() -> T>(Lazy<T, F>);

    /// The type of const thread local *lazy statics* that are dropped.
    ///
    /// For statics that are initialized on first access. Only
    /// providing const access to the underlying data, the are
    /// intended to be declare mutable so that all access to them
    /// requires an unsafe block.
    #[derive(Debug)]
    pub struct ConstLazy<T, F = fn() -> T>(Lazy<T, F>);

    struct DestructorRegister(UnsafeCell<Option<Vec<fn()>>>);

    impl Drop for DestructorRegister {
        fn drop(&mut self) {
            if let Some(vec) = unsafe { (*self.0.get()).take() } {
                for f in vec {
                    f()
                }
            }
        }
    }

    unsafe impl Sync for DestructorRegister {}

    thread_local! {
        static DESTRUCTORS: DestructorRegister = DestructorRegister(UnsafeCell::new(None));
    }

    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn __touch_tls_destructors() {
        DESTRUCTORS.with(|d| {
            if (*d.0.get()).is_none() {
                *d.0.get() = Some(vec![])
            }
        })
    }

    #[doc(hidden)]
    #[inline(always)]
    pub unsafe fn __push_tls_destructor(f: fn()) {
        DESTRUCTORS.with(|d| (*d.0.get()).as_mut().unwrap().push(f))
    }

    impl<T: fmt::Debug, F> fmt::Debug for Lazy<T, F> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.debug_struct("Lazy")
                .field("cell", &self.value)
                .field("init", &"..")
                .finish()
        }
    }

    impl<T: Default> Default for Lazy<T> {
        fn default() -> Self {
            Self::new(Default::default)
        }
    }

    impl<T, F> Lazy<T, F> {
        /// Initialize a lazy with a builder as argument.
        pub const fn new(f: F) -> Self {
            Self {
                value: UnsafeCell::new(MaybeUninit::uninit()),
                #[cfg(not(feature = "debug_lazy"))]
                initer: Once::new(),
                #[cfg(not(feature = "debug_lazy"))]
                init_exp: Cell::new(Some(f)),
                #[cfg(feature = "debug_lazy")]
                inited: AtomicBool::new(false),
                #[cfg(feature = "debug_lazy")]
                debug_initer: ReentrantMutex::const_new(
                    RawMutex::INIT,
                    RawThreadId::INIT,
                    DebugLazyState {
                        initer:   Cell::new(None),
                        function: Cell::new(Some(f)),
                    },
                ),
                #[cfg(feature = "debug_lazy")]
                info: None,
            }
        }

        /// Initialize a lazy with a builder as argument.
        ///
        /// This function is intended to be used internaly
        /// by the dynamic macro.
        pub const fn new_with_info(f: F, _info: StaticInfo) -> Self {
            Self {
                value: UnsafeCell::new(MaybeUninit::uninit()),
                #[cfg(not(feature = "debug_lazy"))]
                initer: Once::new(),
                #[cfg(not(feature = "debug_lazy"))]
                init_exp: Cell::new(Some(f)),
                #[cfg(feature = "debug_lazy")]
                inited: AtomicBool::new(false),
                #[cfg(feature = "debug_lazy")]
                debug_initer: ReentrantMutex::const_new(
                    RawMutex::INIT,
                    RawThreadId::INIT,
                    DebugLazyState {
                        initer:   Cell::new(None),
                        function: Cell::new(Some(f)),
                    },
                ),
                #[cfg(feature = "debug_lazy")]
                info: Some(_info),
            }
        }

        /// Return a pointer to the value.
        ///
        /// The value may be in an uninitialized state.
        #[inline(always)]
        pub const fn as_mut_ptr(this: &Self) -> *mut T {
            this.value.get() as *mut T
        }

        /// Ensure the value is initialized without optimization check
        ///
        /// This is intended to be used at program start up by
        /// the dynamic macro.
        #[inline(always)]
        pub fn __do_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            Lazy::ensure_init(this)
        }
        /// Ensure the value is initialized without optimization check
        ///
        /// Once this function is called, it is guaranteed that
        /// the value is in an initialized state.
        ///
        /// This function is always called when the lazy is dereferenced.
        #[inline(always)]
        pub fn ensure_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            //The compiler fails to automatically choose
            //which branch is the best one...
            #[cfg(not(feature = "debug_lazy"))]
            this.initer.call_once(|| unsafe {
                (*this.value.get()).as_mut_ptr().write(this
                    .init_exp
                    .take()
                    .unwrap_or_else(|| unreachable_unchecked())(
                ));
            });
            #[cfg(feature = "debug_lazy")]
            if !this.inited.load(Ordering::Acquire) {
                let l = this.debug_initer.lock();
                if let Some(initer) = l.initer.get() {
                    if initer == RawThreadId.nonzero_thread_id() {
                        if let Some(info) = &this.info {
                            core::panic!("Recurcive lazy initialization of {:#?}.", info);
                        } else {
                            core::panic!("Recurcive lazy initialization.");
                        }
                    }
                    return;
                } else {
                    l.initer.set(Some(RawThreadId.nonzero_thread_id()));
                    unsafe {
                        (*this.value.get())
                            .as_mut_ptr()
                            .write(l.function.take().unwrap()())
                    };
                    this.inited.store(true, Ordering::Release);
                }
            }
        }
    }

    unsafe impl<F, T: Send + Sync> Send for Lazy<T, F> {}

    unsafe impl<F, T: Sync> Sync for Lazy<T, F> {}

    impl<T, F> Deref for Lazy<T, F>
    where
        F: FnOnce() -> T,
    {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            unsafe {
                Lazy::ensure_init(self);
                &*Lazy::as_mut_ptr(self)
            }
        }
    }
    impl<T, F> DerefMut for Lazy<T, F>
    where
        F: FnOnce() -> T,
    {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            unsafe {
                Lazy::ensure_init(self);
                &mut *Lazy::as_mut_ptr(self)
            }
        }
    }

    impl<T, F> GlobalLazy<T, F> {
        /// Initialize a lazy with a builder as argument.
        ///
        /// # Safety
        ///
        /// This variable shall not be used as a thread_local
        /// statics or within the state of a thread_local static
        pub const unsafe fn new(f: F) -> Self {
            Self(Lazy::new(f))
        }

        /// Initialize a lazy with a builder as argument.
        ///
        /// This function is intended to be used internaly
        /// by the dynamic macro.
        ///
        /// # Safety
        ///
        /// This variable shall not be used as a thread_local
        /// statics or within the state of a thread_local static
        pub const unsafe fn new_with_info(f: F, info: StaticInfo) -> Self {
            Self(Lazy::new_with_info(f, info))
        }

        /// Return a pointer to the value.
        ///
        /// The value may be in an uninitialized state.
        #[inline(always)]
        pub const fn as_mut_ptr(this: &Self) -> *mut T {
            Lazy::as_mut_ptr(&this.0)
        }
        /// Ensure the value is initialized without optimization check
        ///
        /// This is intended to be used at program start up by
        /// the dynamic macro.
        #[inline(always)]
        pub fn __do_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            Lazy::ensure_init(&this.0)
        }
        /// Ensure the value is initialized
        ///
        /// Once this function is called, it is guaranteed that
        /// the value is in an initialized state.
        ///
        /// This function is always called when the lazy is dereferenced.
        #[inline(always)]
        pub fn ensure_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            if !LAZY_INIT_ENSURED.load(Ordering::Acquire) {
                Self::__do_init(this);
            }
        }
    }

    impl<T, F> Deref for GlobalLazy<T, F>
    where
        F: FnOnce() -> T,
    {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            unsafe {
                GlobalLazy::ensure_init(self);
                &*GlobalLazy::as_mut_ptr(self)
            }
        }
    }
    impl<T, F> DerefMut for GlobalLazy<T, F>
    where
        F: FnOnce() -> T,
    {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            unsafe {
                GlobalLazy::ensure_init(self);
                &mut *GlobalLazy::as_mut_ptr(self)
            }
        }
    }

    impl<T, F> ConstLazy<T, F> {
        /// Initialize a lazy with a builder as argument.
        ///
        /// # Safety
        ///
        /// This variable shall not be used as a thread_local
        /// statics or within the state of a thread_local static
        pub const unsafe fn new(f: F) -> Self {
            Self(Lazy::new(f))
        }

        /// Initialize a lazy with a builder as argument.
        ///
        /// This function is intended to be used internaly
        /// by the dynamic macro.
        ///
        /// # Safety
        ///
        /// This variable shall not be used as a thread_local
        /// statics or within the state of a thread_local static
        pub const unsafe fn new_with_info(f: F, info: StaticInfo) -> Self {
            Self(Lazy::new_with_info(f, info))
        }

        /// Return a pointer to the value.
        ///
        /// The value may be in an uninitialized state.
        #[inline(always)]
        pub const fn as_mut_ptr(this: &Self) -> *mut T {
            Lazy::as_mut_ptr(&this.0)
        }
        /// Ensure the value is initialized without optimization check
        ///
        /// This is intended to be used at program start up by
        /// the dynamic macro.
        #[inline(always)]
        pub fn __do_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            Lazy::ensure_init(&this.0)
        }
        /// Ensure the value is initialized
        ///
        /// Once this function is called, it is guaranteed that
        /// the value is in an initialized state.
        ///
        /// This function is always called when the lazy is dereferenced.
        #[inline(always)]
        pub fn ensure_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            Self::__do_init(this);
        }
    }

    impl<T, F> Deref for ConstLazy<T, F>
    where
        F: FnOnce() -> T,
    {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            unsafe {
                ConstLazy::ensure_init(self);
                &*ConstLazy::as_mut_ptr(self)
            }
        }
    }
}

#[cfg(feature = "lazy")]
pub use global_lazy::{ConstLazy, GlobalLazy, Lazy, __touch_tls_destructors, __push_tls_destructor};
