use core::cell::UnsafeCell;
use core::mem::ManuallyDrop;
use core::ops::{Deref, DerefMut};

pub use static_init_macro::{constructor, destructor, dynamic};

pub union Static<T> {
    #[used]
    k: (),
    v: ManuallyDrop<T>,
}

//As a trait in order to avoid noise;
impl<T> Static<T> {
    pub const fn uninit() -> Self {
        Self { k: () }
    }
    pub fn init_with(v: T) -> Self {
        Static {
            v: ManuallyDrop::new(v),
        }
    }
    pub unsafe fn set_to(this: &mut Self, v: T) {
        *this = Self::init_with(v);
    }
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

pub struct ConstStatic<T>(UnsafeCell<Static<T>>);

impl<T> ConstStatic<T> {
    pub const fn uninit() -> Self {
        Self(UnsafeCell::new(Static::uninit()))
    }
    pub unsafe fn set_to(this: &Self, v: T) {
        *this.0.get() = Static::init_with(v)
    }
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

//static ANTECEDANTS: Dumb<Mutex<Ant>> = Dumb(UnsafeCell::new(DumbInner{k:()}));
//unsafe fn init() {
//     *ANTECEDANTS.0.get() = DumbInner{v:ManuallyDrop::new(Mutex::new(HashMap::new()))};
//}
//
//#[link_section = ".init_array"]
//#[used]
//static DO_INIT: unsafe fn()->() = init;
//#[cfg(test)]
//mod tests {
//    #[test]
//    fn it_works() {
//        assert_eq!(2 + 2, 4);
//    }
//}
