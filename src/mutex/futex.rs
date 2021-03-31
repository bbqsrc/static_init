#[cfg(all(
    not(feature = "parking_lot_core"),
    any(target_os = "linux", target_os = "android")
))]
mod linux {
    use core::ptr;
    use core::ops::Deref;
    use core::sync::atomic::AtomicU32;
    use libc::{
        syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT_BITSET, FUTEX_WAKE_BITSET,
    };

    pub(crate) struct Futex {
        futex: AtomicU32,
    }

    impl Futex {
        pub(crate) const fn new(value: u32) -> Self {
            Self {
                futex: AtomicU32::new(value),
            }
        }

        pub(crate) fn compare_and_wait_as_reader(&self, value: u32) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    1,
                ) == 0
            }
        }
        pub(crate) fn compare_and_wait_as_writer(&self, value: u32) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    2,
                ) == 0
            }
        }
        pub(crate) fn wake_readers(&self) -> usize {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    i32::MAX,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    1,
                ) as usize
            }
        }
        pub(crate) fn wake_one_writer(&self) -> bool {
            unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    1,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    2,
                ) == 1
            }
        }

    }
    impl Deref for Futex {
        type Target = AtomicU32;
        fn deref(&self) -> &Self::Target {
            &self.futex
        }
    }
}
#[cfg(all(
    not(feature = "parking_lot_core"),
    any(target_os = "linux", target_os = "android")
))]
pub(crate) use linux::Futex;

#[cfg(feature = "parking_lot_core")]
mod other {
    use core::ops::Deref;
    use core::sync::atomic::{AtomicU32, Ordering};
    use parking_lot_core::{
        park, unpark_all, unpark_one, ParkResult, DEFAULT_PARK_TOKEN, DEFAULT_UNPARK_TOKEN,
    };

    pub(crate) struct Futex(AtomicU32);

    impl Futex {
        pub(crate) const fn new(value: u32) -> Self {
            Self(AtomicU32::new(value))
        }

        pub(crate) fn compare_and_wait_as_reader(&self, value: u32) -> bool {
            unsafe {
                matches!(
                    park(
                        &self.0 as *const _ as usize,
                        || self.0.load(Ordering::Relaxed) == value,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    ),
                    ParkResult::Unparked(_)
                )
            }
        }
        pub(crate) fn compare_and_wait_as_writer(&self, value: u32) -> bool {
            unsafe {
                matches!(
                    park(
                        (&self.0 as *const _ as usize) + 1,
                        || self.0.load(Ordering::Relaxed) == value,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    ),
                    ParkResult::Unparked(_)
                )
            }
        }
        pub(crate) fn wake_readers(&self) -> usize {
            unsafe { unpark_all(&self.0 as *const _ as usize, DEFAULT_UNPARK_TOKEN) }
        }
        pub(crate) fn wake_one_writer(&self) -> bool {
            unsafe {
                let r = unpark_one((&self.0 as *const _ as usize) + 1, |_| DEFAULT_UNPARK_TOKEN);
                r.unparked_threads == 1
            }
        }
    }

    impl Deref for Futex {
        type Target = AtomicU32;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
}
#[cfg(feature = "parking_lot_core")]
pub(crate) use other::Futex;
