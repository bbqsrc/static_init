#[cfg(debug_mode)]
use super::{FinalyMode, InitMode, StaticInfo};

use core::mem::ManuallyDrop;

union StaticBase<T> {
    k: (),
    v: ManuallyDrop<T>,
}

pub use static_impl::{ConstStatic, Static, __set_init_prio};

#[cfg(debug_mode)]
mod static_impl {
    use super::{FinalyMode, InitMode, StaticBase, StaticInfo};
    use core::cell::UnsafeCell;
    use core::mem::ManuallyDrop;
    use core::ops::{Deref, DerefMut};
    use core::cmp::Ordering::*;
    /// The actual type of mutable *dynamic statics*.
    ///
    /// It implements `Deref<Target=T>` and `DerefMut`.
    ///
    /// All associated functions are only usefull for the implementation of
    /// the `dynamic` proc macro attribute
    pub struct Static<T>(StaticBase<T>, StaticInfo, AtomicI32);

    /// The actual type of non mutable *dynamic statics*.
    ///
    /// It implements `Deref<Target=T>`.
    ///
    /// All associated functions are only usefull for the implementation of
    /// the `dynamic` proc macro attribute
    pub struct ConstStatic<T>(UnsafeCell<Static<T>>);

    use core::sync::atomic::{AtomicI32, Ordering};

    static CUR_INIT_PRIO: AtomicI32 = AtomicI32::new(i32::MIN);

    static CUR_DROP_PRIO: AtomicI32 = AtomicI32::new(i32::MIN);

    #[doc(hidden)]
    #[inline]
    pub fn __set_init_prio(v: i32) {
        CUR_INIT_PRIO.store(v, Ordering::Relaxed);
    }

    impl<T> Static<T> {
        #[inline]
        /// Build an uninitialized Static
        ///
        /// # Safety
        ///
        /// The target object should be a mutable statics to ensure
        /// that all accesses to this object are unsafe.
        pub const unsafe fn uninit(info: StaticInfo) -> Self {
            Self(StaticBase { k: () }, info, AtomicI32::new(0))
        }
        #[inline]
        pub const fn from(v: T, info: StaticInfo) -> Self {
            Static(
                StaticBase {
                    v: ManuallyDrop::new(v),
                },
                info,
                AtomicI32::new(1),
            )
        }

        #[inline]
        pub fn set_to(this: &mut Self, v: T) {
            this.0.v = ManuallyDrop::new(v);
            this.2.store(1, Ordering::Relaxed);
        }

        #[inline]
        /// # Safety
        ///
        /// The objet should not be accessed after this call
        pub unsafe fn drop(this: &mut Self) {
            if let FinalyMode::ProgramDestructor(prio) = &this.1.drop_mode {
                CUR_DROP_PRIO.store(*prio as i32, Ordering::Relaxed);
                ManuallyDrop::drop(&mut this.0.v);
                CUR_DROP_PRIO.store(i32::MIN, Ordering::Relaxed);
            } else {
                ManuallyDrop::drop(&mut this.0.v);
            };
            this.2.store(2, Ordering::Relaxed);
        }
    }

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
                "Attempt to access variable {:#?} after it was destroyed during destruction \
                 priority {prio}. Tip increase drop priority of this static to a value larger \
                 than {prio} (attribute syntax: `#[dynamic(drop=<prio>)]`)",
                info,
                prio = CUR_DROP_PRIO.load(Ordering::Relaxed)
            )
        }
        let init_prio = CUR_INIT_PRIO.load(Ordering::Relaxed);
        let drop_prio = CUR_DROP_PRIO.load(Ordering::Relaxed);

        if let FinalyMode::ProgramDestructor(prio) = &info.drop_mode {
            match drop_prio.cmp(&(*prio as i32)) {
                Equal => core::panic!(
                    "This access to variable {:#?} is not sequenced before to its drop. Tip \
                     increase drop priority of this static to a value larger than {prio} \
                     (attribute syntax: `#[dynamic(drop=<prio>)]`)",
                    info,
                    prio = drop_prio
                ),
               Greater =>
                core::panic!(
                    "Unexpected initialization order while accessing {:#?} from drop priority {}. \
                     This is a bug of `static_init` library, please report \"
             the issue inside `static_init` repository.",
                    info,
                    drop_prio
                ),
               Less => (),
            }
        }

        if let InitMode::ProgramConstructor(prio) = &info.init_mode {
            match init_prio.cmp(&(*prio as i32)) {
              Equal => 
                core::panic!(
                    "This access to variable {:#?} is not sequenced after construction of this \
                     static. Tip increase init priority of this static to a value larger than \
                     {prio} (attribute syntax: `#[dynamic(init=<prio>)]`)",
                    info,
                    prio = init_prio
                ),
              Greater => 
                core::panic!(
                    "Unexpected initialization order while accessing {:#?} from init priority {}. \
                     This is a bug of `static_init` library, please report \"
             the issue inside `static_init` repository.",
                    info,
                    init_prio,
                ),
              Less => ()
            }
        }
    }

    impl<T> Deref for Static<T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            check_access(&self.1, self.2.load(Ordering::Relaxed));
            unsafe { &*self.0.v }
        }
    }
    impl<T> DerefMut for Static<T> {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            check_access(&self.1, self.2.load(Ordering::Relaxed));
            unsafe { &mut *self.0.v }
        }
    }

    impl<T> ConstStatic<T> {
        #[inline]
        /// Build an uninitialized ConstStatic
        ///
        /// # Safety 
        ///
        /// The target object should be a mutable static to 
        /// ensure that all accesses to the object are unsafe.
        pub const unsafe fn uninit(info: StaticInfo) -> Self {
            Self(UnsafeCell::new(Static::uninit(info)))
        }
        #[inline]
        pub const fn from(v: T, info: StaticInfo) -> Self {
            Self(UnsafeCell::new(Static::from(v, info)))
        }
        #[inline]
        /// # Safety
        ///
        /// The reference to self should be unique. 
        pub unsafe fn set_to(this: &Self, v: T) {
            Static::set_to(&mut (*this.0.get()), v)
        }
        #[inline]
        /// # Safety
        ///
        /// The objet should not be accessed after this call
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
}

#[cfg(not(debug_mode))]
mod static_impl {
    use super::StaticBase;
    use core::cell::UnsafeCell;
    use core::mem::ManuallyDrop;
    use core::ops::{Deref, DerefMut};
    /// The actual type of mutable *dynamic statics*.
    ///
    /// It implements `Deref<Target=T>` and `DerefMut`.
    ///
    /// All associated functions are only usefull for the implementation of
    /// the `dynamic` proc macro attribute
    pub struct Static<T>(StaticBase<T>);

    /// The actual type of non mutable *dynamic statics*.
    ///
    /// It implements `Deref<Target=T>`.
    ///
    /// All associated functions are only usefull for the implementation of
    /// the `dynamic` proc macro attribute
    pub struct ConstStatic<T>(UnsafeCell<Static<T>>);

    #[doc(hidden)]
    #[inline(always)]
    pub fn __set_init_prio(_: i32) {}

    //As a trait in order to avoid noise;
    impl<T> Static<T> {
        #[inline]
        /// Build a new static.
        ///
        /// # Safety
        ///
        /// It should always initialize a mutable static
        /// to ensure that any access to such object is unsafe
        pub const unsafe fn uninit() -> Self {
            Self(StaticBase { k: () })
        }
        #[inline]
        pub const fn from(v: T) -> Self {
            Static(StaticBase {
                v: ManuallyDrop::new(v),
            })
        }

        #[inline]
        pub fn set_to(this: &mut Self, v: T) {
            this.0.v = ManuallyDrop::new(v);
        }

        #[inline]
        /// Drop the inner object
        /// 
        /// # Safety
        ///
        /// The object should have been previously initialized
        pub unsafe fn drop(this: &mut Self) {
            ManuallyDrop::drop(&mut this.0.v);
        }
    }

    impl<T> Deref for Static<T> {
        type Target = T;
        #[inline(always)]
        fn deref(&self) -> &T {
            unsafe { &*self.0.v }
        }
    }
    impl<T> DerefMut for Static<T> {
        #[inline(always)]
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.0.v }
        }
    }

    impl<T> ConstStatic<T> {
        #[inline]
        /// Build a new ConstStatic
        ///
        /// # Safety
        ///
        /// The object should always be a mutable static as acces to
        /// it is always unsafe
        pub const unsafe fn uninit() -> Self {
            Self(UnsafeCell::new(Static::uninit()))
        }
        #[inline]
        pub const fn from(v: T) -> Self {
            Self(UnsafeCell::new(Static::from(v)))
        }
        #[inline]
        /// # Safety
        /// 
        /// The reference to Self should be unique
        pub unsafe fn set_to(this: &Self, v: T) {
            Static::set_to(&mut (*this.0.get()), v)
        }
        #[inline]
        /// # Safety
        ///
        /// The object should have been previously initialized
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
}
