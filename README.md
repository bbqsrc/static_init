[![LICENSE](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE-MIT)
[![LICENSE](https://img.shields.io/badge/license-apache-blue.svg)](LICENSE-APACHE)
[![Documentation](https://docs.rs/static_init/badge.svg)](https://docs.rs/static_init)
[![Crates.io Version](https://img.shields.io/crates/v/static_init.svg)](https://crates.io/crates/static_init)


Safe non const initialized statics and safe mutable statics with unbeatable performance.

Also provides code execution at program start-up/exit.

# Feature

[x] non const initialized statics.

[x] statics dropped at program exit.

[x] safe mutable lazy statics (locked).

[x] every feature with `no_std` support.

[x] unbeatable performance, can be order of magnitude faster that any other solution.

[x] registration of code execution at program exit without allocation (as opposed to libc::at_exit).

[x] ergonomic syntax.

[x] sound and safe.

[x] on nigtly, `thread_locals` and safe mutable `thread_locals`, guaranteed to be
    dropped at thread exit with the lowest possible overhead compared to
    what is provided by system library thread support or the standard library!

# Fastest Lazy Statics

This crate provides *lazy statics* on all plateforms.

On unixes and windows *lesser lazy statics* are *lazy* during program startup phase
(before `main` is called). Once main is called, those statics are all guaranteed to be
initialized and any access to them almost no incur any performance cost

```rust
use static_init::{dynamic};

#[dynamic] 
static L1: Vec<i32> = vec![1,2,3,4,5,6];

#[dynamic(drop)] 
static L2: Vec<i32> = {let v = L1.clone(); v.push(43); v};
```

Those static initialization and access can be 10x faster than
what is provided by the standard library or other crates.

# Safe Mutable Statics

Just add the `mut` keyword to have mutable locked statics.

```rust
use static_init::{dynamic};

#[dynamic] 
static mut L1: Vec<i32> = vec![1,2,3,4,5,6];

#[dynamic(drop)] 
static mut L2: Vec<i32> = {
	//get a unique lock:
	let lock = L1.write(); 
	lock.push(42); 
	lock.clone()
	};
```

Those statics use an *apdaptative phase locker* that gives them surprising performance.

# Classical Lazy statics 

By default, initialization of statics declared with the `dynamic` is forced before main
start on plateform that support it. If *lazyness* if a required feature, the attribute argument
`lazy` can be used.

```rust
use static_init::{dynamic};

#[dynamic(lazy)] 
static L1: Vec<i32> = vec![1,2,3,4,5,6];

#[dynamic(lazy,drop)] 
static mut L3: Vec<i32> =L1.clone(); 
```

Even if the static is not mut, dropped statics are always locked. There is also a `finalize` attribute
argument that can be used to run a "drop" equivalent at program exit but leaves the static unchanged. 

Those lazy also provide superior performances compared to other solutions.

# `no_std` support

On linux or Reddox (TBC) this library is `no_std`. The library use directly the `futex` system call
to place thread in a wait queue when needed.

On other plateform `no_std` support can be gain by using the `spin_loop` feature. NB that lock strategies
based on spin loop are not system-fair and cause entire system slow-down.

# Performant

## Under the hood

The statics and mutable statics declared with `dynamic` attribute use what we
call an  *adaptative phase locker*. This is a lock that is in between a `Once`
and a `RwLock`. It is carefully implemented as a variation over the `RwLock`
algorithms of `parking_lot` crate with other tradeoff and different
capabilities. 

It is qualified *adaptative* because the decision to take a read lock,
a write lock or not to take a lock is performed while the lock attempt is
performed and a thread may attempt to get a write lock but decides to be waked
as the owner of a read lock if it is about to be placed in a wait queue.

Statics and thread locals that need to register themselve for destruction at
program or thread exit are implemented as members of an intrusive list. This
implementation avoid heap memory allocation caused by system library support
(`libc::at_exit`, `glibc::__cxa_at_thread_exit`, pthread... registers use heap
memory allocation), and it avoid to fall on system library implementation
limits that may cause `thread_locals` declared with `std::thread_locals` not to
be dropped. 

Last but not least of the optimization, on windows and unixes (but not Mac yet)
`dynamic` statics initialization is forced before main start. This fact unable
a double check with a single boolean for all statics that is much faster other
double check solution. 

## Benchmark results

# Thread local support

On nightly `thread_local` support can be enable with the feature
`thread_local`. The attribute `dynamic` can be used with thread locals as with
regular statics. In this case, the mutable `thread_local` will behave similarly
to a RefCell with the same syntax as mutable lazy statics.

```rust
#[dynamic(drop)] //guaranteed to be drop: no leak contrarily to std::thread_local
#[thread_local]
static V: Vec<i32> = vec![1,1,2,3,5];

#[dynamic]
#[thread_local]
static mut W: Vec<i32> = V.clone();

assert_ne!(W.read().len(), 0);
assert_ne!(W.try_read().unwrap().len(), 0);
```

# Unsafe Low level 

## Unchecked statics initiliazed at program start up

The library also provides unchecked statics, whose initialization is run before main start. Those statics
does not imply any memory overhead neither execution time overhead. This is the responsability of the coder
to be sure not to access those static before they are initialized.

```rust
#[dynamic(10)]
static A: Vec<i32> = vec![1,2,3];

#[dynamic(0,drop)]
static mut B: Vec<i32> = unsafe {A.clone()};
```

Even if A is not declared mutable, the attribute macro convert it into a mutable static to ensure that every
access to it is unsafe.

The number indicates the priority, the larger the number, the sooner the static will be initialized.

Those statics can also be droped at program exit with the `drop` attribute argument.

## Program constructor destructor 

It is possible to register fonction for execution before main start/ after main returns.

```rust
#[constructor(10)]
extern "C" fn run_first() {}

#[constructor(0)]
extern "C" fn then_run() {}

#[destructor(0)]
extern "C" fn pre_finish() {}

#[destructor(10)]
extern "C" fn finaly() {}
```

