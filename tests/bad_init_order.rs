// Copyright 2021 Olivier Kannengieser 
//
// Licensed under the Apache License, Version 2.0, <LICENSE-APACHE or
// http://apache.org/licenses/LICENSE-2.0> or the MIT license <LICENSE-MIT or
// http://opensource.org/licenses/MIT>, at your option. This file may not be
// copied, modified, or distributed except according to those terms.

#[cfg(any(feature = "debug_core", debug_assertions))]
mod test {
    use static_init::{constructor, dynamic};

    #[dynamic]
    static mut V0: i32 = 12;

    #[dynamic(10)]
    static mut V1: i32 = unsafe { *V0 };

    fn panic_hook(p: &core::panic::PanicInfo<'_>) -> () {
        println!("Panic caught {}", p);
        std::process::exit(0)
    }

    #[constructor(200)]
    unsafe extern "C" fn set_hook() {
        std::panic::set_hook(Box::new(panic_hook));
    }
}

#[test]
fn bad_init_order() {
    assert!(!cfg!(any(feature = "debug_core", debug_assertions)));
}
