extern crate static_init;
use static_init::{constructor,dynamic};


static mut VAL2: i32 = 0;
static mut VAL: i32 = 0;

#[constructor(10)]
fn init_val() {
    unsafe {VAL = 10}
}
#[constructor(20)]
fn init_val2() {
    unsafe {VAL2 = VAL}
}

#[derive(Debug,Eq,PartialEq)]
struct A(i32);

impl A {
    fn new(v:i32) -> A {
        A(v)
    }
}
impl Drop for A {
    fn drop(&mut self) {
        assert_eq!(self.0,33)
    }
}

#[dynamic(10)]
static mut V2: A = A::new(12);

#[dynamic(20)]
static V1: A = A::new(unsafe{(*V2).0}-2);

#[dynamic(init=10)]
static mut V3: A = A::new(12);

#[dynamic(init=20)]
static V4: A = A::new(unsafe{(*V2).0}-2);

#[dynamic(init=30,drop)]
static V5: A = A::new((*V4).0+23);

#[test]
fn constructor (){
    unsafe{assert_eq!(VAL,10)};
    unsafe{assert_eq!(VAL2,10)};
}

#[test]
fn dynamic_init (){
    assert_eq!((*V1).0,10);
    unsafe{assert_eq!((*V2).0,12)};
    unsafe {(*V2).0 = 8};
    unsafe{assert_eq!((*V2).0,8)};
    assert_eq!((*V4).0,10);
    unsafe{assert_eq!((*V3).0,12)};
}
