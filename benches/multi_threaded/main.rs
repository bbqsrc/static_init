#![feature(asm)]

mod synchronised_bench;
use synchronised_bench::{synchro_bench, synchro_bench_input, Config};

mod tick_counter;

use static_init::{Generator, MutLazy,Lazy};

use parking_lot::{RwLock, RawRwLock, lock_api::{RwLockWriteGuard,MappedRwLockWriteGuard,RwLockReadGuard,MappedRwLockReadGuard}};
use std::sync::atomic::{AtomicUsize, Ordering};

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, BatchSize, PlotConfiguration, AxisScale};

use lazy_static::lazy::Lazy as STLazy;

use std::time::Duration;

struct XX;
impl Generator<i32> for XX {
    #[inline(always)]
    fn generate(&self) -> i32 {
        10
    }
}

enum StaticStatus<T> {
    Uninitialized,
    Poisoned,
    Value(T),
}
struct Guard<'a,T>(&'a mut StaticStatus<T>);

impl<'a,T> Drop for Guard<'a,T> {
    fn drop(&mut self) {
        *self.0 = StaticStatus::Poisoned;
    }
}

struct RwMut<T,F> (RwLock<StaticStatus<T>>,F);

impl<T,F> RwMut<T,F> 
    where F: Fn() -> T
    {
    fn write(&self) -> MappedRwLockWriteGuard<'_,RawRwLock,T> {
        let mut l = self.0.write();
        if let StaticStatus::Uninitialized = &mut * l {
            let g = Guard(&mut *l);
            *g.0 = StaticStatus::Value((self.1)());
            std::mem::forget(g);
        }
        RwLockWriteGuard::map(l,|v| 
        match v {
            StaticStatus::Value(v) => v,
            
            StaticStatus::Poisoned => {
                panic!("Poisoned accceess");
            }
            StaticStatus::Uninitialized => unreachable!(),
        }
        )
    }
    fn read(&self) -> MappedRwLockReadGuard<'_,RawRwLock,T> {
        let mut l = self.0.write();
        if let StaticStatus::Uninitialized = &mut * l {
            let g = Guard(&mut *l);
            *g.0 = StaticStatus::Value((self.1)());
            std::mem::forget(g);
        }
        let l = self.0.read();
        RwLockReadGuard::map(l,|v| 
        match v {
            StaticStatus::Value(v) => v,
            
            StaticStatus::Poisoned => {
                panic!("Poisoned accceess");
            }
            StaticStatus::Uninitialized => unreachable!(),
        }
        )
    }
    }

            
fn bench_init_mut_lazy(c: &mut Criterion) {
    let mut gp = c.benchmark_group("Init Thread Scaling");
    gp.measurement_time(Duration::from_secs(15));
    //BUG
    //gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    gp.bench_with_input(
        BenchmarkId::new("LazyMut read", 1),
        &1,
        |b,_| b.iter_batched(
            || MutLazy::new(XX),
            |l: MutLazy<_,_>| *l.read(),
            BatchSize::SmallInput
            )
        );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut read", 2),
        &2,
        |_| MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, 2,2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut read", 4),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, 4,4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut read", 8),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, 8,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut read", 16),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, 16,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut read", 32),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.read(),
        Config::<true, 32,8>,
    );

    gp.bench_with_input(
        BenchmarkId::new("LazyMut write", 1),
        &1,
        |b,_| b.iter_batched(
            || MutLazy::new(XX),
            |l: MutLazy<_,_>| *l.write(),
            BatchSize::SmallInput
            )
        );

    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut write", 2),
        &2,
        |_| MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, 2,2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut write", 4),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, 4,4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut write", 8),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, 8,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut write", 16),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, 16,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut write", 32),
        &8,
        |_| MutLazy::new(XX),
        |l| *l.write(),
        Config::<true, 32,8>,
    );
    
    let init = || RwMut(RwLock::new(StaticStatus::Uninitialized),|| 33);
    gp.bench_with_input(
        BenchmarkId::new("LazyMut PkLot read", 1),
        &1,
        |b,_| b.iter_batched(
            init,
            |l| *l.read(),
            BatchSize::SmallInput
            )
        );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot read", 2),
        &2,
        |_| init(),
        |l| *l.read(),
        Config::<true, 2, 2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot read", 4),
        &8,
        |_| init(),
        |l| *l.read(),
        Config::<true, 4, 4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot read", 8),
        &8,
        |_| init(),
        |l| *l.read(),
        Config::<true, 8, 8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot read", 16),
        &8,
        |_| init(),
        |l| *l.read(),
        Config::<true, 16, 8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot read", 32),
        &8,
        |_| init(),
        |l| *l.read(),
        Config::<true, 32, 8>,
    );
    
    gp.bench_with_input(
        BenchmarkId::new("LazyMut PkLot read", 1),
        &1,
        |b,_| b.iter_batched(
            init,
            |l| *l.read(),
            BatchSize::SmallInput
            )
        );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot write", 2),
        &2,
        |_| init(),
        |l| *l.write(),
        Config::<true, 2, 2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot write", 4),
        &8,
        |_| init(),
        |l| *l.write(),
        Config::<true, 4, 4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot write", 8),
        &8,
        |_| init(),
        |l| *l.write(),
        Config::<true, 8, 8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot write", 16),
        &8,
        |_| init(),
        |l| *l.write(),
        Config::<true, 16, 8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyMut PkLot write", 32),
        &8,
        |_| init(),
        |l| *l.write(),
        Config::<true, 32, 8>,
    );

    gp.bench_with_input(
        BenchmarkId::new("Lazy", 1),
        &1,
        |b,_| b.iter_batched(
            || Lazy::new(XX),
            |l: Lazy<_,_>| *l,
            BatchSize::SmallInput
            )
        );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("Lazy", 2),
        &2,
        |_| Lazy::new(XX),
        |l| **l,
        Config::<true, 2,2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("Lazy", 4),
        &8,
        |_| Lazy::new(XX),
        |l| **l,
        Config::<true, 4,4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("Lazy", 8),
        &8,
        |_| Lazy::new(XX),
        |l| **l,
        Config::<true, 8,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("Lazy", 16),
        &8,
        |_| Lazy::new(XX),
        |l| **l,
        Config::<true, 16,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("Lazy", 32),
        &8,
        |_| Lazy::new(XX),
        |l| **l,
        Config::<true, 32,8>,
    );

    let build = || STLazy::<i32>::INIT;
    let accessv = |l: STLazy<i32>| {   
                let r: &'static STLazy<i32> = unsafe{&*(&l as *const STLazy<i32>)};
                *r.get(|| 33)
            };
    let access = |l: &STLazy<i32>| {   
                let r: &'static STLazy<i32> = unsafe{&*(l as *const STLazy<i32>)};
                *r.get(|| 33)
            };
    gp.bench_with_input(
        BenchmarkId::new("LazyStatic", 1),
        &1,
        |b,_| b.iter_batched(
            build,
            accessv,
            BatchSize::SmallInput
            )
        );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyStatic", 2),
        &2,
        |_| build(),
        access,
        Config::<true, 2,2>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyStatic", 4),
        &8,
        |_| build(),
        access,
        Config::<true, 4,4>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyStatic", 8),
        &8,
        |_| build(),
        access,
        Config::<true, 8,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyStatic", 16),
        &8,
        |_| build(),
        access,
        Config::<true, 16,8>,
    );
    synchro_bench_input(
        &mut gp,
        BenchmarkId::new("LazyStatic", 32),
        &8,
        |_| build(),
        access,
        Config::<true, 32,8>,
    );

    gp.finish();
}

criterion_group! {name=multi; config=Criterion::default();
targets=bench_init_mut_lazy,
}

criterion_main! {multi}
