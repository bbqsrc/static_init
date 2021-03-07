extern crate proc_macro;
extern crate syn;
use syn::spanned::Spanned;
use syn::*;

extern crate quote;
use quote::{quote, quote_spanned};

use proc_macro::TokenStream;

extern crate proc_macro2;
use proc_macro2::{Span, TokenStream as TokenStream2};

//TODO: on windows sectionls are classified in alphabetical order by the linker
//then the c runtime will run every thing between CRT$XCA and CRT$XCZ so
//it should be possible to define priorty using this

/// Attribute for functions run at program initialization (before main).
///
/// ```ignore
/// #[constructor]
/// unsafe extern "C" fn initer () {
/// // run before main start
/// }
/// ```
/// The execution order of constructors is unspecified. Nevertheless on ELF plateform (linux, any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `constructor(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1].
///
/// Constructors with a priority of 65535 are run first (in unspecified order), then constructors
/// with priority 65534 are run ...  then constructors
/// with priority number 0 and finaly constructors with no priority.
///
/// # Safety
///
/// Constructor functions must be unsafe. Any access to [macro@dynamic] statics with an equal or lower
/// initialization priority will cause undefined behavior. (NB: usual static data initialized
/// by a const expression are always in an initialized state so it is always safe to read them).
///
/// Notably, on Elf gnu variant platforms, accesses to the program argument or environment through `std::env::*` functionalities
/// with a priority 65535-100 will cause undefined behavior. On windows thoses accesses `std::env::*` will never cause
/// undefined behavior. On other plateforms (non gnu variant of unixes and mac), any access to
/// `std::env::*` in a constructor, whatever its priority, will cause undefined behavior. In this
/// last case, the information may be accessible in the /proc/self directory.
///
/// ```ignore
/// #[constructor(0)]
/// unsafe extern "C" fn first () {
/// // run before main start
/// }
///
/// #[constructor(1)]
/// unsafe extern "C" fn then () {
/// // run before main start
/// }
/// ```
///
/// NB: Whatever the priority, constructors are run after initialization of libc resources. C++ static
/// objects are initialized as constructors with no priorities. On ELF plateform, libstdc++
/// resources are initialized with priority 65535-100.
///
/// # Constructor signature
///
/// Constructor function should have type `unsafe extern "C" fn() -> ()`.
///
/// But on plateform where the program is linked
/// with the gnu variant of libc (which covers all gnu variant platforms) constructor functions
/// can take (or not) `argc: i32, argv: **const u8, env: **const u8` arguments.
/// `argc` is the size of the argv
/// sequence, `argv` and `env` both refer to null terminated contiguous sequence of pointer
/// to c-string (c-strings are null terminated sequence of u8).
/// Cf "glibc source"/csu/elf-init.c, and System V ABI.
#[proc_macro_attribute]
pub fn constructor(args: TokenStream, input: TokenStream) -> TokenStream {
    let func: ItemFn = parse_macro_input!(input);

    let priority = match parse_priority(args) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };

    let section = if cfg!(target_os = "linux")
        || cfg!(target_os = "android")
        || cfg!(target_os = "freebsd")
        || cfg!(target_os = "dragonfly")
        || cfg!(target_os = "netbsd")
        || cfg!(target_os = "openbsd")
        || cfg!(target_os = "solaris")
        || cfg!(target_os = "illumos")
        || cfg!(target_os = "emscripten")
        || cfg!(target_os = "haiku")
        || cfg!(target_os = "l4re")
        || cfg!(target_os = "fuchsia")
        || cfg!(target_os = "redox")
        || cfg!(target_os = "vxworks")
    {
        if let Some(p) = priority {
            format!(".init_array.{:05}", p)
        } else {
            ".init_array.65536".to_string()
            //.init_array.65537 and .init_array.65538 used for lazy
        }
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        if priority.is_some() {
            return quote!(compile_error!(
                "Constructor priority not supported on this plateform."
            ))
            .into();
        }
        "__DATA,__mod_init_func".to_string()
    } else if cfg!(target_os = "windows") {
        if let Some(p) = priority {
            format!(".CRT$XCTZ{:05}", p)
        } else {
            ".CRT$XCTZ65536".to_string()
        }
    } else {
        return quote!(compile_error!("Target not supported")).into();
    };

    let mod_name = format!("__static_init_constructor_{}", func.sig.ident);
    let sp = func.sig.span();
    let typ = if cfg!(target_env = "gnu")
        && cfg!(target_family = "unix")
        && !func.sig.inputs.is_empty()
    {
        quote_spanned!(sp.span()=>unsafe extern "C" fn(i32,*const*const u8, *const *const u8))
    } else {
        quote_spanned!(sp.span()=>unsafe extern "C" fn())
    };
    gen_ctor_dtor(func, &section, &mod_name, &parse2(typ).unwrap()).into()
}

/// Attribute for functions run at program termination (after main)
///
/// ```ignore
/// #[destructor]
/// unsafe extern "C" fn droper () {
/// // run after main return
/// }
/// ```
///
/// The execution order of destructors is unspecified. Nevertheless on ELF plateform (linux,any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `destructor(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1].
///
/// Destructors without priority are run first (in unspecified order), then destructors with priority 0 are run,
/// then destructors with priority number 1,... finaly destructors with priority 65535 are run.
///
/// # Safety
///
/// Destructor functions must be unsafe. Any access to statics dropped with an equal or lower
/// priority will cause undefined behavior.
///
/// ```ignore
/// #[destructor(1)]
/// unsafe extern "C" fn first () {
/// // run after main return
/// }
///
/// #[destructor(0)]
/// unsafe extern "C" fn then () {
/// // run after main return
/// }
/// ```
///
/// # Destructor signature
///
/// Destructor function should have type `unsafe extern "C" fn() -> ()`.
#[proc_macro_attribute]
pub fn destructor(args: TokenStream, input: TokenStream) -> TokenStream {
    let func: ItemFn = parse_macro_input!(input);

    let priority = match parse_priority(args) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let section = if cfg!(target_os = "linux")
        || cfg!(target_os = "android")
        || cfg!(target_os = "freebsd")
        || cfg!(target_os = "dragonfly")
        || cfg!(target_os = "netbsd")
        || cfg!(target_os = "openbsd")
        || cfg!(target_os = "solaris")
        || cfg!(target_os = "illumos")
        || cfg!(target_os = "emscripten")
        || cfg!(target_os = "haiku")
        || cfg!(target_os = "l4re")
        || cfg!(target_os = "fuchsia")
        || cfg!(target_os = "redox")
        || cfg!(target_os = "vxworks")
    {
        if let Some(p) = priority {
            format!(".fini_array.{:05}", p)
        } else {
            ".fini_array.65536".to_string()
            //.fini_array.65537 and .fini_array used for lazy
        }
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        if priority.is_some() {
            return quote!(compile_error!(
                "Constructor priority not supported on this plateform."
            ))
            .into();
        }
        "__DATA,__mod_term_func".to_string()
    } else if cfg!(target_os = "windows") {
        if let Some(p) = priority {
            format!(".CRT$XPTZ{:05}", p)
        } else {
            ".CRT$XPU".to_string()
            //".CRT$XPVB" and ".CRT$XPVA" used for lazy
        }
    } else {
        return quote!(compile_error!("Target not supported")).into();
    };

    let mod_name = format!("__static_init_constructor_{}", func.sig.ident);
    let sp = func.sig.span();
    let typ = quote_spanned!(sp.span()=>unsafe extern "C" fn());
    gen_ctor_dtor(func, &section, &mod_name, &parse2(typ).unwrap()).into()
}

/// Statics initialized with non const functions.
///
/// Statics on which this attribute is applied will be be initialized at run time (optionaly see
/// bellow), before main start. This allow statics initialization with non const expressions.
///
/// There are two variants of dynamic statics:
///
/// # Dynamic lazy statics
///
/// These dynamics are supported on all plateforms and requires std support. Lazy dynamics are
/// enabled by the default feature "lazy".
///
/// Lazy statics are declared with the `[dynamic(lazy)]` attribute. On unixes and windows plateforms these
/// statics are optimized versions of [std::lazy::SyncLazy]. After program initialization phase,
/// those statics are guaranteed to be initialized and access to them will be as fast as any access
/// to a regular static. On other plateforms, those statics are equivalent to [std::lazy::SyncLazy].
///
/// ```ignore
/// struct A(i32);
///
/// impl A {
///   //new is not const
///   fn new(v:i32) -> A {
///     A(v)
///   }
/// }
///
/// #[dynamic(lazy)]
/// static V :A = A::new(42);
/// ```
///
/// Optionnaly, if the default feature "lazy_drop" is enabled, lazy dynamic statics declared with
/// `[dynamic(lazy,drop)]` will be dropped at program exit. Dropped lazy dynamic statics ared
/// dropped in the reverse order of their initialization. This feature is implemented thanks to
/// `libc::atexit`.
///
/// ```ignore
/// struct A(i32);
///
/// impl A {
///   //new is not const
///   fn new(v:i32) -> A {
///     A(v)
///   }
/// }
///
/// impl Drop for A {
///     fn drop(&mut A){
///         println!("{}",A.0)
///     }
/// }
///
/// // V2 is initialized before V1,
/// // so V1 will be dropped before V2
/// // The program will print:
/// // 33
/// // 42
/// #[dynamic(lazy,drop)]
/// static V1 :A = A::new(V2.0 - 9);
///
/// #[dynamic(lazy,drop)]
/// static V2 :A = A::new(42);
/// ```
///
/// # Dynamic statics
///
/// Those statics will be initialized at program startup, without ordering, accept between those
/// that have different priorities on plateform that support priorities. Those statics are
/// supported on unixes and windows with priorities and mac without priorities.
///
/// ## Safety
///
/// Initialization expressions must be unsafe blocks. During initialization, any access to other
/// "dynamic" statics initialized with a lower priority will cause undefined behavior. Similarly,
/// during drop any access to a "dynamic" static dropped with a lower priority will cause undefined
/// behavior.
///
/// ```ignore
/// struct A(i32);
///
/// impl A {
///   //new is not const
///   fn new(v:i32) -> A {
///     A(v)
///   }
/// }
///
/// #[dynamic]
/// static V :A = unsafe{A::new(42)};
/// ```
///
/// ## Execution Order
///
/// The execution order of "dynamic" static initializations is unspecified. Nevertheless on ELF plateform (linux,any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `dynamic(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1].
///
/// Statics with priority number 65535 are initialized first (in unspecified order), then statics
/// with priority number 65534 are initialized ...  then statics
/// with priority number 0 and finaly statics without priority.
///
/// ```ignore
/// struct A(i32);
///
/// impl A {
///   //new is not const
///   fn new(v:i32) -> A {
///     A(v)
///   }
/// }
///
/// //V1 must be initialized first
/// //because V2 uses the value of V1.
/// #[dynamic(10)]
/// static mut V1 :A = unsafe{A::new(33)};
///
/// #[dynamic(20)]
/// static V2 :A = unsafe{A::new(V1.0 + 9)};
/// ```
///
/// # Full syntax and dropped statics
///
/// Finaly the full syntax is for the attribute is:
///
/// ```text
/// "dynamic" [ "(" <dyn_opts> ")" ]
///
/// dyn_opts:
///   <dyn_opt>
///   <dyn_opt>, <dyn_opts>
///
/// dyn_opt:
///   "init" [ "=" <priority> ]
///   "drop" [ "=" <priority> ]
/// ```  
///
/// The macro attribute `dynamic` is equivalent to `dynamic(init=0)`
/// and `dynamic(<num>)` to `dynamic(init=<num>)`. In the absence of `init`
/// the static will be const initialized as usual static. The `drop` option
/// cause the static to be droped after main returns. The priority has the
/// same semantic as for the [macro@destructor] attribute: statics without priority
/// are droped first, then statics with priority 0,... and finaly statics with priority
/// 65535 are the last dropped.
///
/// If the drop priority is not explicitly specified, it will equal that of the initializaton
/// priority.
///
/// ```ignore
/// struct A(i32);
///
/// impl A {
///   //new is not const
///   fn new(v:i32) -> A {
///     A(v)
///   }
///   //new is not const
///   const fn const_new(v:i32) -> A {
///     A(v)
///   }
/// }
///
/// impl Drop for A {
///     fn drop(&mut self) {}
///     }
///
/// //const initialized droped after main exit
/// #[dynamic(drop)]
/// static mut V1 :A = unsafe{A::new_const(33)};
///
/// //initialized before V1 and droped after V1
/// #[dynamic(20,drop=10)]
/// static V2 :A = unsafe{A::new(10)};
///
/// // if a drop priority is not specified, it equals the
/// // init priority so the attribute bellow is equivalent to
/// // #[dynamic(init=20, drop=20)
/// #[dynamic(init=20,drop)]
/// static V3 :A = unsafe{A::new(10)};
///
/// // not droped
/// #[dynamic(init)]
/// static V4 :A = unsafe{A::new(10)};
///
/// // not droped
/// #[dynamic]
/// static V5 :A = unsafe{A::new(10)};
///
/// // not droped
/// #[dynamic(10)]
/// static V6 :A = unsafe{A::new(10)};
/// ```
///
/// # Actual type of "dynamic" statics
///
/// A mutable "dynamic" static declared to have type `T`, will have type `static_init::Static<T>`.
///
/// A mutable "dynamic" static declared to have type `T`, will have type `static_init::ConstStatic<T>`.
///
/// Those types are opaque types that implements `Deref<T>`. `static_init::Static` also implements
/// `DerefMut`.
///
/// ```no_run
///
/// // V has type static_init::ConstStatic<i32>
/// #[dynamic]
/// static V :i32 = unsafe{0};
///
/// // W has type static_init::Static<i32>
/// #[dynamic]
/// static W :i32 = unsafe{0};
/// ```

#[proc_macro_attribute]
pub fn dynamic(args: TokenStream, input: TokenStream) -> TokenStream {
    let item: ItemStatic = parse_macro_input!(input);

    let options = match parse_dyn_options(parse_macro_input!(args)) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };

    gen_dyn_init(item, options).into()
}

#[derive(Eq, PartialEq)]
enum Initialization {
    Dynamic,
    Lazy,
    None,
}

struct DynOptions {
    init:          Initialization,
    init_priority: Option<u32>,
    drop:          bool,
    drop_priority: Option<u32>,
}

fn parse_priority(args: TokenStream) -> std::result::Result<Option<u32>, TokenStream2> {
    if !args.is_empty() {
        if let Ok(n) = syn::parse(args.clone()).map_err(|e| e.to_compile_error()) {
            let n: Ident = n;
            if n == "__lazy_init" {
                return Ok(Some(65537));
            } else if n == "__lazy_init_finished" {
                return Ok(Some(65538));
            }
        }
        let n: LitInt = syn::parse(args).map_err(|e| e.to_compile_error())?;
        Ok(Some(
            n.base10_parse::<u16>()
                .map(|v| 65535 - (v as u32))
                .map_err(|e| e.to_compile_error())?,
        ))
    } else {
        Ok(None)
    }
}

fn parse_dyn_options(args: AttributeArgs) -> std::result::Result<DynOptions, TokenStream2> {
    if !args.is_empty() {
        let mut opt = DynOptions {
            init:          Initialization::None,
            init_priority: None,
            drop:          false,
            drop_priority: None,
        };
        for arg in args {
            match arg {
                NestedMeta::Meta(Meta::Path(id)) => {
                    let id = if let Some(id) = id.get_ident() {
                        id
                    } else {
                        return Err(
                            quote_spanned!(id.span()=>compile_error!(concat!("Unexpected attribute argument \"",stringify!(#id),"\""))),
                        );
                    };
                    if id == "init" {
                        opt.init = Initialization::Dynamic;
                    } else if id == "drop" {
                        opt.drop = true;
                        if opt.init == Initialization::Lazy && !cfg!(feature = "lazy_drop") {
                            return Err(
                                quote_spanned!(id.span()=>compile_error!("feature lazy_drop must be enabled")),
                            );
                        }
                    } else if id == "lazy" {
                        if cfg!(feature = "lazy") {
                            opt.init = Initialization::Lazy;
                            if opt.drop && !cfg!(feature = "lazy_drop") {
                                return Err(
                                    quote_spanned!(id.span()=>compile_error!("feature lazy_drop must be enabled")),
                                );
                            }
                        } else {
                            return Err(
                                quote_spanned!(id.span()=>compile_error!("feature lazy must be enabled")),
                            );
                        }
                    } else {
                        return Err(
                            quote_spanned!(id.span()=>compile_error!(concat!("Unknown attribute argument \"",stringify!(#id),"\""))),
                        );
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) => {
                    let id = if let Some(id) = nv.path.get_ident() {
                        id
                    } else {
                        return Err(
                            quote_spanned!(nv.path.span()=>compile_error!(concat!("Unknown attribute argument \"",stringify!(#nv),"\""))),
                        );
                    };
                    if id == "init" {
                        opt.init = Initialization::Dynamic;
                        if let Lit::Int(n) = &nv.lit {
                            let prio = n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?;
                            opt.init_priority = Some(prio as u32);
                        } else {
                            return Err(
                                quote_spanned!(nv.lit.span()=>compile_error!("Expected an init priority (u16)")),
                            );
                        }
                    } else if id == "drop" {
                        opt.drop = true;
                        if let Lit::Int(n) = &nv.lit {
                            let prio = n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?;
                            opt.drop_priority = Some(prio as u32);
                        } else {
                            return Err(
                                quote_spanned!(nv.lit.span()=>compile_error!("Expected a drop priority (u16)")),
                            );
                        }
                    } else {
                        return Err(
                            quote_spanned!(id.span()=>compile_error!("Expected eithe 'init' or 'drop'")),
                        );
                    }
                }
                NestedMeta::Lit(Lit::Int(n)) => {
                    opt.init = Initialization::Dynamic;
                    opt.init_priority =
                        Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())? as u32);
                }
                _ => {
                    return Err(
                        quote_spanned!(arg.span()=>compile_error!("Expected either 'init' or 'drop'")),
                    )
                }
            }
        }

        Ok(opt)
    } else {
        Ok(DynOptions {
            init:          Initialization::Dynamic,
            init_priority: None,
            drop:          false,
            drop_priority: None,
        })
    }
}

fn gen_ctor_dtor(func: ItemFn, section: &str, mod_name: &str, typ: &TypeBareFn) -> TokenStream2 {
    let mod_name = Ident::new(mod_name, Span::call_site());
    let section = LitStr::new(section, Span::call_site());
    let func_name = &func.sig.ident;

    let sp = func.sig.span();
    if func.sig.unsafety.is_none() {
        quote_spanned! {sp=>compile_error!("Constructors and destructors must be unsafe functions as \
        they may access uninitialized memory regions")}
    } else {
        quote_spanned! {sp=>
            #func
            #[doc(hidden)]
            #[link_section = #section]
            #[used]
            pub static #mod_name: #typ = #func_name;
        }
    }
}

fn gen_dyn_init(mut stat: ItemStatic, mut options: DynOptions) -> TokenStream2 {
    let stat_name = &stat.ident;
    let expr = &*stat.expr;
    let stat_typ = &*stat.ty;

    if !(options.init == Initialization::Lazy) && !matches!(*stat.expr, syn::Expr::Unsafe(_)) {
        let sp = stat.expr.span();
        if options.init == Initialization::Dynamic {
            return quote_spanned!(sp=>compile_error!("Initializer expression must be an unsafe block \
            because this expression may access uninitialized data"));
        } else {
            return quote_spanned!(sp=>compile_error!("Although the initialization of this \"dynamic\" static is safe \
            an unsafe block is required for this initialization as a reminder that the drop phase may lead to undefined behavior"));
        }
    }

    //fix drop priority, if not specified, drop priority equal
    //that of initialization priority
    //
    if let Some(p) = options.init_priority {
        if options.drop_priority.is_none() {
            options.drop_priority = Some(p);
        }
    }

    let (typ, stat_ref): (Type, Expr) = if options.init != Initialization::Lazy {
        if stat.mutability.is_some() {
            (
                parse_quote! {
                    ::static_init::Static::<#stat_typ>
                },
                parse_quote! {
                    &mut #stat_name
                },
            )
        } else {
            (
                parse_quote! {
                    ::static_init::ConstStatic::<#stat_typ>
                },
                parse_quote! {
                    &#stat_name
                },
            )
        }
    } else {
        (
            parse_quote! {
                ::static_init::Lazy::<#stat_typ>
            },
            parse_quote! {
                &#stat_name
            },
        )
    };

    let sp = stat.expr.span();

    let init_priority = options.init_priority.map_or(-1, |p| p as i32);
    let drop_priority = options.drop_priority.map_or(-1, |p| p as i32);

    let initer = match options.init {
        Initialization::Dynamic => {
            let attr: Attribute = if let Some(priority) = options.init_priority {
                parse_quote!(#[::static_init::constructor(#priority)])
            } else {
                parse_quote!(#[::static_init::constructor])
            };
            Some(quote_spanned! {sp=>
                    #attr
                    unsafe extern "C" fn __static_init_initializer() {
                        ::static_init::__set_init_prio(#init_priority);
                        #typ::set_to(#stat_ref,#expr);
                        ::static_init::__set_init_prio(i32::MIN);
                    }
            })
        }
        Initialization::Lazy => Some(quote_spanned! {sp=>
                #[::static_init::constructor(__lazy_init)]
                unsafe extern "C" fn __static_init_initializer() {
                    ::static_init::Lazy::do_init(#stat_ref);
                }
        }),
        Initialization::None => None,
    };

    let droper = if options.drop && options.init != Initialization::Lazy {
        let attr: Attribute = if let Some(priority) = options.drop_priority {
            parse_quote!(#[::static_init::destructor(#priority)])
        } else {
            parse_quote!(#[::static_init::destructor])
        };
        Some(quote_spanned! {sp=>
                #attr
                unsafe extern "C" fn __static_init_droper() {
                    #typ::drop(#stat_ref)
                }
        })
    } else {
        None
    };

    let statid = &stat.ident;

    match options.init {
        Initialization::Dynamic => {
            let q = quote_spanned! {sp=>{
                #initer
                #droper
                #typ::uninit(
                ::static_init::StaticInfo{
                    variable_name: ::core::stringify!(#statid),
                    file_name: ::core::file!(),
                    line: ::core::line!(),
                    column: ::core::column!(),
                    init_priority: #init_priority,
                    drop_priority: #drop_priority
                    })
            }
            }
            .into();
            *stat.expr = match parse(q) {
                Ok(exp) => exp,
                Err(e) => return e.to_compile_error(),
            }
        }
        Initialization::Lazy if !options.drop => {
            let q = quote_spanned! {sp=>{
                #initer
                unsafe{::static_init::Lazy::new_with_info(|| {#expr},
                    ::static_init::StaticInfo{
                        variable_name: ::core::stringify!(#statid),
                        file_name: ::core::file!(),
                        line: ::core::line!(),
                        column: ::core::column!(),
                        init_priority: -2,
                        drop_priority: -2 
                        }
                )}
            }
            }
            .into();
            *stat.expr = match parse(q) {
                Ok(exp) => exp,
                Err(e) => return e.to_compile_error(),
            }
        }
        Initialization::Lazy => {
            let q = quote_spanned! {sp=>{
                extern "C" fn __static_init_dropper() {
                    unsafe{::core::ptr::drop_in_place(::static_init::Lazy::as_mut_ptr(#stat_ref))}
                }
                #initer
                unsafe{::static_init::Lazy::new_with_info(|| {
                    let v = #expr;
                    unsafe{::libc::atexit(__static_init_dropper)};
                    v
                    },
                    ::static_init::StaticInfo{
                        variable_name: ::core::stringify!(#statid),
                        file_name: ::core::file!(),
                        line: ::core::line!(),
                        column: ::core::column!(),
                        init_priority: -2,
                        drop_priority: -2 
                        }

                    )}
            }
            }
            .into();
            *stat.expr = match parse(q) {
                Ok(exp) => exp,
                Err(e) => return e.to_compile_error(),
            }
        }
        Initialization::None => {
            assert!(options.drop);
            let q = quote_spanned! {sp=>{
                #initer
                #droper
                #typ::from(#expr,::static_init::StaticInfo{
                    variable_name: ::core::stringify!(#statid),
                    file_name: ::core::file!(),
                    line: ::core::line!(),
                    column: ::core::line!(),
                    init_priority: #init_priority,
                    drop_priority: #drop_priority
                    })
            }
            }
            .into();
            *stat.expr = match parse(q) {
                Ok(exp) => exp,
                Err(e) => return e.to_compile_error(),
            }
        }
    };

    *stat.ty = typ;

    quote_spanned! {sp=>

    #[allow(unused_unsafe)]
    #stat
    }
}
