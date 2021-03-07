#![feature(prelude_import)]
#![feature(thread_local)]
#[prelude_import]
use std::prelude::v1::*;
#[macro_use]
extern crate std;
extern crate static_init;
use static_init::{constructor, destructor, dynamic};
static mut DEST: i32 = 0;
unsafe extern "C" fn dest_0() {
    {
        match (&DEST, &0) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    DEST += 1;
}
#[doc(hidden)]
#[link_section = ".fini_array.65536"]
#[used]
pub static __static_init_constructor_dest_0: unsafe extern "C" fn() = dest_0;
unsafe extern "C" fn dest_1() {
    {
        match (&DEST, &1) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    DEST += 1;
}
#[doc(hidden)]
#[link_section = ".fini_array.65535"]
#[used]
pub static __static_init_constructor_dest_1: unsafe extern "C" fn() = dest_1;
unsafe extern "C" fn dest_2() {
    {
        match (&DEST, &2) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    DEST += 1;
}
#[doc(hidden)]
#[link_section = ".fini_array.65435"]
#[used]
pub static __static_init_constructor_dest_2: unsafe extern "C" fn() = dest_2;
static mut INI: i32 = 0;
unsafe extern "C" fn init_2() {
    {
        match (&INI, &0) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    INI += 1;
}
#[doc(hidden)]
#[link_section = ".init_array.65335"]
#[used]
pub static __static_init_constructor_init_2: unsafe extern "C" fn() = init_2;
unsafe extern "C" fn init_1() {
    {
        match (&INI, &1) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    INI += 1;
}
#[doc(hidden)]
#[link_section = ".init_array.65535"]
#[used]
pub static __static_init_constructor_init_1: unsafe extern "C" fn() = init_1;
unsafe extern "C" fn init_0() {
    {
        match (&INI, &2) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    };
    INI += 1;
}
#[doc(hidden)]
#[link_section = ".init_array.65536"]
#[used]
pub static __static_init_constructor_init_0: unsafe extern "C" fn() = init_0;
#[cfg(all(unix, target_env = "gnu"))]
mod gnu {
    use super::constructor;
    use std::env::args_os;
    use std::ffi::{CStr, OsStr};
    use std::os::unix::ffi::OsStrExt;
    unsafe extern "C" fn get_args_env(
        argc: i32,
        mut argv: *const *const u8,
        _env: *const *const u8,
    ) {
        let mut argc_counted = 0;
        while !(*argv).is_null() {
            if !args_os()
                .any(|x| x == OsStr::from_bytes(CStr::from_ptr(*argv as *const i8).to_bytes()))
            {
                :: core :: panicking :: panic ("assertion failed: args_os().any(|x|\\n                  x ==\\n                      OsStr::from_bytes(CStr::from_ptr(*argv as\\n                                                           *const i8).to_bytes()))")
            };
            argv = argv.add(1);
            argc_counted += 1
        }
        {
            match (&argc_counted, &argc) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65536"]
    #[used]
    pub static __static_init_constructor_get_args_env: unsafe extern "C" fn(
        i32,
        *const *const u8,
        *const *const u8,
    ) = get_args_env;
}
struct A(i32);
#[automatically_derived]
#[allow(unused_qualifications)]
impl ::core::fmt::Debug for A {
    fn fmt(&self, f: &mut ::core::fmt::Formatter) -> ::core::fmt::Result {
        match *self {
            A(ref __self_0_0) => {
                let debug_trait_builder = &mut ::core::fmt::Formatter::debug_tuple(f, "A");
                let _ = ::core::fmt::DebugTuple::field(debug_trait_builder, &&(*__self_0_0));
                ::core::fmt::DebugTuple::finish(debug_trait_builder)
            }
        }
    }
}
impl ::core::marker::StructuralEq for A {}
#[automatically_derived]
#[allow(unused_qualifications)]
impl ::core::cmp::Eq for A {
    #[inline]
    #[doc(hidden)]
    fn assert_receiver_is_total_eq(&self) -> () {
        {
            let _: ::core::cmp::AssertParamIsEq<i32>;
        }
    }
}
impl ::core::marker::StructuralPartialEq for A {}
#[automatically_derived]
#[allow(unused_qualifications)]
impl ::core::cmp::PartialEq for A {
    #[inline]
    fn eq(&self, other: &A) -> bool {
        match *other {
            A(ref __self_1_0) => match *self {
                A(ref __self_0_0) => (*__self_0_0) == (*__self_1_0),
            },
        }
    }
    #[inline]
    fn ne(&self, other: &A) -> bool {
        match *other {
            A(ref __self_1_0) => match *self {
                A(ref __self_0_0) => (*__self_0_0) != (*__self_1_0),
            },
        }
    }
}
impl A {
    fn new(v: i32) -> A {
        A(v)
    }
}
impl Drop for A {
    fn drop(&mut self) {
        {
            match (&self.0, &33) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        }
    }
}
extern crate test;
#[cfg(test)]
#[rustc_test_marker]
pub const inner_static: test::TestDescAndFn = test::TestDescAndFn {
    desc: test::TestDesc {
        name: test::StaticTestName("inner_static"),
        ignore: false,
        allow_fail: false,
        should_panic: test::ShouldPanic::No,
        test_type: test::TestType::IntegrationTest,
    },
    testfn: test::StaticTestFn(|| test::assert_test_result(inner_static())),
};
fn inner_static() {
    static mut IX: ::static_init::ConstStatic<usize> = {
        unsafe extern "C" fn __static_init_initializer() {
            ::static_init::__set_init_prio(-1i32);
            fn __static_init_do_init() -> usize {
                unsafe { &IX as *const _ as usize }
            }
            ::static_init::ConstStatic::<usize>::set_to(&IX, __static_init_do_init());
            ::static_init::__set_init_prio(i32::MIN);
        }
        #[doc(hidden)]
        #[link_section = ".init_array.65536"]
        #[used]
        pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
            __static_init_initializer;
        ::static_init::ConstStatic::<usize>::uninit(::static_init::StaticInfo {
            variable_name: "IX",
            file_name: "tests/macro.rs",
            line: 93u32,
            column: 24u32,
            init_priority: -1i32,
            drop_priority: -1i32,
        })
    };
    static mut IX2: ::static_init::ConstStatic<usize> = {
        unsafe extern "C" fn __static_init_initializer() {
            ::static_init::__set_init_prio(-1i32);
            fn __static_init_do_init() -> usize {
                unsafe { &IX2 as *const _ as usize }
            }
            ::static_init::ConstStatic::<usize>::set_to(&IX2, __static_init_do_init());
            ::static_init::__set_init_prio(i32::MIN);
        }
        #[doc(hidden)]
        #[link_section = ".init_array.65536"]
        #[used]
        pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
            __static_init_initializer;
        ::static_init::ConstStatic::<usize>::uninit(::static_init::StaticInfo {
            variable_name: "IX2",
            file_name: "tests/macro.rs",
            line: 95u32,
            column: 25u32,
            init_priority: -1i32,
            drop_priority: -1i32,
        })
    };
    static mut I: i32 = 0;
    unsafe extern "C" fn f() {
        I = 3
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65536"]
    #[used]
    pub static __static_init_constructor_f: unsafe extern "C" fn() = f;
    unsafe {
        {
            match (&*IX, &(&IX as *const _ as usize)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&*IX2, &(&IX2 as *const _ as usize)) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&I, &3) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        }
    };
}
static mut V0: ::static_init::Static<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(-1i32);
        fn __static_init_do_init() -> A {
            A::new(unsafe { V1.0 } - 5)
        }
        ::static_init::Static::<A>::set_to(&mut V0, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65536"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::Static::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V0",
        file_name: "tests/macro.rs",
        line: 111u32,
        column: 20u32,
        init_priority: -1i32,
        drop_priority: -1i32,
    })
};
static mut V2: ::static_init::Static<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(20i32);
        fn __static_init_do_init() -> A {
            A::new(12)
        }
        ::static_init::Static::<A>::set_to(&mut V2, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65515"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::Static::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V2",
        file_name: "tests/macro.rs",
        line: 114u32,
        column: 20u32,
        init_priority: 20i32,
        drop_priority: 20i32,
    })
};
static mut V1: ::static_init::ConstStatic<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(10i32);
        fn __static_init_do_init() -> A {
            A::new(unsafe { V2.0 } - 2)
        }
        ::static_init::ConstStatic::<A>::set_to(&V1, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65525"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::ConstStatic::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V1",
        file_name: "tests/macro.rs",
        line: 117u32,
        column: 16u32,
        init_priority: 10i32,
        drop_priority: 10i32,
    })
};
static mut V3: ::static_init::Static<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(20i32);
        fn __static_init_do_init() -> A {
            A::new(12)
        }
        ::static_init::Static::<A>::set_to(&mut V3, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65515"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::Static::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V3",
        file_name: "tests/macro.rs",
        line: 120u32,
        column: 20u32,
        init_priority: 20i32,
        drop_priority: 20i32,
    })
};
static mut V4: ::static_init::ConstStatic<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(10i32);
        fn __static_init_do_init() -> A {
            A::new(unsafe { V2.0 } - 2)
        }
        ::static_init::ConstStatic::<A>::set_to(&V4, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65525"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::ConstStatic::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V4",
        file_name: "tests/macro.rs",
        line: 123u32,
        column: 16u32,
        init_priority: 10i32,
        drop_priority: 10i32,
    })
};
static mut V5: ::static_init::ConstStatic<A> = {
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(5i32);
        fn __static_init_do_init() -> A {
            A::new(unsafe { V4.0 } + 23)
        }
        ::static_init::ConstStatic::<A>::set_to(&V5, __static_init_do_init());
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65530"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    unsafe extern "C" fn __static_init_droper() {
        ::static_init::ConstStatic::<A>::drop(&V5)
    }
    #[doc(hidden)]
    #[link_section = ".fini_array.65530"]
    #[used]
    pub static __static_init_constructor___static_init_droper: unsafe extern "C" fn() =
        __static_init_droper;
    ::static_init::ConstStatic::<A>::uninit(::static_init::StaticInfo {
        variable_name: "V5",
        file_name: "tests/macro.rs",
        line: 126u32,
        column: 16u32,
        init_priority: 5i32,
        drop_priority: 5i32,
    })
};
static mut V6: ::static_init::ConstStatic<A> = {
    unsafe extern "C" fn __static_init_droper() {
        ::static_init::ConstStatic::<A>::drop(&V6)
    }
    #[doc(hidden)]
    #[link_section = ".fini_array.65536"]
    #[used]
    pub static __static_init_constructor___static_init_droper: unsafe extern "C" fn() =
        __static_init_droper;
    ::static_init::ConstStatic::<A>::from(
        A(33),
        ::static_init::StaticInfo {
            variable_name: "V6",
            file_name: "tests/macro.rs",
            line: 129u32,
            column: 129u32,
            init_priority: -1i32,
            drop_priority: -1i32,
        },
    )
};
static mut DROP_V: i32 = 0;
struct C(i32);
impl Drop for C {
    fn drop(&mut self) {
        unsafe {
            {
                match (&self.0, &DROP_V) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                }
            };
            DROP_V += 1;
        };
    }
}
static mut C3: ::static_init::ConstStatic<C> = {
    extern "C" fn __static_init_dropper() {
        unsafe { ::static_init::ConstStatic::<C>::drop(&C3) }
    }
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(-1i32);
        fn __static_init_do_init() -> C {
            C(0)
        }
        ::static_init::ConstStatic::<C>::set_to(&C3, __static_init_do_init());
        ::libc::atexit(__static_init_dropper);
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65536"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::ConstStatic::<C>::uninit(::static_init::StaticInfo {
        variable_name: "C3",
        file_name: "tests/macro.rs",
        line: 145u32,
        column: 16u32,
        init_priority: -1i32,
        drop_priority: -2i32,
    })
};
static mut C2: ::static_init::ConstStatic<C> = {
    extern "C" fn __static_init_dropper() {
        unsafe { ::static_init::ConstStatic::<C>::drop(&C2) }
    }
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(10i32);
        fn __static_init_do_init() -> C {
            C(1)
        }
        ::static_init::ConstStatic::<C>::set_to(&C2, __static_init_do_init());
        ::libc::atexit(__static_init_dropper);
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65525"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::ConstStatic::<C>::uninit(::static_init::StaticInfo {
        variable_name: "C2",
        file_name: "tests/macro.rs",
        line: 148u32,
        column: 16u32,
        init_priority: 10i32,
        drop_priority: -2i32,
    })
};
static mut C1: ::static_init::ConstStatic<C> = {
    extern "C" fn __static_init_dropper() {
        unsafe { ::static_init::ConstStatic::<C>::drop(&C1) }
    }
    unsafe extern "C" fn __static_init_initializer() {
        ::static_init::__set_init_prio(20i32);
        fn __static_init_do_init() -> C {
            C(2)
        }
        ::static_init::ConstStatic::<C>::set_to(&C1, __static_init_do_init());
        ::libc::atexit(__static_init_dropper);
        ::static_init::__set_init_prio(i32::MIN);
    }
    #[doc(hidden)]
    #[link_section = ".init_array.65515"]
    #[used]
    pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
        __static_init_initializer;
    ::static_init::ConstStatic::<C>::uninit(::static_init::StaticInfo {
        variable_name: "C1",
        file_name: "tests/macro.rs",
        line: 151u32,
        column: 16u32,
        init_priority: 20i32,
        drop_priority: -2i32,
    })
};
unsafe extern "C" fn check_drop_v() {
    {
        match (&DROP_V, &3) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    let kind = ::core::panicking::AssertKind::Eq;
                    ::core::panicking::assert_failed(
                        kind,
                        &*left_val,
                        &*right_val,
                        ::core::option::Option::None,
                    );
                }
            }
        }
    }
}
#[doc(hidden)]
#[link_section = ".fini_array.65536"]
#[used]
pub static __static_init_constructor_check_drop_v: unsafe extern "C" fn() = check_drop_v;
extern crate test;
#[cfg(test)]
#[rustc_test_marker]
pub const dynamic_init: test::TestDescAndFn = test::TestDescAndFn {
    desc: test::TestDesc {
        name: test::StaticTestName("dynamic_init"),
        ignore: false,
        allow_fail: false,
        should_panic: test::ShouldPanic::No,
        test_type: test::TestType::IntegrationTest,
    },
    testfn: test::StaticTestFn(|| test::assert_test_result(dynamic_init())),
};
fn dynamic_init() {
    unsafe {
        {
            match (&V0.0, &5) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V1.0, &10) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V2.0, &12) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        V2.0 = 8;
        {
            match (&V2.0, &8) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V4.0, &10) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V3.0, &12) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V5.0, &33) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        {
            match (&V6.0, &33) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
    }
}
#[cfg(feature = "lazy")]
mod lazy {
    use core::sync::atomic::{AtomicI32, Ordering};
    extern crate test;
    #[cfg(test)]
    #[rustc_test_marker]
    pub const thread_local: test::TestDescAndFn = test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("lazy::thread_local"),
            ignore: false,
            allow_fail: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(|| test::assert_test_result(thread_local())),
    };
    #[cfg(feature = "test_thread_local")]
    fn thread_local() {
        #[thread_local]
        static mut TH_LOCAL: ::static_init::Lazy<A> = {
            unsafe extern "C" fn __static_init_initializer() {
                ::static_init::Lazy::<A>::__do_init(&TH_LOCAL);
            }
            #[doc(hidden)]
            #[link_section = ".init_array.65537"]
            #[used]
            pub static __static_init_constructor___static_init_initializer:
                unsafe extern "C" fn() = __static_init_initializer;
            ::static_init::Lazy::<A>::new_with_info(
                || A::new(3),
                ::static_init::StaticInfo {
                    variable_name: "TH_LOCAL",
                    file_name: "tests/macro.rs",
                    line: 183u32,
                    column: 34u32,
                    init_priority: -2,
                    drop_priority: -2,
                },
            )
        };
        unsafe {
            {
                match (&TH_LOCAL.0, &3) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                }
            };
            TH_LOCAL.0 = 42;
            {
                match (&TH_LOCAL.0, &42) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                }
            };
        }
        std::thread::spawn(|| unsafe {
            {
                match (&TH_LOCAL.0, &3) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                }
            };
        })
        .join()
        .unwrap();
        ();
        {
            match (&unsafe { *TH_LOCAL_UNSAFE }, &10) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
        static DROP_COUNT: AtomicI32 = AtomicI32::new(0);
        struct B;
        impl Drop for B {
            fn drop(&mut self) {
                DROP_COUNT.fetch_add(1, Ordering::Relaxed);
            }
        }
        ();
        ();
        std::thread::spawn(|| unsafe {
            &*B1;
            &*B2
        })
        .join()
        .unwrap();
        std::thread::spawn(|| ()).join().unwrap();
        std::thread::spawn(|| unsafe {
            &*B1;
            &*B2
        })
        .join()
        .unwrap();
        {
            match (&DROP_COUNT.load(Ordering::Relaxed), &4) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
    }
    use super::A;
    use static_init::dynamic;
    static L1: ::static_init::GlobalLazy<A> = {
        unsafe extern "C" fn __static_init_initializer() {
            ::static_init::GlobalLazy::<A>::__do_init(&L1);
        }
        #[doc(hidden)]
        #[link_section = ".init_array.65537"]
        #[used]
        pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
            __static_init_initializer;
        ::static_init::GlobalLazy::<A>::new_with_info(
            || A::new(unsafe { L0.0 } + 1),
            ::static_init::StaticInfo {
                variable_name: "L1",
                file_name: "tests/macro.rs",
                line: 226u32,
                column: 20u32,
                init_priority: -2,
                drop_priority: -2,
            },
        )
    };
    static mut L0: ::static_init::GlobalLazy<A> = {
        unsafe extern "C" fn __static_init_initializer() {
            ::static_init::GlobalLazy::<A>::__do_init(&L0);
        }
        #[doc(hidden)]
        #[link_section = ".init_array.65537"]
        #[used]
        pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
            __static_init_initializer;
        ::static_init::GlobalLazy::<A>::new_with_info(
            || A::new(10),
            ::static_init::StaticInfo {
                variable_name: "L0",
                file_name: "tests/macro.rs",
                line: 229u32,
                column: 24u32,
                init_priority: -2,
                drop_priority: -2,
            },
        )
    };
    #[cfg(feature = "lazy_drop")]
    static mut L2: ::static_init::GlobalLazy<A> = {
        extern "C" fn __static_init_dropper() {
            unsafe { ::core::ptr::drop_in_place(::static_init::GlobalLazy::<A>::as_mut_ptr(&L2)) }
        }
        unsafe extern "C" fn __static_init_initializer() {
            ::static_init::GlobalLazy::<A>::__do_init(&L2);
        }
        #[doc(hidden)]
        #[link_section = ".init_array.65537"]
        #[used]
        pub static __static_init_constructor___static_init_initializer: unsafe extern "C" fn() =
            __static_init_initializer;
        ::static_init::GlobalLazy::<A>::new_with_info(
            || {
                let v = (|| A::new(33))();
                unsafe { ::libc::atexit(__static_init_dropper) };
                v
            },
            ::static_init::StaticInfo {
                variable_name: "L2",
                file_name: "tests/macro.rs",
                line: 233u32,
                column: 24u32,
                init_priority: -2,
                drop_priority: -2,
            },
        )
    };
    extern crate test;
    #[cfg(test)]
    #[rustc_test_marker]
    pub const lazy_init: test::TestDescAndFn = test::TestDescAndFn {
        desc: test::TestDesc {
            name: test::StaticTestName("lazy::lazy_init"),
            ignore: false,
            allow_fail: false,
            should_panic: test::ShouldPanic::No,
            test_type: test::TestType::IntegrationTest,
        },
        testfn: test::StaticTestFn(|| test::assert_test_result(lazy_init())),
    };
    fn lazy_init() {
        unsafe {
            {
                match (&L0.0, &10) {
                    (left_val, right_val) => {
                        if !(*left_val == *right_val) {
                            let kind = ::core::panicking::AssertKind::Eq;
                            ::core::panicking::assert_failed(
                                kind,
                                &*left_val,
                                &*right_val,
                                ::core::option::Option::None,
                            );
                        }
                    }
                }
            }
        };
        {
            match (&L1.0, &11) {
                (left_val, right_val) => {
                    if !(*left_val == *right_val) {
                        let kind = ::core::panicking::AssertKind::Eq;
                        ::core::panicking::assert_failed(
                            kind,
                            &*left_val,
                            &*right_val,
                            ::core::option::Option::None,
                        );
                    }
                }
            }
        };
    }
}
#[main]
pub fn main() -> () {
    extern crate test;
    test::test_main_static(&[&thread_local, &lazy_init, &inner_static, &dynamic_init])
}
