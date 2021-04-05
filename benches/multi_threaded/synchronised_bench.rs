use std::time::Duration;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use core::cell::UnsafeCell;
use crossbeam::thread;

use criterion::{black_box,BenchmarkGroup,measurement::WallTime,BenchmarkId};

use crate::tick_counter::TickCounter;

struct MutSynchronized<T>(UnsafeCell<T>);

unsafe impl<T> Sync for MutSynchronized<T> {}

pub struct Config<const MICRO_BENCH: bool, const NTHREAD: usize, const NT_SART: usize>;

pub fn synchro_bench<T, R, const MICRO_BENCH: bool, const NT: usize, const NT_START: usize>(
    c: &mut BenchmarkGroup<WallTime>,
    name: &str,
    build: impl Fn() -> T,
    access: impl Fn(&T) -> R + Sync,
    _: Config<MICRO_BENCH, NT, NT_START>,
) {

    let started: AtomicUsize = AtomicUsize::new(NT_START);

    let vm: MutSynchronized<T> = MutSynchronized(UnsafeCell::new(build()));

    let tk = TickCounter::new();

    assert!(NT_START <= NT);

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
                            if x == NT_START + 1 {
                                break;
                            }
                            if x == NT + 2 {
                                return;
                            }
                            if x < NT_START {
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
                    black_box(unsafe { access(&*vm.0.get()) });
                    s.elapsed()
                };
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
                        .compare_exchange_weak(NT_START, NT_START + 1, Ordering::AcqRel, Ordering::Relaxed)
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
                        .compare_exchange(NT_START + 1, 2 * NT + 10, Ordering::AcqRel, Ordering::Relaxed)
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

pub fn synchro_bench_input<I,T, R, const MICRO_BENCH: bool, const NT: usize, const NT_START: usize>(
    c: &mut BenchmarkGroup<WallTime>,
    id: BenchmarkId,
    input: &I,
    build: impl Fn(&I) -> T,
    access: impl Fn(&T) -> R + Sync,
    _: Config<MICRO_BENCH, NT, NT_START>,
) {

    let started: AtomicUsize = AtomicUsize::new(NT_START);

    let vm: MutSynchronized<T> = MutSynchronized(UnsafeCell::new(build(input)));

    let tk = TickCounter::new();

    let (sender, receiver) = sync_channel(0);

    assert!(NT_START <= NT);

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
                            if x == NT_START + 1 {
                                break;
                            }
                            if x == NT + 2 {
                                return;
                            }
                            if x < NT_START {
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
                    black_box(unsafe { access(&*vm.0.get()) });
                    s.elapsed()
                };
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
                    for _ in 1..32 {core::hint::spin_loop()};
                }
            }
        };

        let mut spawned = vec![];
        for _ in 0..NT {
            let sender = sender.clone();
            spawned.push(s.spawn(move |_| test_init(sender)));
        }

        c.bench_with_input(id, input, |b, input| {
            b.iter_custom(|iter| {
                let mut total = Duration::from_nanos(0);
                for _ in 0..iter {
                    unsafe { *vm.0.get() = build(input) };
                    //VMX.store(0, Ordering::Relaxed);
                    while started
                        .compare_exchange_weak(NT_START, NT_START + 1, Ordering::AcqRel, Ordering::Relaxed)
                        .is_err()
                    {
                        for _ in 1..8 {core::hint::spin_loop()};
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
                        .compare_exchange(NT_START + 1, 2 * NT + 10, Ordering::AcqRel, Ordering::Relaxed)
                        .unwrap();
                    while started
                        .compare_exchange_weak(3 * NT + 10, 0, Ordering::AcqRel, Ordering::Relaxed)
                        .is_err()
                    {
                        for _ in 1..32 {core::hint::spin_loop()};
                    }
                }
                total
            })
        });

        started
            .compare_exchange(NT_START, NT + 2, Ordering::AcqRel, Ordering::Relaxed)
            .unwrap();
        spawned.into_iter().for_each(|t| t.join().unwrap());
    })
    .unwrap();
}

