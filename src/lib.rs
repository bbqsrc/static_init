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
//! ## ELF plateform:
//!   - `info ld`
//!   - linker script: `ld --verbose`
//!   - [ELF specification](https://docs.oracle.com/cd/E23824_01/html/819-0690/chapter7-1.html#scrolltoc)
//! ## Windows
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

#[cfg(target_os = "windows")]
mod windows {
    #[link_section = ".RIN0"]
    #[used]
    static INIT_START : Option<fn()->()> = None;
    #[link_section = ".RIN8"]
    #[used]
    static INIT_END : Option<fn()->()> = None;
    #[link_section = ".RFI0"]
    #[used]
    static FINI_START : Option<fn()->()> = None;
    #[link_section = ".RFI8"]
    #[used]
    static FINI_END : Option<fn()->()> = None;

    fn initialize() {
        let mut start: *const Option<fn()->()> = &INIT_START;
        let end: *const Option<fn()->()> = &INIT_END;
        while start != end {
            let f = unsafe{*start};
            if let Some(f) = f {
                f()
            }
            start = unsafe{start.add(1)};
        }
    }

    #[used]
    #[link_section = ".CRT$XCU"]
    static INITIALIZER: fn()->() = initialize;

    fn finalize() {
        let mut start: *const Option<fn()->()> = &FINI_START;
        let end: *const Option<fn()->()> = &FINI_END;
        while start != end {
            let f = unsafe{*start};
            if let Some(f) = f {
                f()
            }
            start = unsafe{start.add(1)};
        }
    }

    #[used]
    #[link_section = ".CRT$XPU"]
    static FINALIZER: fn()->() = finalize;

}
