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

/// Attribute for functions run at program initialization (before main)
///
/// ```ignore
/// #[constructor]
/// fn initer () {
/// // run before main start
/// }
/// ```
/// The execution order of constructors is unspecified. Nevertheless on ELF plateform (linux,any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `constructor(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1]. 
///
/// Constructors with priority number 0 are run first (in unspecified order), then functions
/// with priority number 1 are run ...  then functions
/// with priority number 65535 and finaly constructors with no priority. 
///
/// ```ignore
/// #[constructor(0)]
/// fn first () {
/// // run before main start
/// }
///
/// #[constructor(1)]
/// fn then () {
/// // run before main start
/// }
/// ```
///
/// NB: Whatever the priority constructors are run after initialization of libc resources. C++ static
/// objects are initialized as constructors with no priorities. On ELF plateform libstdc++
/// resources are initialized with priority 100.
///
///
//        crate::os::raw::c_int,
//        *const *const u8,
//        *const *const u8,
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
            format!(".init_array")
        }
    } else if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        if priority.is_some() {
            return quote!(compile_error!("Constructor priority not supported on this plateform.")).into();
        }
        format!("__DATA,__mod_init_func")
    } else if cfg!(target_os = "windows") {
        if let Some(p) = priority {
            format!(".CRT$XCTZ{:05}", p)
        } else {
            format!(".CRT$XCU")
        }
    } else {
        return quote!(compile_error!("Target not supported")).into();
    };

    let mod_name = format!("__static_init_constructor_{}", func.sig.ident);
    gen_ctor_dtor(func, &section, &mod_name).into()
}

/// Attribute for functions run at program termination (after main)
///
/// ```ignore
/// #[destructor]
/// fn droper () {
/// // run before main start
/// }
/// ```
///
/// The execution order of destructors is unspecified. Nevertheless on ELF plateform (linux,any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `destructor(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1]. 
///
/// Destructors without priority are run first (in unspecified order), then destructors with priority 65535 are run,
/// the destructors with priority number 65534,... finaly destructors with priority 0 are run. 
///
/// ```ignore
/// #[destructor(1)]
/// fn first () {
/// // run after main return
/// }
///
/// #[destructor(0)]
/// fn then () {
/// // run after main return
/// }
/// ```
///
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
            format!(".fini_array.{:05}", p as u32 + 1)
        } else {
            format!(".fini_array")//so that it comes the latter
        }
    } else 
        if cfg!(target_os = "macos") || cfg!(target_os = "ios") {
        if priority.is_some() {
            return quote!(compile_error!("Constructor priority not supported on this plateform.")).into();
        }
            format!("__DATA,__mod_term_func")
        } else if cfg!(target_os = "windows") {
        if let Some(p) = priority {
            format!(".CRT$XPTZ{:05}", p)
        } else {
            format!(".CRT$XPU")
        }
        } else {
            return quote!(compile_error!("Target not supported")).into();
    };

    let mod_name = format!("__static_init_constructor_{}", func.sig.ident);
    gen_ctor_dtor(func, &section, &mod_name).into()
}

/// Statics initialized with non const functions.
///
/// Statics on which this attribute is applied will be
/// be initialized at run time (optionaly see bellow), before
/// main start. This allow statics initialization with non
/// const expressions.
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
/// static V :A = A::new(42);
/// ```
/// The execution order of destructors is unspecified. Nevertheless on ELF plateform (linux,any unixes but mac) and
/// windows plateform a priority can be specified using the syntax `dynamic(<num>)` where
/// `<num>` is a number included in the range [0 ; 2<sup>16</sup>-1]. 
///
/// Statics with priority number 0 are initialized first (in unspecified order), then statics
/// with priority number 1 are initialized ...  then statics
/// with priority number 65535 and finaly statics with no priority. 
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
/// static mut V1 :A = A::new(33);
///
/// #[dynamic(20)]
/// static V2 :A = A::new(unsafe{V1.0} + 9);
/// ```
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
/// The macro attribute `dynamic` is equivalent to `dynamic(init=65535)`
/// and `dynamic(<num>)` to `dynamic(init=65535)`. In the absence of `init`
/// dyn_opt, the static will not be created dynamically. The `drop` dyn_opt
/// cause the static to be droped after main returns. The priority in as the
/// same semantic as for the [macro@destructor] attribute: statics without priority
/// are droped first, then statics with priority 65536 and finaly statics with priority
/// 0 are the last dropped.
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
/// static mut V1 :A = A::new_const(33);
///
/// //initialized before V1 and droped after V1
/// #[dynamic(20,drop=10)]
/// static V2 :A = A::new(10);
///
/// //as above
/// #[dynamic(init=20,drop=10)]
/// static V3 :A = A::new(10);
///
/// // not droped
/// #[dynamic(init)]
/// static V4 :A = A::new(10);
///
/// // not droped
/// #[dynamic]
/// static V5 :A = A::new(10);
///
/// // not droped
/// #[dynamic(10)]
/// static V6 :A = A::new(10);
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

struct DynOptions {
    init: bool,
    init_priority: Option<u16>,
    drop: bool,
    drop_priority: Option<u16>,
}

fn parse_priority(args: TokenStream) -> std::result::Result<Option<u16>, TokenStream2> {
    if !args.is_empty() {
        let n: LitInt = syn::parse(args).map_err(|e| e.to_compile_error())?;

        Ok(Some(
            n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?,
        ))
    } else {
        Ok(None)
    }
}

fn parse_dyn_options(args: AttributeArgs) -> std::result::Result<DynOptions, TokenStream2> {
    if !args.is_empty() {
        let mut opt = DynOptions {
            init: false,
            init_priority: None,
            drop: false,
            drop_priority: None,
        };
        for arg in args {
            match arg {
                NestedMeta::Meta(Meta::Path(id)) => {
                    let id = if let Some(id) = id.get_ident() {
                        id
                    } else {
                        return Err(quote_spanned!(id.span()=>compile_error!("Unexpected attribute argument #id")).into());
                    };
                    if id == "init" {
                        opt.init = true;
                    } else if id == "drop" {
                        opt.drop = true;
                    } else {
                        return Err(quote_spanned!(id.span()=>compile_error!("Unexpected attribute argument #id")).into());
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) => {
                    let id = if let Some(id) = nv.path.get_ident() {
                        id
                    } else {
                        return Err(quote_spanned!(nv.path.span()=>compile_error!("Unexpected attribute argument #id")).into());
                    };
                    if id == "init" {
                        opt.init = true;
                        if let Lit::Int(n) = nv.lit {
                            opt.init_priority =
                                Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                        } else {
                            return Err(quote_spanned!(nv.lit.span()=>compile_error!("Expected an init priority (u16)")).into());
                        }
                    } else if id == "drop" {
                        opt.drop = true;
                        if let Lit::Int(n) = nv.lit {
                            opt.drop_priority =
                                Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                        } else {
                            return Err(quote_spanned!(nv.lit.span()=>compile_error!("Expected a drop priority (u16)")).into());
                        }
                    } else {
                        return Err(quote_spanned!(id.span()=>compile_error!("Expected eithe 'init' or 'drop'")).into());
                    }
                }
                NestedMeta::Lit(Lit::Int(n)) => {
                    opt.init = true;
                    opt.init_priority =
                        Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                }
                _ => return Err(
                    quote_spanned!(arg.span()=>compile_error!("Expected either 'init' or 'drop'"))
                        .into(),
                ),
            }
        }

        Ok(opt)
    } else {
        Ok(DynOptions {
            init: true,
            init_priority: None,
            drop: true,
            drop_priority: None,
        })
    }
}

fn gen_ctor_dtor(func: ItemFn, section: &str, mod_name: &str) -> TokenStream2 {
    let mod_name = Ident::new(mod_name, Span::call_site());
    let section = LitStr::new(section, Span::call_site());
    let func_name = &func.sig.ident;
    let ext = &func.sig.abi;
    let input = &func.sig.inputs;
    let ret = &func.sig.output;


    quote! {
        #func
        #[doc(hidden)]
        pub mod #mod_name {
            #[link_section = #section]
            #[used]
            pub static INIT_FUNC: #ext fn (#input) #ret = super::#func_name;
        }
    }
}

fn gen_dyn_init(mut stat: ItemStatic, options: DynOptions) -> TokenStream2 {
    let mod_name = Ident::new(
        &format!("_static_init_of_{}", stat.ident),
        Span::call_site(),
    );
    let stat_name = &stat.ident;
    let expr = &*stat.expr;
    let stat_typ = &*stat.ty;
    let (typ, stat_ref): (Type, Expr) = if stat.mutability.is_some() {
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
    };
    let initer = if options.init {
        let con_attr: Attribute = if let Some(priority) = options.init_priority {
            parse_quote!(#[constructor(#priority)])
        } else {
            parse_quote!(#[constructor])
        };
        Some(quote! {
                use ::static_init::{constructor};
                #con_attr
                fn init() {
                    use super::*;
                    let r = #expr;
                    unsafe{#typ::set_to(#stat_ref,r)}
                }
        })
    } else {
        None
    };
    let droper = if options.drop {
        let con_attr: Attribute = if let Some(priority) = options.drop_priority {
            parse_quote!(#[destructor(#priority)])
        } else {
            parse_quote!(#[destructor])
        };
        Some(quote! {
                use ::static_init::{destructor};
                #con_attr
                fn droper() {
                    use super::*;
                    unsafe{#typ::drop(#stat_ref)}
                }
        })
    } else {
        None
    };
    if options.init {
        *stat.expr = parse_quote! {
            #typ::uninit()
        };
    } else {
        assert!(options.drop);
        *stat.expr = parse_quote! {
            #typ::from(#expr)
        };
    }
    *stat.ty = typ;

    quote! {
    #stat

    #[doc(hidden)]
    pub mod #mod_name {
        #initer
        #droper
    }
    }
}
