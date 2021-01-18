#![no_std]
//! This crates provides macros to run code or initialize/drop statics at program startup and exit.
//!
//! All functions marked with the [constructor] attribute are 
//! run before `main` is started.
//!
//! All function marked with the [destructor] attribute are 
//! run after `main` has returned.
//!
//! Static variables marked with the [dynamic] attribute can
//! be initialized before main start and optionaly droped
//! after main returns. Contrarily to lazy_static crate, access
//! to those static variable does not incur an extra pointer check then 
//! dereference cost.
//!
//! The attributes [constructor] and [destructor] works by placing the marked function pointer in
//! dedicated object file sections. On elf plateform, a priority can be specified. 
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
