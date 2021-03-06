#![cfg_attr(not(feature = "lazy"), no_std)]
#![allow(clippy::missing_safety_doc)]
//! Module initialization termination function with priorities and (mutable) statics initialization with
//! non const functions.
//!
//!
//! # Functionalities
//! - Optimized version of lazy statics that does not undergoes access penalty (needs std support).
//! - Code execution before or after `main` but after libc and rust runtime has been initialized
//! (but not alway std::env see doc bellow, no_std)
//! - Mutable and const statics with non const initialization (no_std).
//! - Statics dropable after `main` exits (no_std).
//! - Zero cost access to statics (no_std).
//! - Priorities on elf plateforms (linux, bsd, etc...) and window (no_std).
//!
//! # Example
//! ```rust
//! use static_init::{constructor,destructor,dynamic};
//!
//! #[constructor]
//! unsafe extern "C" fn do_init(){
//! }
//! //Care not to use priorities above 65535-100
//! //as those high priorities are used by
//! //the rust runtime.
//! #[constructor(200)]
//! unsafe extern "C" fn do_first(){
//! }
//!
//! #[destructor]
//! unsafe extern "C" fn finaly() {
//! }
//! #[destructor(100)]
//! unsafe extern "C" fn ultimately() {
//! }
//!
//! #[dynamic(lazy)]
//! static L1: Vec<i32> = vec![1,2,3];
//!
//! #[dynamic(lazy,drop)]
//! static mut L2: Vec<i32> = L1.clone();
//!
//! #[dynamic]
//! static V1: Vec<i32> = unsafe {vec![1,2,3]};
//!
//! #[dynamic(init,drop)]
//! static mut V2: Vec<i32> = unsafe {vec![1,2,3]};
//!
//! //Initialized before V1
//! //then destroyed after V1
//! #[dynamic(init=142,drop=142)]
//! static mut INIT_AND_DROP: Vec<i32> = unsafe {vec![1,2,3]};
//!
//! fn main(){
//!     assert_eq!(V1[0],1);
//!     unsafe{
//!     assert_eq!(V2[2],3);
//!     V2[2] = 42;
//!     assert_eq!(V2[2], 42);
//!
//!     assert_eq!(L1[0],1);
//!     unsafe{
//!     assert_eq!(L2[2],3);
//!     L2[2] = 42;
//!     assert_eq!(L2[2], 42);
//!     }
//!     }
//! }
//! ```
//!
//! # Attributes
//!
//! Static variables marked with the [dynamic(lazy)] are initialized
//! on first use or just befor main start on unixes and windows. On
//! those plateforms access to those statics will be as fast as regular statics.
//! On other plateforms they fall back to equivalent of `std::lazy::SyncLazy`.
//!
//! Lazy statics requires std support and can be desabled by disabling "lazy" feature.
//! All other attributes does not requires std support but are only supported on unixes, mac and
//! windows.
//!
//! All functions marked with the [constructor] attribute are
//! run before `main` is started.
//!
//! All function marked with the [destructor] attribute are
//! run after `main` has returned.
//!
//! Static variables marked with the [dynamic] attribute can
//! be initialized before main start and optionaly droped
//! after main returns.
//!
//! The attributes [constructor] and [destructor] works by placing the marked function pointer in
//! dedicated object file sections.
//!
//! Priority ranges from 0 to 2<sup>16</sup>-1. The absence of priority is equivalent to
//! a hypothetical priority number of -1.
//!
//! During program initialization:
//!
//! - constructors with priority 65535 are the first called;
//! - constructors without priority are called last.
//!
//! During program termination, the order is reversed:
//!
//! - destructors without priority are the first called;
//! - destructors with priority 65535 are the last called.
//!
//! # Safety
//!
//!  Any access to lazy statics are safe. 
//!
//!  If `debug-assertions` is enabled or feature `debug_order` is passed accesses to
//!  statics not yet initialized will cause a panic.
//!
//!  If neither `debug-assertions` nor feature `debug_order` are enabled accesses to
//!  statics that are not initialized will cause undefined behavior (Unless this access happen
//!  during initialization phase a zero initialized memory is a valid memory
//!  representation for the type of the static).
//!
//!  Accesses to uninitialized dynamic may happen when a constructor access a dynamic static
//!  that as a lower or equal initialization priority or when a destructor access a dynamic static dropped
//!  with a lower or equal drop priority
//!
//! ```no_run
//! use static_init::dynamic;
//!
//! #[dynamic]
//! static V1: Vec<i32> = unsafe {vec![1,2,3]};
//!
//! //potential undefined behavior: V1 may not have been initialized yet
//! #[dynamic]
//! static V2: i32 = unsafe {V1[0]};
//!
//! //undefined behavior, V3 is unconditionnaly initialized before V1
//! #[dynamic(1000)]
//! static V3: i32 = unsafe {V1[0]};
//!
//! #[dynamic(1000)]
//! static V4: Vec<i32> = unsafe {vec![1,2,3]};
//!
//! //Good, V5 initialized after V4
//! #[dynamic(500)]
//! static V5: i32 = unsafe {V4[0]};
//!
//! //Good, V6 initialized after V5 and v4
//! #[dynamic]
//! static V6: i32 = unsafe {*V5+V4[1]};
//!
//!
//! # fn main(){}
//! ```
//!
//! # Comparisons against other crates
//!
//! ## [lazy_static][1]
//!  - lazy_static only provides const statics.
//!  - Each access to lazy_static statics costs 2ns on a x86.
//!  - lazy_static does not provide priorities.
//!  - lazy_static statics initialization is *safe*.
//!
//! ## [ctor][2]
//!  - ctor only provides const statics.
//!  - ctor does not provide priorities.
//!
//! # Documentation and details
//!
//! ## Mac
//!   - [MACH_O specification](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
//!   - GCC source code gcc/config/darwin.c indicates that priorities are not supported.
//!
//!   Initialization functions pointers are placed in section "__DATA,__mod_init_func" and
//!   "__DATA,__mod_term_func"
//!
//!   std::env is not initialized in any constructor.
//!
//! ## ELF plateforms:
//!  - `info ld`
//!  - linker script: `ld --verbose`
//!  - [ELF specification](https://docs.oracle.com/cd/E23824_01/html/819-0690/chapter7-1.html#scrolltoc)
//!
//!  The runtime will run fonctions pointers of section ".init_array" at startup and function
//!  pointers in ".fini_array" at program exit. The linker place in the target object file
//!  sectio .init_array all sections from the source objects whose name is of the form
//!  .init_array.NNNNN in lexicographical order then the .init_array sections of those same source
//!  objects. It does equivalently with .fini_array and .fini_array.NNNN sections.
//!
//!  Usage can be seen in gcc source gcc/config/pru.c
//!
//!  Resources of libstdc++ are initialized with priority 65535-100 (see gcc source libstdc++-v3/c++17/default_resource.h)
//!  The rust standard library function that capture the environment and executable arguments is
//!  executed at priority 65535-99 on gnu platform variants. On other elf plateform they are not accessbile in any constructors. Nevertheless
//!  one can read into /proc/self directory to retrieve the command line.
//!  Some callbacks constructors and destructors with priority 65535 are
//!  registered by rust/rtlibrary.
//!  Static C++ objects are usually initialized with no priority (TBC). lib-c resources are
//!  initialized by the C-runtime before any function in the init_array (whatever the priority) are executed.
//!
//! ## Windows
//!
//!   std::env is initialized before any constructors.
//!
//!  - [this blog post](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
//!
//!  At start up, any functions pointer between sections ".CRT$XIA" and ".CRT$XIZ"
//!  and then any functions between ".CRT$XCA" and ".CRT$XCZ". It happens that the C library
//!  initialization functions pointer are placed in ".CRT$XIU" and C++ statics functions initialization
//!  pointers are placed in ".CRT$XCU". At program finish the pointers between sections
//!  ".CRT$XPA" and ".CRT$XPZ" are run first then those between ".CRT$XTA" and ".CRT$XTZ".
//!
//!  Some reverse engineering was necessary to find out a way to implement
//!  constructor/destructor priority.
//!
//!  Contrarily to what is reported in this blog post, msvc linker
//!  only performs a lexicographicall ordering of section whose name
//!  is of the form "\<prefix\>$\<suffix\>" and have the same \<prefix\>.
//!  For example "RUST$01" and "RUST$02" will be ordered but those two
//!  sections will not be ordered with "RHUM" section.
//!
//!  Moreover, it seems that section name of the form \<prefix\>$\<suffix\> are
//!  not limited to 8 characters.
//!
//!  So static initialization function pointers are placed in section ".CRT$XCU" and
//!  those with a priority `p` in `format!(".CRT$XCTZ{:05}",65535-p)`. Destructors without priority
//!  are placed in ".CRT$XPU" and those with a priority in `format!(".CRT$XPTZ{:05}",65535-p)`.
//!
//!
//! [1]: https://crates.io/crates/lazy_static
//! [2]: https://crates.io/crates/ctor
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
pub struct StaticInfo {
    pub variable_name: &'static str,
    pub file_name:     &'static str,
    pub line:          u32,
    pub column:        u32,
    pub init_priority: i32,
    pub drop_priority: i32,
}

#[cfg(any(feature = "debug_order", debug_assertions))]
use core::sync::atomic::{AtomicI32, Ordering};

#[cfg(any(feature = "debug_order", debug_assertions))]
static CUR_INIT_PRIO: AtomicI32 = AtomicI32::new(65537);

#[cfg(any(feature = "debug_order", debug_assertions))]
static CUR_DROP_PRIO: AtomicI32 = AtomicI32::new(65537);

/// The actual type of "dynamic" mutable statics.
///
/// It implements `Deref<Target=T>` and `DerefMut`.
///
/// All associated functions are only usefull for the implementation of
/// the [dynamic] proc macro attribute
pub struct Static<T>(
    StaticBase<T>,
    #[cfg(any(feature = "debug_order", debug_assertions))] StaticInfo,
    #[cfg(any(feature = "debug_order", debug_assertions))] AtomicI32,
);

#[cfg(any(feature = "debug_order", debug_assertions))]
#[inline]
pub fn __set_init_prio(v: i32) {
    CUR_INIT_PRIO.store(v, Ordering::Relaxed);
}
#[cfg(not(any(feature = "debug_order", debug_assertions)))]
#[inline(always)]
pub fn __set_init_prio(_: i32) {}

//As a trait in order to avoid noise;
impl<T> Static<T> {
    #[inline]
    pub const fn uninit(_info: StaticInfo) -> Self {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            Self(StaticBase { k: () }, _info, AtomicI32::new(0))
        }
        #[cfg(not(any(feature = "debug_order", debug_assertions)))]
        {
            Self(StaticBase { k: () })
        }
    }
    #[inline]
    pub const fn from(v: T, _info: StaticInfo) -> Self {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            Static(
                StaticBase {
                    v: ManuallyDrop::new(v),
                },
                _info,
                AtomicI32::new(1),
            )
        }
        #[cfg(not(any(feature = "debug_order", debug_assertions)))]
        {
            Static(StaticBase {
                v: ManuallyDrop::new(v),
            })
        }
    }

    #[inline]
    pub unsafe fn set_to(this: &mut Self, v: T) {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            this.0.v = ManuallyDrop::new(v);
            this.2.store(1, Ordering::Relaxed);
        }
        #[cfg(not(any(feature = "debug_order", debug_assertions)))]
        {
            this.0.v = ManuallyDrop::new(v);
        }
    }

    #[inline]
    pub unsafe fn drop(this: &mut Self) {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            CUR_DROP_PRIO.store(this.1.drop_priority, Ordering::Relaxed);
            ManuallyDrop::drop(&mut this.0.v);
            this.2.store(2, Ordering::Relaxed);
        }
        #[cfg(not(any(feature = "debug_order", debug_assertions)))]
        {
            ManuallyDrop::drop(&mut this.0.v);
        }
    }
}
impl<T> Deref for Static<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            let status = self.2.load(Ordering::Relaxed);
            if status == 0 {
                core::panic!(
                    "Attempt to access variable {:#?} before it is initialized during \
                     initialization priority {}",
                    self.1,
                    CUR_INIT_PRIO.load(Ordering::Relaxed)
                )
            }
            if status == 2 {
                core::panic!(
                    "Attempt to access variable {:#?} after it was destroyed during destruction \
                     priority {}",
                    self.1,
                    CUR_DROP_PRIO.load(Ordering::Relaxed)
                )
            }
        }
        unsafe { &*self.0.v }
    }
}
impl<T> DerefMut for Static<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        #[cfg(any(feature = "debug_order", debug_assertions))]
        {
            let status = self.2.load(Ordering::Relaxed);
            if status == 0 {
                core::panic!(
                    "Attempt to access variable {:#?} before it is initialized during \
                     initialization
                priority {}",
                    self.1,
                    CUR_INIT_PRIO.load(Ordering::Relaxed)
                )
            }
            if status == 2 {
                core::panic!(
                    "Attempt to access variable {:#?} after it was destroyed during destruction
                priority {}",
                    self.1,
                    CUR_DROP_PRIO.load(Ordering::Relaxed)
                )
            }
        }
        unsafe { &mut *self.0.v }
    }
}

/// The actual type of "dynamic" non mutable statics.
///
/// It implements `Deref<Target=T>`.
///
/// All associated functions are only usefull for the implementation of
/// the [dynamic] proc macro attribute
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
    #[inline]
    fn deref(&self) -> &T {
        unsafe { &**self.0.get() }
    }
}

#[cfg(feature = "lazy")]
mod global_lazy {
    use core::cell::Cell;
    use core::cell::UnsafeCell;
    use core::fmt;
    use core::hint::unreachable_unchecked;
    use core::mem::MaybeUninit;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::Ordering;
    use std::sync::Once;

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

    /// A lazy static that is ensured to be initialized after program startup
    /// initialization phase.
    pub struct Lazy<T, F = fn() -> T> {
        value:    UnsafeCell<MaybeUninit<T>>,
        initer:   Once,
        init_exp: Cell<Option<F>>,
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
        /// #Safety
        /// The static shall be initialized only when used
        /// in conjunction with the dynamic(lazy) attribute
        pub const fn new(f: F) -> Self {
            Self {
                value:    UnsafeCell::new(MaybeUninit::uninit()),
                initer:   Once::new(),
                init_exp: Cell::new(Some(f)),
            }
        }
        #[inline(always)]
        pub fn as_mut_ptr(this: &Self) -> *mut T {
            this.value.get() as *mut T
        }
        #[inline(always)]
        pub fn do_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            this.initer.call_once(|| unsafe {
                (&mut *this.value.get()).as_mut_ptr().write(this
                    .init_exp
                    .take()
                    .unwrap_or_else(|| unreachable_unchecked())(
                ))
            });
        }
        #[inline(always)]
        fn ensure_init(this: &Self)
        where
            F: FnOnce() -> T,
        {
            if !LAZY_INIT_ENSURED.load(Ordering::Acquire) {
                Self::do_init(this);
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
                &*(*self.value.get()).as_ptr()
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
                &mut *(*self.value.get()).as_mut_ptr()
            }
        }
    }
}

pub use global_lazy::Lazy;
