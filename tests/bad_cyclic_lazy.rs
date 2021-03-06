#[cfg(all(any(feature = "debug_order", debug_assertions),feature="lazy"))]
mod test {
    use static_init::{constructor, dynamic};

    #[dynamic(lazy)]
    static mut V0: i32 = *V0;

    fn panic_hook(p: &core::panic::PanicInfo<'_>) -> () {
        println!("Panic caught {}", p);
        std::process::exit(0)
    }

    #[constructor(10)]
    unsafe extern "C" fn set_hook() {
        std::panic::set_hook(Box::new(panic_hook));
    }
}

#[test]
fn bad_init_order() {
    assert!(!cfg!(all(any(feature = "debug_order", debug_assertions),feature="lazy")));
}
