extern crate static_init;
use static_init::{constructor, destructor, dynamic};

static mut DEST: i32 = 0;

#[destructor]
unsafe extern "C" fn dest_0() {
    assert_eq!(DEST, 0);
    DEST += 1;
}

#[destructor(0)]
unsafe extern "C" fn dest_1() {
    assert_eq!(DEST, 1);
    DEST += 1;
}
#[destructor(100)]
unsafe extern "C" fn dest_2() {
    assert_eq!(DEST, 2);
    DEST += 1;
}

static mut INI: i32 = 0;

#[constructor(200)]
unsafe extern "C" fn init_2() {
    assert_eq!(INI, 0);
    INI += 1;
}
#[constructor(0)]
unsafe extern "C" fn init_1() {
    assert_eq!(INI, 1);
    INI += 1;
}
#[constructor]
unsafe extern "C" fn init_0() {
    assert_eq!(INI, 2);
    INI += 1;
}

#[cfg(all(unix, target_env = "gnu"))]
mod gnu {
    use super::constructor;
    use std::env::args_os;
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;

    #[constructor]
    unsafe extern "C" fn get_args_env(
        argc: i32,
        mut argv: *const *const u8,
        _env: *const *const u8,
    ) {
        let mut argc_counted = 0;

        while !(*argv).is_null() {
            assert!(args_os()
                .any(|x| x == OsStr::from_bytes(CStr::from_ptr(*argv as *const i8).to_bytes())));
            argv = argv.add(1);
            argc_counted += 1
        }
        assert_eq!(argc_counted, argc);
    }
}

#[derive(Debug, Eq, PartialEq)]
struct A(i32);

impl A {
    fn new(v: i32) -> A {
        A(v)
    }
}
impl Drop for A {
    fn drop(&mut self) {
        assert_eq!(self.0, 33)
    }
}

#[test]
fn inner_static() {
    #[dynamic]
    static IX: usize = unsafe { &IX as *const _ as usize };
    #[dynamic]
    static IX2: usize = unsafe { &IX2 as *const _ as usize };

    static mut I: i32 = 0;

    #[constructor]
    unsafe extern "C" fn f() {
        I = 3
    }

    assert_eq!(*IX, &IX as *const _ as usize);
    assert_eq!(*IX2, &IX2 as *const _ as usize);
    unsafe { assert_eq!(I, 3) };
}

#[dynamic]
static mut V0: A = unsafe { A::new(V1.0 - 5) };

#[dynamic(20)]
static mut V2: A = unsafe { A::new(12) };

#[dynamic(10)]
static V1: A = unsafe { A::new(V2.0 - 2) };

#[dynamic(init = 20)]
static mut V3: A = unsafe { A::new(12) };

#[dynamic(init = 10)]
static V4: A = unsafe { A::new(V2.0 - 2) };

#[dynamic(init = 5, drop)]
static V5: A = unsafe { A::new(V4.0 + 23) };

#[dynamic(drop)]
static V6: A = unsafe { A(33) };

#[test]
fn dynamic_init() {
    unsafe { assert_eq!(V0.0, 5) };
    assert_eq!(V1.0, 10);
    unsafe { assert_eq!(V2.0, 12) };
    unsafe { V2.0 = 8 };
    unsafe { assert_eq!(V2.0, 8) };
    assert_eq!(V4.0, 10);
    unsafe { assert_eq!(V3.0, 12) };
    assert_eq!(V5.0, 33);
    assert_eq!(V6.0, 33);
}

#[cfg(feature = "lazy")]
mod lazy {
    use super::A;
    use static_init::dynamic;
    #[dynamic(lazy)]
    static L1: A = A::new(unsafe { L0.0 + 1 });

    #[dynamic(lazy)]
    static mut L0: A = A::new(10);

    #[cfg(feature = "lazy_drop")]
    #[dynamic(lazy, drop)]
    static mut L2: A = A::new(33);

    #[test]
    fn lazy_init() {
        unsafe { assert_eq!(L0.0, 10) };
        assert_eq!(L1.0, 11);
    }
}
