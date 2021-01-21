#![feature(test)]
extern crate static_init;
use static_init::dynamic;

extern crate test;
use std::sync::atomic::{AtomicI32, Ordering};
use test::Bencher;

extern crate lazy_static;
use lazy_static::lazy_static;

#[dynamic(10)]
static W: AtomicI32 = unsafe{AtomicI32::new(0)};

#[dynamic(10)]
static mut WM: AtomicI32 = unsafe{AtomicI32::new(0)};

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
