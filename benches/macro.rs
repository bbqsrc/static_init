#![feature(test)]
extern crate static_init;
use ctor::ctor;
use static_init::dynamic;

extern crate test;
use std::sync::atomic::{AtomicI32, Ordering};
use test::Bencher;

extern crate lazy_static;
use lazy_static::lazy_static;

static O: AtomicI32 = AtomicI32::new(0);

#[dynamic(10)]
static W: AtomicI32 = unsafe { AtomicI32::new(0) };

#[dynamic(10)]
static mut WM: AtomicI32 = unsafe { AtomicI32::new(0) };

lazy_static! {
    static ref WL: AtomicI32 = AtomicI32::new(0);
    static ref WL1: AtomicI32 = AtomicI32::new(WL2.load(Ordering::Relaxed));
    static ref WL2: AtomicI32 = AtomicI32::new(WL1.load(Ordering::Relaxed));
}

#[ctor]
static WCT: AtomicI32 = AtomicI32::new(0);

#[dynamic(lazy)]
static L: AtomicI32 = AtomicI32::new(0);

#[bench]
fn access_regular(bench: &mut Bencher) {
    bench.iter(|| O.fetch_add(1, Ordering::Relaxed));
}

#[bench]
fn access(bench: &mut Bencher) {
    bench.iter(|| W.fetch_add(1, Ordering::Relaxed));
}
#[bench]
fn access_m(bench: &mut Bencher) {
    bench.iter(|| unsafe { WM.fetch_add(1, Ordering::Relaxed) });
}
#[bench]
fn access_l(bench: &mut Bencher) {
    bench.iter(|| L.fetch_add(1, Ordering::Relaxed));
}
//access to lazy static cost 2ns
#[bench]
fn lazy_static(bench: &mut Bencher) {
    bench.iter(|| WL.fetch_add(1, Ordering::Relaxed));
}

#[bench]
fn ctor(bench: &mut Bencher) {
    bench.iter(|| WCT.fetch_add(1, Ordering::Relaxed));
}
