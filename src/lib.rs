#![no_std]
//! This crates provides macros to run code or initialize/drop statics and mutable statics at program startup and exit.
//!
//! # Functionalities
//! * Code execution before or after `main`.
//! * Mutable and const statics with non const initialization.
//! * Statics droppable after `main` exits.
//! * Zero cost access to statics.
//! * On elf plateforms, priorities can be specified.
//!
//! # Attributes
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
//! dedicated object file sections. On elf plateforms, a priority can also be specified. 
//!
//! # Comparisons with other crates
//!
//! ## [lazy_static][1]
//!  - lazy_static only provides const statics.
//!  - Each access to lazy_static statics cost 2ns on a x86.
//!  - lazy_static does not provide priorities on elf plateforms (unixes, linux, bsd, etc..).
//!
//! ## [ctor][2]
//!  - ctor only provides const statics.
//!  - ctor does not provide priority on elf plateforms (unixes, linux, bsd, etc..)
//!
//! # Documentation
//! ## Mac
//!   - [MACH_O specification](https://www.cnblogs.com/sunkang/archive/2011/05/24/2055635.html)
//!   - GCC source code gcc/config/darwin.c indicates that priorities are not supported. 
//!
//!   Initialization functions pointers are placed in section "__DATA,__mod_init_func" and
//!   "__DATA,__mod_term_func"
//!
//! ## ELF plateform:
//!   - `info ld`
//!   - linker script: `ld --verbose`
//!   - [ELF specification](https://docs.oracle.com/cd/E23824_01/html/819-0690/chapter7-1.html#scrolltoc)
//!
//!   The runtime will run fonctions pointers of section ".init_array" at startup and function
//!   pointers in ".fini_array" at program exit. The linker place in the target object file
//!   sectio .init_array all sections from the source objects whose name is of the form
//!   .init_array.NNNNN in lexicographical order then the .init_array sections of those same source
//!   objects. It does equivalently with .fini_array and .fini_array.NNNN sections.
//!
//!   Usage can be seen in gcc source gcc/config/pru.c
//!
//!   Resources of libstdc++ are initialized with priority 100 (see gcc source libstdc++-v3/c++17/default_resource.h)
//!   The rust standard library function that capture the environment and executable arguments is
//!   executed at priority 99. Some callbacks constructors and destructors with priority 0 are
//!   registered by rust/rtlibrary.
//!   Static C++ objects are usually initialized with no priority (TBC). lib-c resources are
//!   initialized by the C-runtime before any function in the init_array (whatever the priority) are executed.
//!
//! ## Windows
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
//!  is of the form "<prefix>$<suffix>" and have the same <prefix>.
//!  For example "RUST$01" and "RUST$02" will be ordered but those two
//!  sections will not be ordered with "RHUM" section.
//!
//!  Moreover, it seems that section name of the form <prefix>$<suffix> are 
//!  not limited to 8 characters.
//!
//!  So static initialization function pointers will be placed in section ".CRT$XCU" and
//!  those with a priority `p` in `format!(".CRT$XCT{:05}",p)`. Destructors without priority
//!  are placed in ".CRT$XPU" and those with a priority in `format!(".CRT$XPT{:05}")`.
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

/// This is the actual type of a mutable static declared with the
/// 'dynamic' attribute. It implements `Deref<Target=T>` and `DerefMut`.
///
/// All associated functions are only usefull for the implementation of
/// the [dynamic] proc macro attribute
pub union Static<T> {
    #[used]
    k: (),
    v: ManuallyDrop<T>,
}


//As a trait in order to avoid noise;
impl<T> Static<T> {
    #[inline]
    pub const fn uninit() -> Self {
        Self { k: () }
    }
    #[inline]
    pub const fn from(v: T) -> Self {
        Static {
            v: ManuallyDrop::new(v),
        }
    }
    #[inline]
    pub unsafe fn set_to(this: &mut Self, v: T) {
        *this = Self::from(v);
    }
    #[inline]
    pub unsafe fn drop(this: &mut Self) {
        ManuallyDrop::drop(&mut this.v);
    }
}
impl<T> Deref for Static<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        unsafe { &*self.v }
    }
}
impl<T> DerefMut for Static<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.v }
    }
}

/// This is the actual type of a const static declared with the
/// 'dynamic' attribute. It implements `Deref<Target=T>`.
///
/// All associated functions are only usefull for the implementation of
/// the [dynamic] proc macro attribute
pub struct ConstStatic<T>(UnsafeCell<Static<T>>);

impl<T> ConstStatic<T> {
    #[inline]
    pub const fn uninit() -> Self {
        Self(UnsafeCell::new(Static::uninit()))
    }
    #[inline]
    pub const fn from(v: T) -> Self {
        Self(UnsafeCell::new(Static::from(v)))
    }
    #[inline]
    pub unsafe fn set_to(this: &Self, v: T) {
        *this.0.get() = Static::from(v)
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
