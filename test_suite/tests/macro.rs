#![feature(test)]
extern crate static_init;
use static_init::{constructor, destructor, dynamic};
extern crate test;
use std::sync::atomic::{AtomicI32, Ordering};
use test::Bencher;

extern crate lazy_static;
use lazy_static::lazy_static;

static mut DEST: i32 = 0;

#[destructor]
fn dest_0() {
    unsafe {
        assert_eq!(DEST, 0);
        DEST += 1;
    }
}

#[destructor(100)]
fn dest_1() {
    unsafe {
        assert_eq!(DEST, 1);
        DEST += 1;
    }
}
#[destructor(0)]
fn dest_2() {
    unsafe {
        assert_eq!(DEST, 2);
        DEST += 1;
    }
}
static mut INI: i32 = 0;

#[constructor(0)]
fn init_2() {
    unsafe {
        assert_eq!(INI, 0);
        INI += 1;
    }
}
#[constructor(200)]
fn init_1() {
    unsafe {
        assert_eq!(INI, 1);
        INI += 1;
    }
}
#[constructor]
fn init_0() {
    unsafe {
        assert_eq!(INI, 2);
        INI += 1;
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
        //assert_eq!(self.0, 33)
    }
}

#[dynamic]
static mut V0: A = A::new((*V1).0 - 1);

#[dynamic(10)]
static mut V2: A = A::new(12);

#[dynamic(20)]
static V1: A = A::new(unsafe { (*V2).0 } - 2);

#[dynamic(init = 10)]
static mut V3: A = A::new(12);

#[dynamic(init = 20)]
static V4: A = A::new(unsafe { (*V2).0 } - 2);

#[dynamic(init = 30, drop)]
static V5: A = A::new((*V4).0 + 23);


#[test]
fn dynamic_init() {
    unsafe { assert_eq!((*V0).0, 9) };
    assert_eq!((*V1).0, 10);
    unsafe { assert_eq!((*V2).0, 12) };
    unsafe { (*V2).0 = 8 };
    unsafe { assert_eq!((*V2).0, 8) };
    assert_eq!((*V4).0, 10);
    unsafe { assert_eq!((*V3).0, 12) };
}

#[dynamic(10)]
static W: AtomicI32 = AtomicI32::new(0);

#[dynamic(10)]
static mut WM: AtomicI32 = AtomicI32::new(0);

lazy_static! {
    static ref WL: AtomicI32 = AtomicI32::new(0);
}

#[bench]
fn access(bench: &mut Bencher) {
    bench.iter(|| W.fetch_add(1, Ordering::Relaxed));
}
#[bench]
fn access_m(bench: &mut Bencher) {
    bench.iter(|| unsafe { WM.fetch_add(1, Ordering::Relaxed) });
}
//access to lazy static cost 2ns
#[bench]
fn lazy_static(bench: &mut Bencher) {
    bench.iter(|| WL.fetch_add(1, Ordering::Relaxed));
}
