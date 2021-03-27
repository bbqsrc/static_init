// Copyright 2021 Olivier Kannengieser
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#![feature(test)]
#![feature(thread_local)]
#![feature(asm)]

extern crate static_init;
use static_init::Lazy;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
//use ctor::ctor;
//use static_init::dynamic;

use core::cell::UnsafeCell;
use parking_lot::RwLock;
use crossbeam::thread;
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
fn bench_init_rwlock(c: &mut Criterion) {
    bench_init(
        c,
        "init rwlock / 8 concurent accesses time sum",
        || RwLock::<Option<AtomicUsize>>::new(None),
        |v| { let mut l = v.write();
                    match &*l {
                        None => {
                            *l = Some(AtomicUsize::new(33));
                            l.as_ref().unwrap().load(Ordering::Relaxed)
                        }
                        Some(x) => x.load(Ordering::Relaxed),
                    }
                },
    )
}

fn bench_init_mut_lazy(c: &mut Criterion) {
    bench_init(
        c,
        "init MutLazy => write_lock / 8 concurent accesses time sum",
        || MutLazy::new(XX),
        |l| *l.write_lock(),
    )
}
fn bench_init_mut_lazy_r(c: &mut Criterion) {
    bench_init(
        c,
        "init MutLazy => read_lock / 8 concurent accesses time sum",
        || MutLazy::new(XX),
        |l| *l.read_lock(),
    )
}
fn bench_init_lazy(c: &mut Criterion) {
    bench_init(
        c,
        "init Lazy / 8 concurents accesses time sum",
        || Lazy::new(XX),
        |l| **l,
    )
}
fn bench_inited_mut_lazy(c: &mut Criterion) {
    bench_init(
        c,
        "inited MutLazy => write_lock / 8 concurent accesses time sum",
        || {let v = MutLazy::new(XX); &*v.read_lock(); v},
        |l| *l.write_lock(),
    )
}
fn bench_inited_mut_lazy_r(c: &mut Criterion) {
    bench_init(
        c,
        "inited MutLazy => read_lock / 8 concurent accesses time sum",
        || {let v = MutLazy::new(XX); &*v.read_lock(); v},
        |l| *l.read_lock(),
    )
}
fn bench_inited_lazy(c: &mut Criterion) {
    bench_init(
        c,
        "inited Lazy / 8 concurents accesses time sum",
        || {let v = Lazy::new(XX); &*v; v},
        |l| **l,
    )
}
fn bench_inited_lazy_access(c: &mut Criterion) {
    let v = Lazy::new(XX);
    &*v;
    c.bench_function("lazy access", |b| b.iter(|| { *v}));
}
fn bench_inited_mut_lazy_readlock(c: &mut Criterion) {
    let v = MutLazy::new(XX);
    &*v.read_lock();
    c.bench_function("mut lazy read access", |b| b.iter(|| { *v.read_lock()}));
}
fn bench_inited_mut_lazy_writelock(c: &mut Criterion) {
    let v = MutLazy::new(XX);
    &*v.read_lock();
    c.bench_function("mut lazy write access", |b| b.iter(|| { *v.write_lock()}));
}

criterion_group!(name=benches; config=Criterion::default();
    targets=bench_init_rwlock,bench_init_mut_lazy,bench_init_mut_lazy_r,bench_init_lazy,
    bench_inited_mut_lazy,bench_inited_mut_lazy_r,bench_inited_lazy,
    bench_inited_lazy_access, bench_inited_mut_lazy_readlock,bench_inited_mut_lazy_writelock
    );
criterion_main!(benches);

fn bench_init<T, R>(
    c: &mut Criterion,
    name: &str,
    build: impl Fn() -> T,
    access: impl Fn(&T) -> R + Sync,
) {
    const NT: usize = 8;
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
                let duration = tk.time(|| unsafe { access(&*vm.0.get()) });
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
                loop {
                    match started.compare_exchange_weak(
                        expect,
                        expect + 1,
                        Ordering::AcqRel,
                        Ordering::Relaxed,
                    ) {
                        Err(x) => {
                            if x >= 2 * NT + 10 {
                                expect = x;
                            }
                            core::hint::spin_loop();
                            continue;
                        }
                        Ok(_) => break,
                    }
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
                        total += receiver.recv().unwrap();
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
        arr.sort();
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
