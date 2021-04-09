#![cfg_attr(feature = "thread_local", feature(thread_local))]

mod synchronised_bench;
use synchronised_bench::{synchro_bench_input, Config};

mod tick_counter;

use static_init::{dynamic, Generator, Lazy, LockedLazy};

use parking_lot::{
    lock_api::{MappedRwLockReadGuard, MappedRwLockWriteGuard, RwLockReadGuard, RwLockWriteGuard},
    RawRwLock, RwLock,
};

use criterion::{
    criterion_group, criterion_main, measurement::WallTime, AxisScale, BatchSize, BenchmarkGroup,
    BenchmarkId, Criterion, PlotConfiguration,
};

use lazy_static::lazy::Lazy as STLazy;

use std::time::Duration;

use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

struct Xx;
impl Generator<i32> for Xx {
    #[inline(always)]
    fn generate(&self) -> i32 {
        10
    }
}

fn occupy_for(duration: Duration) {
    let e = std::time::Instant::now();
    while e.elapsed() < duration {
        for _ in 0..1024 {
            std::hint::spin_loop();
        }
    }
}

struct W(Duration);
impl Generator<i32> for W {
    #[inline(always)]
    fn generate(&self) -> i32 {
        occupy_for(self.0);
        10
    }
}

struct RwMut<T, F>(RwLock<Option<T>>, AtomicI32, F);

struct Guard<'a>(&'a AtomicI32);

impl<'a> Drop for Guard<'a> {
    fn drop(&mut self) {
        self.0.store(2, Ordering::Release);
    }
}

impl<T, F> RwMut<T, F>
where
    F: Generator<T>,
{
    fn new(f: F) -> Self {
        Self(RwLock::new(None), AtomicI32::new(0), f)
    }
    //fn try_write(&self) -> Result<MappedRwLockWriteGuard<'_,RawRwLock,T>,()> {
    //
    //    let status = self.1.load(Ordering::Acquire);

    //    if status != 1 {
    //        return Err(());
    //    }

    //    let l = self.0.write();

    //    Ok(RwLockWriteGuard::map(l,|v| v.as_mut().unwrap()))

    //}
    //fn try_read(&self) -> Result<MappedRwLockReadGuard<'_,RawRwLock,T>,()> {
    //
    //    let status = self.1.load(Ordering::Acquire);

    //    if status != 1 {
    //        return Err(());
    //    }

    //    let l = self.0.read();

    //    Ok(RwLockReadGuard::map(l,|v| v.as_ref().unwrap()))

    //}
    //fn fast_try_write(&self) -> Option<Result<MappedRwLockWriteGuard<'_,RawRwLock,T>,()>> {
    //
    //    let status = self.1.load(Ordering::Acquire);

    //    if status != 1 {
    //        return Some(Err(()));
    //    }

    //    let l = self.0.try_write();

    //    l.map(|l| Ok(RwLockWriteGuard::map(l,|v| v.as_mut().unwrap())))

    //}
    //fn fast_try_read(&self) -> Option<Result<MappedRwLockReadGuard<'_,RawRwLock,T>,()>> {
    //
    //    let status = self.1.load(Ordering::Acquire);

    //    if status != 1 {
    //        return Some(Err(()));
    //    }

    //    let l = self.0.try_read();

    //    l.map(|l| Ok(RwLockReadGuard::map(l,|v| v.as_ref().unwrap())))

    //}
    fn fast_write(&self) -> Option<MappedRwLockWriteGuard<'_, RawRwLock, T>> {
        let mut l = self.0.try_write()?;

        let status = self.1.load(Ordering::Acquire);

        if status == 0 {
            let g = Guard(&self.1);
            *l = Some(self.2.generate());
            std::mem::forget(g);
            self.1.store(1, Ordering::Release)
        } else if status == 2 {
            panic!("Poisoned accceess");
        }

        Some(RwLockWriteGuard::map(l, |v| v.as_mut().unwrap()))
    }
    fn fast_read(&self) -> Option<MappedRwLockReadGuard<'_, RawRwLock, T>> {
        let mut status = self.1.load(Ordering::Acquire);

        if status == 0 {
            let mut l = self.0.try_write()?;
            status = self.1.load(Ordering::Acquire);
            if status == 0 {
                let g = Guard(&self.1);
                *l = Some(self.2.generate());
                std::mem::forget(g);
                self.1.store(1, Ordering::Release)
            } else if status == 2 {
                panic!("Poisoned accceess");
            }
            return Some(RwLockReadGuard::map(RwLockWriteGuard::downgrade(l), |v| {
                v.as_ref().unwrap()
            }));
        } else if status == 2 {
            panic!("Poisoned accceess");
        }

        let l = self.0.try_read();

        l.map(|l| RwLockReadGuard::map(l, |v| v.as_ref().unwrap()))
    }
    fn write(&self) -> MappedRwLockWriteGuard<'_, RawRwLock, T> {
        let mut l = self.0.write();

        let status = self.1.load(Ordering::Acquire);

        if status == 0 {
            let g = Guard(&self.1);
            *l = Some(self.2.generate());
            std::mem::forget(g);
            self.1.store(1, Ordering::Release)
        } else if status == 2 {
            panic!("Poisoned accceess");
        }

        RwLockWriteGuard::map(l, |v| v.as_mut().unwrap())
    }
    fn read(&self) -> MappedRwLockReadGuard<'_, RawRwLock, T> {
        let mut status = self.1.load(Ordering::Acquire);

        if status == 0 {
            let mut l = self.0.write();
            status = self.1.load(Ordering::Acquire);
            if status == 0 {
                let g = Guard(&self.1);
                *l = Some(self.2.generate());
                std::mem::forget(g);
                self.1.store(1, Ordering::Release)
            } else if status == 2 {
                panic!("Poisoned accceess");
            }
            return RwLockReadGuard::map(RwLockWriteGuard::downgrade(l), |v| v.as_ref().unwrap());
        } else if status == 2 {
            panic!("Poisoned accceess");
        }

        let l = self.0.read();

        RwLockReadGuard::map(l, |v| v.as_ref().unwrap())
    }
}

fn do_bench<'a, R, T, F: Fn() -> T + Copy, A: Fn(&T) -> R + Sync>(
    gp: &mut BenchmarkGroup<'a, WallTime>,
    name: &str,
    init: F,
    access: A,
) {
    macro_rules! mb {
        ($t:literal) => {
            mb!($t - $t)
        };
        ($t:literal - $l:literal) => {
            synchro_bench_input(
                gp,
                BenchmarkId::new(name, $t),
                &$t,
                |_| init(),
                |l| access(l),
                Config::<true, $t, $l, true>,
            );
        };
    }

    gp.measurement_time(Duration::from_secs(3));

    gp.bench_with_input(BenchmarkId::new(name, 1), &1, |b, _| {
        b.iter_batched(init, |l| access(&l), BatchSize::SmallInput)
    });
    mb!(2);

    gp.measurement_time(Duration::from_secs(5));

    mb!(4);

    gp.measurement_time(Duration::from_secs(8));

    mb!(8 - 8);

    gp.measurement_time(Duration::from_secs(15));

    mb!(16 - 8);

    mb!(32 - 8);
}

fn bench_init(c: &mut Criterion) {
    let mut gp = c.benchmark_group("Init Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Locked Lazy read", || LockedLazy::new(Xx), |l| *l.read());

    do_bench(
        &mut gp,
        "Locked Lazy write",
        || LockedLazy::new(Xx),
        |l| *l.write(),
    );

    let init = || RwMut::new(|| 33);

    do_bench(&mut gp, "Locked Lazy PkLot read", init, |l| *l.read());

    do_bench(&mut gp, "Locked Lazy PkLot write", init, |l| *l.write());

    gp.finish();

    let mut gp = c.benchmark_group("Init Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Lazy", || Lazy::new(Xx), |l| **l);

    let init = || STLazy::<i32>::INIT;

    let access = |l: &STLazy<i32>| {
        let r: &'static STLazy<i32> = unsafe { &*(l as *const STLazy<i32>) };
        *r.get(|| 33)
    };

    do_bench(&mut gp, "static_lazy::Lazy", init, access);

    gp.finish();


    let mut gp = c.benchmark_group("Init (1us) Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Locked Lazy read",
        || LockedLazy::new(W(Duration::from_micros(1))),
        |l| *l.read(),
    );

    do_bench(
        &mut gp,
        "Locked Lazy write",
        || LockedLazy::new(W(Duration::from_micros(1))),
        |l| *l.write(),
    );

    let init = || {
        RwMut::new(|| {
            occupy_for(Duration::from_micros(1));
            33
        })
    };

    do_bench(&mut gp, "Locked Lazy PkLot read", init, |l| *l.read());

    do_bench(&mut gp, "Locked Lazy PkLot write", init, |l| *l.write());

    gp.finish();


    let mut gp = c.benchmark_group("Init (5us) Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Locked Lazy read",
        || LockedLazy::new(W(Duration::from_micros(5))),
        |l| *l.read(),
    );

    do_bench(
        &mut gp,
        "Locked Lazy write",
        || LockedLazy::new(W(Duration::from_micros(5))),
        |l| *l.write(),
    );

    let init = || {
        RwMut::new(|| {
            occupy_for(Duration::from_micros(5));
            33
        })
    };

    do_bench(&mut gp, "Locked Lazy PkLot read", init, |l| *l.read());

    do_bench(&mut gp, "Locked Lazy PkLot write", init, |l| *l.write());

    gp.finish();


    let mut gp = c.benchmark_group("Init (10us) Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Locked Lazy read",
        || LockedLazy::new(W(Duration::from_micros(10))),
        |l| *l.read(),
    );

    do_bench(
        &mut gp,
        "Locked Lazy write",
        || LockedLazy::new(W(Duration::from_micros(10))),
        |l| *l.write(),
    );

    let init = || {
        RwMut::new(|| {
            occupy_for(Duration::from_micros(10));
            33
        })
    };

    do_bench(&mut gp, "Locked Lazy PkLot read", init, |l| *l.read());

    do_bench(&mut gp, "Locked Lazy PkLot write", init, |l| *l.write());

    gp.finish();


    let mut gp = c.benchmark_group("Init (20us) Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Locked Lazy read",
        || LockedLazy::new(W(Duration::from_micros(20))),
        |l| *l.read(),
    );

    do_bench(
        &mut gp,
        "Locked Lazy write",
        || LockedLazy::new(W(Duration::from_micros(20))),
        |l| *l.write(),
    );

    let init = || {
        RwMut::new(|| {
            occupy_for(Duration::from_micros(20));
            33
        })
    };

    do_bench(&mut gp, "Locked Lazy PkLot read", init, |l| *l.read());

    do_bench(&mut gp, "Locked Lazy PkLot write", init, |l| *l.write());

    gp.finish();


    let mut gp = c.benchmark_group("Init (1us) Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Lazy",
        || Lazy::new(W(Duration::from_micros(1))),
        |l| **l,
    );

    let init = || STLazy::<i32>::INIT;

    let access = |l: &STLazy<i32>| {
        let r: &'static STLazy<i32> = unsafe { &*(l as *const STLazy<i32>) };
        *r.get(|| {
            occupy_for(Duration::from_micros(1));
            33
        })
    };

    do_bench(&mut gp, "static_lazy::Lazy", init, access);

    gp.finish();


    let mut gp = c.benchmark_group("Init (5us) Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Lazy",
        || Lazy::new(W(Duration::from_micros(5))),
        |l| **l,
    );

    let init = || STLazy::<i32>::INIT;

    let access = |l: &STLazy<i32>| {
        let r: &'static STLazy<i32> = unsafe { &*(l as *const STLazy<i32>) };
        *r.get(|| {
            occupy_for(Duration::from_micros(5));
            33
        })
    };

    do_bench(&mut gp, "static_lazy::Lazy", init, access);

    gp.finish();


    let mut gp = c.benchmark_group("Init (20us) Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Lazy",
        || Lazy::new(W(Duration::from_micros(20))),
        |l| **l,
    );

    let init = || STLazy::<i32>::INIT;

    let access = |l: &STLazy<i32>| {
        let r: &'static STLazy<i32> = unsafe { &*(l as *const STLazy<i32>) };
        *r.get(|| {
            occupy_for(Duration::from_micros(20));
            33
        })
    };

    do_bench(&mut gp, "static_lazy::Lazy", init, access);

    gp.finish();
}

#[dynamic(lesser_lazy)]
static QL: i32 = 33;

#[dynamic(lesser_lazy)]
static mut QLM: i32 = 33;

#[dynamic(lesser_lazy, drop)]
static mut QLMD: i32 = 33;

fn bench_access(c: &mut Criterion) {
    let init = || {
        let l = LockedLazy::new(Xx);
        l.read();
        l
    };

    let rw_init = || {
        let l = RwMut::new(|| 33);
        let _ = l.read();
        l
    };

    macro_rules! fast_access {
        ($acc:expr) => {{
            let v;
            loop {
                match $acc {
                    None => {
                        //for _ in 0..64 {
                        //    core::hint::spin_loop()
                        //}
                        ()
                    }
                    Some(l) => {
                        v = *l;
                        break;
                    }
                }
            }
            v
        }};
    }


    let mut gp = c.benchmark_group("Access Read Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Locked Lazy", init, |l| *l.read());

    do_bench(&mut gp, "LesserLocked Lazy", || (), |_| *QLM.read());

    do_bench(&mut gp, "LesserLocked LazyDrop", || (), |_| *QLMD.read());

    do_bench(&mut gp, "Locked Lazy PkLot", rw_init, |l| *l.read());

    gp.finish();

    let mut gp = c.benchmark_group("Access Write Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Locked Lazy", init, |l| *l.write());

    do_bench(&mut gp, "LesserLocked Lazy", || (), |_| *QLM.write());

    do_bench(&mut gp, "LesserLocked LazyDrop", || (), |_| *QLMD.write());

    do_bench(&mut gp, "Locked Lazy PkLot", rw_init, |l| *l.write());

    gp.finish();

    let mut gp = c.benchmark_group("Fast Read Access Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Locked Lazy", init, |l| {
        fast_access!(l.fast_read())
    });

    do_bench(
        &mut gp,
        "LesserLocked Lazy",
        || (),
        |_| fast_access!(QLM.fast_read()),
    );

    do_bench(
        &mut gp,
        "LesserLocked LazyDrop",
        || (),
        |_| fast_access!(QLMD.fast_read()),
    );

    do_bench(&mut gp, "Locked Lazy PkLot", rw_init, |l| {
        fast_access!(l.fast_read())
    });

    gp.finish();

    let mut gp = c.benchmark_group("Fast Write Access Locked Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(&mut gp, "Locked Lazy", init, |l| {
        fast_access!(l.fast_write())
    });

    do_bench(
        &mut gp,
        "LesserLocked Lazy",
        || (),
        |_| fast_access!(QLM.fast_write()),
    );

    do_bench(
        &mut gp,
        "LesserLocked LazyDrop",
        || (),
        |_| fast_access!(QLMD.fast_write()),
    );

    do_bench(&mut gp, "Locked Lazy PkLot", rw_init, |l| {
        fast_access!(l.fast_write())
    });

    gp.finish();

    let mut gp = c.benchmark_group("Access Thread Scaling");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    do_bench(
        &mut gp,
        "Lazy",
        || {
            let l = Lazy::new(Xx);
            let _ = *l;
            l
        },
        |l| **l,
    );

    do_bench(&mut gp, "LesserLazy", || (), |_| *QL);

    let init = || {
        let l = STLazy::<i32>::INIT;
        let r: &'static STLazy<i32> = unsafe { &*(&l as *const STLazy<i32>) };
        r.get(|| 33);
        l
    };

    let access = |l: &STLazy<i32>| {
        let r: &'static STLazy<i32> = unsafe { &*(l as *const STLazy<i32>) };
        *r.get(|| 33)
    };

    do_bench(&mut gp, "static_lazy::Lazy", init, access);

    gp.finish();
}

criterion_group! {name=multi; config=Criterion::default();
targets=
bench_init,
bench_access,
bench_heavy,
fast_bench_heavy
}

criterion_main! {multi}

struct Yy(usize);

impl Generator<Vec<usize>> for Yy {
    fn generate(&self) -> Vec<usize> {
        let mut v = Vec::new();
        for i in 0..self.0 {
            v.push(i)
        }
        v
    }
}

macro_rules! heavy_bench {
    ($name:ident, $type:ident, $read_lock:ident, $write_lock:ident) => {
        fn $name<'a, const INIT_THEN_READ: bool>(
            gp: &mut BenchmarkGroup<'a, WallTime>,
            name: &str,
            size: usize,
        ) {
            const ITER: usize = 100;

            #[dynamic(0)]
            static ID: Vec<AtomicUsize> = {
                let mut v = vec![];
                for _ in 0..128 {
                    v.push(AtomicUsize::new(0));
                }
                v
            };

            static THREAD_IDS: AtomicUsize = AtomicUsize::new(0);

            thread_local!{
            static THREAD_ID: usize = THREAD_IDS.fetch_add(1, Ordering::Relaxed);
            }

            let init = || {
                let v = $type::new(Yy(size));
                let _ = v.read();
                v
            };

            let access = |l: &$type<Vec<usize>, Yy>| {
                let c0 = unsafe { ID[THREAD_ID.with(|f| *f)].fetch_add(1, Ordering::Relaxed) };
                let mut k = 0;
                while k < ITER {
                    if (INIT_THEN_READ && k > 2) || (!INIT_THEN_READ && (k + c0) % 8 > 2) {
                        let l = $read_lock!(l);
                        let o0 = l[0];
                        for (i, v) in l.iter().enumerate() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!(
                                    "at read thread {} tryal id {}, loop id {}, elem {}, {} ne {}",
                                    THREAD_ID.with(|f| *f),
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
                                            "later read error thread {} tryal id {}, loop id {}, \
                                     elem {}, {} ne {}",
                                            THREAD_ID.with(|f| *f),
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
                        let mut l = $write_lock!(l);
                        let o0 = l[0];
                        for (i, v) in l.iter_mut().enumerate().rev() {
                            let x = *v;
                            if x != o0 + i {
                                eprintln!(
                                    "at write thread {} tryial id {}, loop id {}, elem {}, {} ne \
                             {}",
                                    THREAD_ID.with(|f| *f),
                                    c0,
                                    k,
                                    i,
                                    x,
                                    o0 + i
                                );
                                std::process::exit(1);
                            }
                            *v = i + k * 1000 + 1000000 * c0 + THREAD_ID.with(|f| *f) * 1_000_000_000;
                        }
                    }
                    k += 1;
                }
            };

            synchro_bench_input(
                gp,
                BenchmarkId::new(name, size),
                &size,
                |_| init(),
                access,
                Config::<false, 8, 8, true>,
            );
        }
    };
}

macro_rules! read_access {
    ($l:ident) => {
        $l.read()
    };
}
macro_rules! write_access {
    ($l:ident) => {
        $l.write()
    };
}

macro_rules! fast_read_access {
    ($l:ident) => {
        if let Some(l) = $l.fast_read() {
            l
        } else {
            std::thread::yield_now();
            continue;
        }
    };
}
macro_rules! fast_write_access {
    ($l:ident) => {
        if let Some(l) = $l.fast_write() {
            l
        } else {
            std::thread::yield_now();
            continue;
        }
    };
}

heavy_bench! {heavy_mutlazy,LockedLazy, read_access, write_access}
heavy_bench! {heavy_rwmut,RwMut, read_access, write_access}

heavy_bench! {heavy_fast_mutlazy,LockedLazy, fast_read_access, fast_write_access}
heavy_bench! {heavy_fast_rwmut,RwMut, fast_read_access, fast_write_access}

fn bench_heavy(c: &mut Criterion) {
    let mut gp = c.benchmark_group("Heavy access reads / writes");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    heavy_mutlazy::<false>(&mut gp, "Locked Lazy", 1024);

    heavy_mutlazy::<false>(&mut gp, "Locked Lazy", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_mutlazy::<false>(&mut gp, "Locked Lazy", 4096);

    heavy_mutlazy::<false>(&mut gp, "Locked Lazy", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_mutlazy::<false>(&mut gp, "Locked Lazy", 16384);

    gp.measurement_time(Duration::from_secs(3));

    heavy_rwmut::<false>(&mut gp, "RwLock", 1024);

    heavy_rwmut::<false>(&mut gp, "RwLock", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_rwmut::<false>(&mut gp, "RwLock", 4096);

    heavy_rwmut::<false>(&mut gp, "RwLock", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_rwmut::<false>(&mut gp, "RwLock", 16384);

    gp.finish();

    let mut gp = c.benchmark_group("Heavy access writes then read");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    heavy_mutlazy::<true>(&mut gp, "Locked Lazy", 1024);

    heavy_mutlazy::<true>(&mut gp, "Locked Lazy", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_mutlazy::<true>(&mut gp, "Locked Lazy", 4096);

    heavy_mutlazy::<true>(&mut gp, "Locked Lazy", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_mutlazy::<true>(&mut gp, "Locked Lazy", 16384);

    gp.measurement_time(Duration::from_secs(3));

    heavy_rwmut::<true>(&mut gp, "RwLock", 1024);

    heavy_rwmut::<true>(&mut gp, "RwLock", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_rwmut::<true>(&mut gp, "RwLock", 4096);

    heavy_rwmut::<true>(&mut gp, "RwLock", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_rwmut::<true>(&mut gp, "RwLock", 16384);

    gp.finish();
}

fn fast_bench_heavy(c: &mut Criterion) {
    let mut gp = c.benchmark_group("Heavy fast access reads / writes");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    heavy_fast_mutlazy::<false>(&mut gp, "Locked Lazy", 1024);

    heavy_fast_mutlazy::<false>(&mut gp, "Locked Lazy", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_fast_mutlazy::<false>(&mut gp, "Locked Lazy", 4096);

    heavy_fast_mutlazy::<false>(&mut gp, "Locked Lazy", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_fast_mutlazy::<false>(&mut gp, "Locked Lazy", 16384);

    gp.measurement_time(Duration::from_secs(3));

    heavy_fast_rwmut::<false>(&mut gp, "RwLock", 1024);

    heavy_fast_rwmut::<false>(&mut gp, "RwLock", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_fast_rwmut::<false>(&mut gp, "RwLock", 4096);

    heavy_fast_rwmut::<false>(&mut gp, "RwLock", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_fast_rwmut::<false>(&mut gp, "RwLock", 16384);

    gp.finish();

    let mut gp = c.benchmark_group("Heavy fast access writes then read");

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    heavy_fast_mutlazy::<true>(&mut gp, "Locked Lazy", 1024);

    heavy_fast_mutlazy::<true>(&mut gp, "Locked Lazy", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_fast_mutlazy::<true>(&mut gp, "Locked Lazy", 4096);

    heavy_fast_mutlazy::<true>(&mut gp, "Locked Lazy", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_fast_mutlazy::<true>(&mut gp, "Locked Lazy", 16384);

    gp.measurement_time(Duration::from_secs(3));

    heavy_fast_rwmut::<true>(&mut gp, "RwLock", 1024);

    heavy_fast_rwmut::<true>(&mut gp, "RwLock", 2048);

    gp.measurement_time(Duration::from_secs(5));

    heavy_fast_rwmut::<true>(&mut gp, "RwLock", 4096);

    heavy_fast_rwmut::<true>(&mut gp, "RwLock", 8192);

    gp.measurement_time(Duration::from_secs(10));

    heavy_fast_rwmut::<true>(&mut gp, "RwLock", 16384);

    gp.finish();
}
