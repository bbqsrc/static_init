// Copyright 2021 Olivier Kannengieser
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![feature(test)]
#![feature(thread_local)]
#![feature(asm)]
//TODO
#![allow(dead_code)]

extern crate static_init;
use static_init::{dynamic, Lazy};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
//use ctor::ctor;
//use static_init::dynamic;

use core::cell::UnsafeCell;
use crossbeam::thread;
use parking_lot::RwLock;
use static_init::Generator;
use static_init::MutLazy;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::time::Duration;

struct MutSynchronized<T>(UnsafeCell<T>);
unsafe impl<T> Sync for MutSynchronized<T> {}

struct XX;
impl Generator<i32> for XX {
    #[inline(always)]
    fn generate(&self) -> i32 {
        10
    }
}
fn bench_init_rwlock_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || RwLock::<Option<AtomicUsize>>::new(None),
        |v| {
            let mut l = v.write();
            match &*l {
                None => {
                    *l = Some(AtomicUsize::new(33));
                    l.as_ref().unwrap().load(Ordering::Relaxed)
                }
                Some(x) => x.load(Ordering::Relaxed),
            }
        },
        Config::<true, NT>,
    )
}

fn bench_init_rwlock_1(c: &mut Criterion) {
    bench_init_rwlock_::<1>(c, "init rwlock (parking_lot) => write / no concurency")
}
fn bench_init_rwlock_2(c: &mut Criterion) {
    bench_init_rwlock_::<2>(
        c,
        "init rwlock (parking_lot) => write / 2 concurent accesses time sum",
    )
}
fn bench_init_rwlock_4(c: &mut Criterion) {
    bench_init_rwlock_::<4>(
        c,
        "init rwlock (parking_lot) => write / 4 concurent accesses time sum",
    )
}
fn bench_init_rwlock_8(c: &mut Criterion) {
    bench_init_rwlock_::<8>(
        c,
        "init rwlock (parking_lot) => write / 8 concurent accesses time sum",
    )
}

fn bench_init_mut_lazy_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, NT>,
    )
}
fn bench_init_mut_lazy_1(c: &mut Criterion) {
    bench_init_mut_lazy_::<1>(c, "init mut lazy => write / no concurency")
}
fn bench_init_mut_lazy_2(c: &mut Criterion) {
    bench_init_mut_lazy_::<2>(c, "init mut lazy => write / 2 concurent accesses time sum")
}
fn bench_init_mut_lazy_4(c: &mut Criterion) {
    bench_init_mut_lazy_::<4>(c, "init mut lazy => write / 4 concurent accesses time sum")
}
fn bench_init_mut_lazy_8(c: &mut Criterion) {
    bench_init_mut_lazy_::<8>(c, "init mut lazy => write / 8 concurent accesses time sum")
}

fn bench_init_mut_lazy_r_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, NT>,
    )
}
fn bench_init_mut_lazy_r_1(c: &mut Criterion) {
    bench_init_mut_lazy_r_::<1>(c, "init mut lazy => read / no concurency")
}
fn bench_init_mut_lazy_r_2(c: &mut Criterion) {
    bench_init_mut_lazy_r_::<2>(c, "init mut lazy => read / 2 concurent accesses time sum")
}
fn bench_init_mut_lazy_r_4(c: &mut Criterion) {
    bench_init_mut_lazy_r_::<4>(c, "init mut lazy => read / 4 concurent accesses time sum")
}
fn bench_init_mut_lazy_r_8(c: &mut Criterion) {
    bench_init_mut_lazy_r_::<8>(c, "init mut lazy => read / 8 concurent accesses time sum")
}
fn bench_init_lazy_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(c, name, || Lazy::new(XX), |l| **l, Config::<true, NT>)
}
fn bench_init_lazy_1(c: &mut Criterion) {
    bench_init_lazy_::<1>(c, "init lazy =>  no concurency")
}
fn bench_init_lazy_2(c: &mut Criterion) {
    bench_init_lazy_::<2>(c, "init lazy =>  2 concurent accesses time sum")
}
fn bench_init_lazy_4(c: &mut Criterion) {
    bench_init_lazy_::<4>(c, "init lazy =>  4 concurent accesses time sum")
}
fn bench_init_lazy_8(c: &mut Criterion) {
    bench_init_lazy_::<8>(c, "init lazy =>  8 concurent accesses time sum")
}

fn bench_inited_mut_lazy_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || {
            let v = MutLazy::new(XX);
            &*v.read();
            v
        },
        |l| *l.write(),
        Config::<true, NT>,
    )
}
fn bench_inited_mut_lazy_2(c: &mut Criterion) {
    bench_inited_mut_lazy_::<2>(
        c,
        "inited mut lazy => write / 2 concurent accesses time sum",
    )
}
fn bench_inited_mut_lazy_4(c: &mut Criterion) {
    bench_inited_mut_lazy_::<4>(
        c,
        "inited mut lazy => write / 4 concurent accesses time sum",
    )
}
fn bench_inited_mut_lazy_8(c: &mut Criterion) {
    bench_inited_mut_lazy_::<8>(
        c,
        "inited mut lazy => write / 8 concurent accesses time sum",
    )
}

fn bench_inited_mut_lazy_r_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || {
            let v = MutLazy::new(XX);
            &*v.read();
            v
        },
        |l| *l.read(),
        Config::<true, NT>,
    )
}
fn bench_inited_mut_lazy_r_2(c: &mut Criterion) {
    bench_inited_mut_lazy_r_::<2>(c, "inited mut lazy => read / 2 concurent accesses time sum")
}
fn bench_inited_mut_lazy_r_4(c: &mut Criterion) {
    bench_inited_mut_lazy_r_::<4>(c, "inited mut lazy => read / 4 concurent accesses time sum")
}
fn bench_inited_mut_lazy_r_8(c: &mut Criterion) {
    bench_inited_mut_lazy_r_::<8>(c, "inited mut lazy => read / 8 concurent accesses time sum")
}
fn bench_inited_lazy_<const NT: usize>(c: &mut Criterion, name: &str) {
    bench_init(
        c,
        name,
        || {
            let v = Lazy::new(XX);
            &*v;
            v
        },
        |l| **l,
        Config::<true, NT>,
    )
}
fn bench_inited_lazy_2(c: &mut Criterion) {
    bench_inited_lazy_::<2>(c, "inited mut lazy =>  2 concurent accesses time sum")
}
fn bench_inited_lazy_4(c: &mut Criterion) {
    bench_inited_lazy_::<4>(c, "inited mut lazy =>  4 concurent accesses time sum")
}
fn bench_inited_lazy_8(c: &mut Criterion) {
    bench_inited_lazy_::<8>(c, "inited mut lazy =>  8 concurent accesses time sum")
}

fn bench_inited_lazy_access(c: &mut Criterion) {
    let v = Lazy::new(XX);
    &*v;
    c.bench_function("inited lazy access", |b| b.iter(|| *v));
}
fn bench_inited_mut_lazy_readlock(c: &mut Criterion) {
    let v = MutLazy::new(XX);
    &*v.read();
    c.bench_function("mut lazy read access", |b| b.iter(|| *v.read()));
}
fn bench_inited_mut_lazy_writelock(c: &mut Criterion) {
    let v = MutLazy::new(XX);
    &*v.read();
    c.bench_function("mut lazy write access", |b| b.iter(|| *v.write()));
}

struct YY;
impl Generator<[usize; 1024]> for YY {
    #[inline(always)]
    fn generate(&self) -> [usize; 1024] {
        let mut arr = [0; 1024];
        let mut i = 0;
        arr.iter_mut().for_each(|v| {
            *v = i;
            i += 1
        });
        arr
    }
}

fn bench_mut_lazy_multi_access_<const NT: usize, const INIT_THEN_READ: bool>(c: &mut Criterion, name: &str) {
    const ITER: usize = 100;
    static ID: [AtomicUsize; 64] = [
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ];
    static THREAD_IDS: AtomicUsize = AtomicUsize::new(0);
    #[dynamic]
    #[thread_local]
    static THREAD_ID: usize = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
    bench_init(
        c,
        name,
        || {
            let v = MutLazy::new(YY);
            v.read();
            v
        },
        |l| {
            let c0 = ID[*THREAD_ID].fetch_add(1, Ordering::Relaxed);
            for k in 0..ITER {
                if (INIT_THEN_READ && k > 2) || (!INIT_THEN_READ && (k+c0)%8 > 2) {
                    let l = l.read();
                    let o0 = l[0];
                    for (i, v) in l.iter().enumerate() {
                        let x = *v;
                        if x != o0 + i {
                            eprintln!(
                                "at read thread {} tryal id {}, loop id {}, elem {}, {} ne {}",
                                *THREAD_ID,
                                c0,
                                k,
                                i,
                                x,
                                o0 + i
                            );
                            std::thread::yield_now();
                            std::thread::sleep(std::time::Duration::from_secs(2));
                            std::thread::yield_now();
                            let o0 = l[0];
                            for (i, v) in l.iter().enumerate() {
                                let x = *v;
                                if x != o0 + i {
                                    eprintln!(
                                        "later read error thread {} tryal id {}, loop id {}, elem \
                                         {}, {} ne {}",
                                        *THREAD_ID,
                                        c0,
                                        k,
                                        i,
                                        x,
                                        o0 + i
                                    );
                                    eprintln!("this was a write error?");
                                    std::process::exit(1);
                                }
                            }
                            eprintln!("this was a read error?");
                            std::process::exit(1);
                        }
                    }
                } else {
                    let mut l = l.write();
                    let o0 = l[0];
                    for (i, v) in l.iter_mut().enumerate().rev() {
                        let x = *v;
                        if x != o0 + i {
                            eprintln!(
                                "at write thread {} tryial id {}, loop id {}, elem {}, {} ne {}",
                                *THREAD_ID,
                                c0,
                                k,
                                i,
                                x,
                                o0 + i
                            );
                            std::process::exit(1);
                        }
                        *v = i + k * 1000 + 1000000 * c0 + *THREAD_ID * 1_000_000_000;
                    }
                }
            }
        },
        Config::<false, NT>,
    )
}

fn bench_mut_lazy_multi_access_4(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<4,false>(
        c,
        "100 (read/write) large mut lazy access  / 4 concurent accesses time sum",
    )
}
fn bench_mut_lazy_multi_access_8(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<8,false>(
        c,
        "100 (read/write) large mut lazy access  / 8 concurent accesses time sum",
    )
}

fn bench_mut_lazy_multi_access_16(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<16,false>(
        c,
        "100 (read/write) large mut lazy access  / 16 concurent accesses time sum",
    )
}
fn bench_mut_lazy_multi_init_access_4(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<4,true>(
        c,
        "init then read large mut lazy access  / 4 concurent accesses time sum",
    )
}
fn bench_mut_lazy_multi_init_access_8(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<8,true>(
        c,
        "init then read large mut lazy access  / 8 concurent accesses time sum",
    )
}

fn bench_mut_lazy_multi_init_access_16(c: &mut Criterion) {
    bench_mut_lazy_multi_access_::<16,true>(
        c,
        "init then read large mut lazy access  / 16 concurent accesses time sum",
    )
}

fn bench_mut_rwlock_multi_access_<const NT: usize,const INIT_THEN_READ: bool>(c: &mut Criterion, name: &str) {
    const ITER: usize = 100;
    static ID: [AtomicUsize; 64] = [
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ];
    static THREAD_IDS: AtomicUsize = AtomicUsize::new(0);
    #[dynamic]
    #[thread_local]
    static THREAD_ID: usize = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
    bench_init(
        c,
        name,
        || RwLock::new(YY.generate()),
        |l| {
            let c0 = ID[*THREAD_ID].fetch_add(1, Ordering::Relaxed);
            for k in 0..ITER {
                if (INIT_THEN_READ && k > 2) || (!INIT_THEN_READ && (k+c0)%8 > 2) {
                    let l = l.read();
                    let o0 = l[0];
                    for (i, v) in l.iter().enumerate() {
                        let x = *v;
                        if x != o0 + i {
                            eprintln!("Surprise: test bug!");
                            std::process::exit(1);
                        }
                    }
                } else {
                    let mut l = l.write();
                    let o0 = l[0];
                    for (i, v) in l.iter_mut().enumerate().rev() {
                        let x = *v;
                        if x != o0 + i {
                            eprintln!("Surprise: test bug!");
                            std::process::exit(1);
                        }
                        *v = i + k * 1000 + 1000000 * c0 + *THREAD_ID * 1_000_000_000;
                    }
                }
            }
        },
        Config::<false, NT>,
    )
}
fn bench_mut_rwlock_multi_access_4(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<4,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) access  / 4 concurent accesses time sum",
    )
}

fn bench_mut_rwlock_multi_access_8(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<8,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) access  / 8 concurent accesses time sum",
    )
}
fn bench_mut_rwlock_multi_access_16(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<16,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) access  / 16 concurent accesses time sum",
    )
}
fn bench_mut_rwlock_multi_init_access_4(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<4,true>(
        c,
        "init then read large rwlock (parking_lot) access  / 4 concurent accesses time sum",
    )
}

fn bench_mut_rwlock_multi_init_access_8(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<8,true>(
        c,
        "init then read large rwlock (parking_lot) access  / 8 concurent accesses time sum",
    )
}
fn bench_mut_rwlock_multi_init_access_16(c: &mut Criterion) {
    bench_mut_rwlock_multi_access_::<16,true>(
        c,
        "init then read large rwlock (parking_lot) access  / 16 concurent accesses time sum",
    )
}

fn bench_mut_lazy_multi_fast_access_<const NT: usize, const INIT_THEN_READ: bool>(c: &mut Criterion, name: &str) {
    const ITER: usize = 100;
    static ID: [AtomicUsize; 32] = [
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ];
    static THREAD_IDS: AtomicUsize = AtomicUsize::new(0);
    #[dynamic]
    #[thread_local]
    static THREAD_ID: usize = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
    bench_init(
        c,
        name,
        || {
            let v = MutLazy::new(YY);
            v.read();
            v
        },
        |l| {
            let c0 = ID[*THREAD_ID].fetch_add(1, Ordering::Relaxed);
            let mut k = 0;
            while k < ITER {
                if (INIT_THEN_READ && k > 2) || (!INIT_THEN_READ && (k+c0)%8 > 2) {
                    let l = l.fast_read();
                    if let Some(l) = l {
                        let o0 = l[0];
                        for (i, v) in l.iter().enumerate() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!(
                                    "at read thread {} tryal id {}, loop id {}, elem {}, {} ne {}",
                                    *THREAD_ID,
                                    c0,
                                    k,
                                    i,
                                    x,
                                    o0 + i
                                );
                                std::thread::yield_now();
                                std::thread::sleep(std::time::Duration::from_secs(2));
                                std::thread::yield_now();
                                let o0 = l[0];
                                for (i, v) in l.iter().enumerate() {
                                    let x = *v;
                                    if x != o0 + i {
                                        eprintln!(
                                            "later read error thread {} tryal id {}, loop id {}, elem \
                                             {}, {} ne {}",
                                            *THREAD_ID,
                                            c0,
                                            k,
                                            i,
                                            x,
                                            o0 + i
                                        );
                                        eprintln!("this was a write error?");
                                        std::process::exit(1);
                                    }
                                }
                                eprintln!("this was a read error?");
                                std::process::exit(1);
                            }
                        }
                        k += 1
                    } else {
                        std::thread::yield_now();
                    }
                } else {
                    let l = l.fast_write();
                    if let Some(mut l) = l {
                        let o0 = l[0];
                        for (i, v) in l.iter_mut().enumerate().rev() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!(
                                    "at write thread {} tryial id {}, loop id {}, elem {}, {} ne {}",
                                    *THREAD_ID,
                                    c0,
                                    k,
                                    i,
                                    x,
                                    o0 + i
                                );
                                std::process::exit(1);
                            }
                            *v = i + k * 1000 + 1000000 * c0 + *THREAD_ID * 1_000_000_000;
                        }
                        k += 1
                    } else {
                        std::thread::yield_now()
                    }
                }
            }
        },
        Config::<false, NT>,
    )
}

fn bench_mut_lazy_multi_fast_access_4(c: &mut Criterion) {
    bench_mut_lazy_multi_fast_access_::<4,false>(
        c,
        "100 (read/write) large mut lazy fast access  / 4 concurent accesses time sum",
    )
}
fn bench_mut_lazy_multi_fast_access_8(c: &mut Criterion) {
    bench_mut_lazy_multi_fast_access_::<8,false>(
        c,
        "100 (read/write) large mut lazy fast access  / 8 concurent accesses time sum",
    )
}

fn bench_mut_lazy_multi_fast_access_16(c: &mut Criterion) {
    bench_mut_lazy_multi_fast_access_::<16,false>(
        c,
        "100 (read/write) large mut lazy fast access  / 16 concurent accesses time sum",
    )
}

fn bench_rwlock_multi_fast_access_<const NT: usize,const INIT_THEN_READ: bool>(c: &mut Criterion, name: &str) {
    const ITER: usize = 100;
    static ID: [AtomicUsize; 32] = [
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
        AtomicUsize::new(0),
    ];
    static THREAD_IDS: AtomicUsize = AtomicUsize::new(0);
    #[dynamic]
    #[thread_local]
    static THREAD_ID: usize = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
    bench_init(
        c,
        name,
        || RwLock::new(YY.generate()),
        |l| {
            let c0 = ID[*THREAD_ID].fetch_add(1, Ordering::Relaxed);
            let mut k = 0;
            while k < ITER {
                if (INIT_THEN_READ && k > 2) || (!INIT_THEN_READ && (k+c0)%8 > 2) {
                    let l = l.try_read();
                    if let Some(l) = l {
                        let o0 = l[0];
                        for (i, v) in l.iter().enumerate() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!("Surprise: test bug!");
                                std::process::exit(1);
                            }
                        }
                        k += 1;
                    } else {
                        std::thread::yield_now();
                    }
                } else {
                    let l = l.try_write();
                    if let Some(mut l) = l {
                        let o0 = l[0];
                        for (i, v) in l.iter_mut().enumerate().rev() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!("Surprise: test bug!");
                                std::process::exit(1);
                            }
                            *v = i + k * 1000 + 1000000 * c0 + *THREAD_ID * 1_000_000_000;
                        }
                        k+=1;
                    } else {
                        std::thread::yield_now()
                    }
                }
            }
        },
        Config::<false, NT>,
    )
}

fn bench_rwlock_multi_fast_access_4(c: &mut Criterion) {
    bench_rwlock_multi_fast_access_::<4,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) fast access  / 4 concurent accesses time sum",
    )
}

fn bench_rwlock_multi_fast_access_8(c: &mut Criterion) {
    bench_rwlock_multi_fast_access_::<8,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) fast access  / 8 concurent accesses time sum",
    )
}
fn bench_rwlock_multi_fast_access_16(c: &mut Criterion) {
    bench_rwlock_multi_fast_access_::<16,false>(
        c,
        "100 (read/write) large rwlock (parking_lot) fast access  / 16 concurent accesses time sum",
    )
}
criterion_group!(name=benches; config=Criterion::default();
targets=
//bench_init_rwlock_1,
//bench_init_rwlock_2,
//bench_init_rwlock_4,
//bench_init_rwlock_8,
//bench_init_mut_lazy_1,
//bench_init_mut_lazy_2,
//bench_init_mut_lazy_4,
//bench_init_mut_lazy_8,
//bench_init_mut_lazy_r_1,
//bench_init_mut_lazy_r_2,
//bench_init_mut_lazy_r_4,
//bench_init_mut_lazy_r_8,
//bench_init_lazy_1,
//bench_init_lazy_2,
//bench_init_lazy_4,
//bench_init_lazy_8,
//bench_inited_lazy_access,
//bench_inited_lazy_2,
//bench_inited_lazy_4,
//bench_inited_lazy_8,
//bench_inited_mut_lazy_writelock,
//bench_inited_mut_lazy_2,
//bench_inited_mut_lazy_4,
//bench_inited_mut_lazy_8,
//bench_inited_mut_lazy_readlock,
//bench_inited_mut_lazy_r_2,
//bench_inited_mut_lazy_r_4,
//bench_inited_mut_lazy_r_8,
//bench_mut_rwlock_multi_init_access_4,
//bench_mut_rwlock_multi_init_access_8,
//bench_mut_rwlock_multi_init_access_16,
//bench_mut_lazy_multi_init_access_4,
//bench_mut_lazy_multi_init_access_8,
//bench_mut_lazy_multi_init_access_16,
//bench_mut_rwlock_multi_access_4,
//bench_mut_rwlock_multi_access_8,
//bench_mut_rwlock_multi_access_16,
//bench_mut_lazy_multi_access_4,
//bench_mut_lazy_multi_access_8,
bench_mut_lazy_multi_access_16,
//bench_rwlock_multi_fast_access_4,
//bench_rwlock_multi_fast_access_8,
//bench_rwlock_multi_fast_access_16,
//bench_mut_lazy_multi_fast_access_4,
//bench_mut_lazy_multi_fast_access_8,
//bench_mut_lazy_multi_fast_access_16,
);
criterion_main!(benches);

struct Config<const MICRO_BENCH: bool, const NTHREAD: usize>;

fn bench_init<T, R, const MICRO_BENCH: bool, const NT: usize>(
    c: &mut Criterion,
    name: &str,
    build: impl Fn() -> T,
    access: impl Fn(&T) -> R + Sync,
    _: Config<MICRO_BENCH, NT>,
) {
    //static VMX: AtomicUsize = AtomicUsize::new(0);
    //static VP :RwLock<Option<AtomicU32>> = RwLock::const_new(RawRwLock::INIT,None);

    let started: AtomicUsize = AtomicUsize::new(NT);

    let vm: MutSynchronized<T> = MutSynchronized(UnsafeCell::new(build()));

    let tk = TickCounter::new();

    let (sender, receiver) = sync_channel(0);

    thread::scope(|s| {
        let test_init = {
            |sender: SyncSender<Duration>| loop {
                let mut expect = 0;
                loop {
                    match started.compare_exchange_weak(
                        expect,
                        expect + 1,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    ) {
                        Err(x) => {
                            if x == NT + 1 {
                                break;
                            }
                            if x == NT + 2 {
                                return;
                            }
                            if x < NT {
                                expect = x;
                                core::hint::spin_loop();
                                continue;
                            }
                        }
                        Ok(_) => continue,
                    }
                }
                let duration = if MICRO_BENCH {
                    tk.time(|| unsafe { access(&*vm.0.get()) })
                } else {
                    let s = std::time::Instant::now();
                    criterion::black_box(unsafe { access(&*vm.0.get()) });
                    s.elapsed()
                };
                //let duration = tk.time(|| { let mut l = VP.write();
                //    match &*l {
                //        None => {
                //            *l = Some(AtomicU32::new(33));
                //            l.as_ref().unwrap().load(Ordering::Relaxed)
                //        }
                //        Some(x) => x.load(Ordering::Relaxed),
                //    }
                //});
                //let duration = tk.time(|| VMX.fetch_add(1, Ordering::Relaxed));
                //println!("duration {:?}",duration);
                sender.send(duration).unwrap();
                expect = 2 * NT + 10;

                while let Err(x) = started.compare_exchange_weak(
                    expect,
                    expect + 1,
                    Ordering::AcqRel,
                    Ordering::Relaxed,
                ) {
                    if x >= 2 * NT + 10 {
                        expect = x;
                    }
                    core::hint::spin_loop();
                }
            }
        };

        let mut spawned = vec![];
        for _ in 0..NT {
            let sender = sender.clone();
            spawned.push(s.spawn(move |_| test_init(sender)));
        }

        c.bench_function(name, |b| {
            b.iter_custom(|iter| {
                let mut total = Duration::from_nanos(0);
                for _ in 0..iter {
                    unsafe { *vm.0.get() = build() };
                    //VMX.store(0, Ordering::Relaxed);
                    while started
                        .compare_exchange_weak(NT, NT + 1, Ordering::AcqRel, Ordering::Relaxed)
                        .is_err()
                    {
                        core::hint::spin_loop()
                    }

                    for _ in 0..NT {
                        total += loop {
                            match receiver.recv_timeout(Duration::from_secs(10)) {
                                Err(_) => {
                                    eprintln!("Timed out");
                                    std::process::exit(1);
                                }
                                Ok(v) => break v,
                            }
                        }
                    }
                    started
                        .compare_exchange(NT + 1, 2 * NT + 10, Ordering::AcqRel, Ordering::Relaxed)
                        .unwrap();
                    while started
                        .compare_exchange_weak(3 * NT + 10, 0, Ordering::AcqRel, Ordering::Relaxed)
                        .is_err()
                    {
                        core::hint::spin_loop()
                    }
                }
                total
            })
        });

        started
            .compare_exchange(NT, NT + 2, Ordering::AcqRel, Ordering::Relaxed)
            .unwrap();
        spawned.into_iter().for_each(|t| t.join().unwrap());
    })
    .unwrap();
}

#[derive(Copy, Clone)]
struct TickCounter(u64, f64);
impl TickCounter {
    pub fn new() -> TickCounter {
        let mut arr = [0; 10000];
        for _ in 1..1000 {
            let s = Self::raw_start();
            let e = Self::raw_end();
            black_box(e - s);
        }
        for v in arr.iter_mut() {
            let s = Self::raw_start();
            let e = Self::raw_end();
            *v = e - s;
        }
        arr.sort_unstable();
        for k in 0..1000 {
            arr[k] = arr[1000];
        }
        for k in 9000..10000 {
            arr[k] = arr[8999];
        }
        let s = arr.iter().fold(0, |cur, v| cur + *v);
        let zero = s / 10000;
        let mut k = 0;
        let mut arr = [0f64; 10000];
        loop {
            let s = std::time::Instant::now();
            let s0 = Self::raw_start();
            for _ in 1..1000 {
                let s = Self::raw_start();
                let e = Self::raw_end();
                black_box(e - s);
            }
            let e0 = Self::raw_end();
            let e = std::time::Instant::now();
            if s0 > e0 {
                continue;
            }
            let l = e.duration_since(s).as_nanos() as f64;
            let dst = (e0 - s0) as f64;
            arr[k] = l / dst;
            k += 1;
            if k == 10000 {
                break;
            }
        }
        arr.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for k in 0..1000 {
            arr[k] = arr[1000];
        }
        for k in 9000..10000 {
            arr[k] = arr[8999];
        }
        let s = arr.iter().fold(0f64, |cur, v| cur + *v);
        TickCounter(zero, s / 10000f64)
    }
    pub fn time<R, F: FnOnce() -> R>(&self, f: F) -> std::time::Duration {
        let s = Self::raw_start();
        black_box(f());
        let e = Self::raw_end();
        let v = (e - s) as f64;
        let v = (v - self.0 as f64) * self.1;
        let v = v.round();
        if v > 0f64 {
            std::time::Duration::from_nanos(v as u64)
        } else {
            std::time::Duration::from_nanos(0)
        }
    }
    #[inline(always)]
    fn raw_start() -> u64 {
        let high: u64;
        let low: u64;
        let cpuid_ask: u64 = 0;
        unsafe {
            asm!(
                 "cpuid",
                 "rdtsc",
                 out("rdx") high,
                 inout("rax") cpuid_ask => low,
                 out("rbx") _,
                 out("rcx") _,
                 options(nostack,preserves_flags)
            )
        };
        (high << 32) | low
    }
    #[inline(always)]
    fn raw_end() -> u64 {
        let high: u64;
        let low: u64;
        unsafe {
            asm!(
                 "rdtscp",
                 "mov {high}, rdx",
                 "mov {low}, rax",
                 "mov rax, 0",
                 "cpuid",
                 high = out(reg) high,
                 low = out(reg) low,
                 out("rax")  _,
                 out("rbx")  _,
                 out("rcx")  _,
                 out("rdx")  _,
                 options(nostack,preserves_flags)
            )
        };
        (high << 32) | low
    }
}
