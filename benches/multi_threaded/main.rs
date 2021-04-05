#![feature(asm)]

mod synchronised_bench;
use synchronised_bench::{synchro_bench_input, Config};

mod tick_counter;

use static_init::{Generator, MutLazy,Lazy};

use parking_lot::{RwLock, RawRwLock, lock_api::{RwLockWriteGuard,MappedRwLockWriteGuard,RwLockReadGuard,MappedRwLockReadGuard}};

use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId, BatchSize, PlotConfiguration, AxisScale,BenchmarkGroup,measurement::WallTime};

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
        drop(l);
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

fn do_bench<'a,R,T, F: Fn()->T + Copy, A: Fn(&T)->R + Sync>(gp: &mut BenchmarkGroup<'a,WallTime>,name: &str,init: F, access: A) {

    macro_rules! mb {
        ($t:literal) => { mb!($t - $t) };
        ($t:literal - $l:literal) => {
            synchro_bench_input(
                gp,
                BenchmarkId::new(name, $t),
                &$t,
                |_| init(),
                |l| access(l),
                Config::<true,$t,$l>,
            );
        }
    }

    gp.bench_with_input(
        BenchmarkId::new(name, 1),
        &1,
        |b,_| b.iter_batched(
            init,
            |l| access(&l),
            BatchSize::SmallInput
            )
        );
    mb!(2);
    mb!(4);
    mb!(8);
    mb!(16-8);
    mb!(32-8);

}
            
fn bench_init(c: &mut Criterion) {

    let mut gp = c.benchmark_group("Init Thread Scaling");

    gp.measurement_time(Duration::from_secs(15));

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));


    //do_bench(&mut gp,"LazyMut read",|| MutLazy::new(XX),  |l| *l.read());

    //do_bench(&mut gp,"LazyMut write",|| MutLazy::new(XX),  |l| *l.write());


    //let init = || RwMut(RwLock::new(StaticStatus::Uninitialized),|| 33);

    //do_bench(&mut gp,"LazyMut PkLot read",init,  |l| *l.read());

    //do_bench(&mut gp,"LazyMut PkLot write",init,  |l| *l.write());


    do_bench(&mut gp,"Lazy",|| Lazy::new(XX),  |l| **l);

    let init = || STLazy::<i32>::INIT;

    let access = |l: &STLazy<i32>| {   
                let r: &'static STLazy<i32> = unsafe{&*(l as *const STLazy<i32>)};
                *r.get(|| 33)
            };

    do_bench(&mut gp,"static_lazy::Lazy",init,  access);

    gp.finish();
}

fn bench_access(c: &mut Criterion) {

    let mut gp = c.benchmark_group("Access Thread Scaling");

    gp.measurement_time(Duration::from_secs(15));

    gp.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));


    do_bench(&mut gp,"LazyMut read",|| {let l = MutLazy::new(XX); l.read(); l},  |l| *l.read());

    do_bench(&mut gp,"LazyMut write",|| {let l = MutLazy::new(XX); l.read(); l},  |l| *l.write());


    let init = || { let l = RwMut(RwLock::new(StaticStatus::Uninitialized),|| 33); let _ = l.read(); l};

    do_bench(&mut gp,"LazyMut PkLot read",init,  |l| *l.read());

    do_bench(&mut gp,"LazyMut PkLot write",init,  |l| *l.write());


    do_bench(&mut gp,"Lazy",|| { let l = Lazy::new(XX); let _ = *l; l},  |l| **l);

    let init = || {
        let l = STLazy::<i32>::INIT; 
        let r: &'static STLazy<i32> = unsafe{&*(&l as *const STLazy<i32>)};
        r.get(|| 33); 
        l};

    let access = |l: &STLazy<i32>| {   
                let r: &'static STLazy<i32> = unsafe{&*(l as *const STLazy<i32>)};
                *r.get(|| 33)
            };

    do_bench(&mut gp,"static_lazy::Lazy",init,  access);

    gp.finish();
}

criterion_group! {name=multi; config=Criterion::default();
targets=bench_init
//,bench_access
}

criterion_main! {multi}
