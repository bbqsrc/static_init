#[cfg(all(
    not(feature = "parking_lot_core"),
    any(target_os = "linux", target_os = "android")
))]
mod linux {
    use crate::phase::*;
    use core::ops::Deref;
    use core::ptr;
    use core::sync::atomic::{compiler_fence, AtomicU32, Ordering};
    use libc::{syscall, SYS_futex, FUTEX_PRIVATE_FLAG, FUTEX_WAIT_BITSET, FUTEX_WAKE_BITSET};

    pub(crate) struct Futex {
        futex:        AtomicU32,
        writer_count: AtomicU32,
    }

    const READER_BIT: u32 = 0b01;
    const WRITER_BIT: u32 = 0b10;

    impl Futex {
        pub(crate) const fn new(value: u32) -> Self {
            Self {
                futex:        AtomicU32::new(value),
                writer_count: AtomicU32::new(0),
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
                    READER_BIT,
                ) == 0
            }
        }
        pub(crate) fn compare_and_wait_as_writer(&self, value: u32) -> bool {
            assert_ne!(self.writer_count.fetch_add(1, Ordering::Relaxed), u32::MAX);
            compiler_fence(Ordering::AcqRel);
            let res = unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAIT_BITSET | FUTEX_PRIVATE_FLAG,
                    value,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    WRITER_BIT,
                ) == 0
            };
            compiler_fence(Ordering::AcqRel);
            let prev_count = self.writer_count.fetch_sub(1, Ordering::Relaxed);
            assert_ne!(prev_count, 0);
            //// count = number of threads waiting at the time of wake + those
            ////         for which the futex syscall as been interrupted but count not
            ////         yet substracted + those that are in the process of waiting
            //// so here count is larger than the number of waiting threads
            if res && prev_count > 1 {
                self.futex.fetch_or(WRITE_WAITER_BIT, Ordering::Relaxed);
            }
            res
        }
        pub(crate) fn wake_readers(&self) -> usize {
            let count = unsafe {
                syscall(
                    SYS_futex,
                    &self.futex as *const _ as *const _,
                    FUTEX_WAKE_BITSET | FUTEX_PRIVATE_FLAG,
                    MAX_WAKED_READERS as i32,
                    ptr::null::<u32>(),
                    ptr::null::<u32>(),
                    READER_BIT,
                ) as usize
            };
            if count == MAX_WAKED_READERS {
                self.futex.fetch_or(READ_WAITER_BIT, Ordering::Relaxed);
            }
            count
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
                    WRITER_BIT,
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
    use crate::phase::*;
    use core::ops::Deref;
    use core::sync::atomic::{compiler_fence, AtomicU32, Ordering};
    use parking_lot_core::{
        park, unpark_filter, unpark_one, FilterOp, ParkResult, DEFAULT_PARK_TOKEN,
        DEFAULT_UNPARK_TOKEN,
    };

    pub(crate) struct Futex {
        futex:        AtomicU32,
        writer_count: AtomicU32,
    }

    impl Futex {
        pub(crate) const fn new(value: u32) -> Self {
            Self {
                futex:        AtomicU32::new(value),
                writer_count: AtomicU32::new(0),
            }
        }

        pub(crate) fn compare_and_wait_as_reader(&self, value: u32) -> bool {
            unsafe {
                matches!(
                    park(
                        self.reader_key(),
                        || self.futex.load(Ordering::Relaxed) == value,
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
            assert_ne!(self.writer_count.fetch_add(1, Ordering::Relaxed), u32::MAX);
            compiler_fence(Ordering::AcqRel);
            let res = unsafe {
                matches!(
                    park(
                        self.writer_key(),
                        || self.futex.load(Ordering::Relaxed) == value,
                        || {},
                        |_, _| {},
                        DEFAULT_PARK_TOKEN,
                        None,
                    ),
                    ParkResult::Unparked(_)
                )
            };
            compiler_fence(Ordering::AcqRel);
            let prev_count = self.writer_count.fetch_sub(1, Ordering::Relaxed);
            assert_ne!(prev_count, 0);
            //// count = number of threads waiting at the time of unpark + those
            ////         for which the futex syscall as been interrupted but count not
            ////         yet substracted + those that are in the process of waiting
            //// so here count is larger than the number of waiting threads
            if res && prev_count > 1 {
                self.futex.fetch_or(WRITE_WAITER_BIT, Ordering::Relaxed);
            }
            res
        }
        pub(crate) fn wake_readers(&self) -> usize {
            let mut c = 0;
            let r = unsafe {
                unpark_filter(
                    self.reader_key(),
                    |_| {
                        if c < MAX_WAKED_READERS {
                            c += 1;
                            FilterOp::Unpark
                        } else {
                            FilterOp::Stop
                        }
                    },
                    |_| DEFAULT_UNPARK_TOKEN,
                )
            };

            if c == MAX_WAKED_READERS {
                self.futex.fetch_or(READ_WAITER_BIT, Ordering::Relaxed);
            }

            r.unparked_threads
        }
        pub(crate) fn wake_one_writer(&self) -> bool {
            let r = unsafe { unpark_one(self.writer_key(), |_| DEFAULT_UNPARK_TOKEN) };
            r.unparked_threads == 1
        }

        fn reader_key(&self) -> usize {
            &self.futex as *const _ as usize
        }
        fn writer_key(&self) -> usize {
            (&self.futex as *const _ as usize) + 1
        }
    }

    impl Deref for Futex {
        type Target = AtomicU32;
        fn deref(&self) -> &Self::Target {
            &self.futex
        }
    }
}
#[cfg(feature = "parking_lot_core")]
pub(crate) use other::Futex;
