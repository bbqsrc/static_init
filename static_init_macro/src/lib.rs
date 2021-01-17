extern crate proc_macro;
extern crate syn;
use syn::*;
use syn::spanned::Spanned;

extern crate quote;
use quote::{quote,quote_spanned};

use proc_macro::TokenStream;

extern crate proc_macro2;
use proc_macro2::{Span, TokenStream as TokenStream2};

#[proc_macro_attribute]
pub fn constructor(args: TokenStream, input: TokenStream) -> TokenStream {
    let func: ItemFn = parse_macro_input!(input);
    
    let priority = match parse_priority(args) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };

    let section = if cfg!(target_os = "linux") {
        if let Some(p) = priority {
            format!(".init_array.{:05}",p)
        } else {
            format!(".init_array")
        }
    } else if cfg!(target_os = "macos") {
            format!("__DATA,__mod_init_func")
    } else if cfg!(target_os = "windows") {
            format!(".CRT$XCU")
    } else {
        unimplemented!()
    };

    let mod_name = format!("__static_init_constructor_{}",func.sig.ident);
    gen_ctor_dtor(func,&section,&mod_name).into()
}

#[proc_macro_attribute]
pub fn destructor(args: TokenStream, input: TokenStream) -> TokenStream {
    let func: ItemFn = parse_macro_input!(input);
    
    let priority = match parse_priority(args) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };
    let section = if cfg!(target_os = "linux") {
        if let Some(p) = priority {
            format!(".fini_array.{:05}",p)
        } else {
            format!(".fini_array")
        }
    } else if cfg!(target_os = "macos") {
            format!("__DATA,__mod_term_func")
    } else if cfg!(target_os = "windows") {
            format!(".CRT$XPU")
    } else {
        unimplemented!()
    };

    let mod_name = format!("__static_init_constructor_{}",func.sig.ident);
    gen_ctor_dtor(func,&section,&mod_name).into()
}
struct DynOptions{
    init: bool,
    init_priority: Option<u16>,
    drop: bool,
    drop_priority: Option<u16>,
}

#[proc_macro_attribute]
pub fn dynamic(args: TokenStream, input: TokenStream) -> TokenStream {
    let item: ItemStatic = parse_macro_input!(input);
    
    let options = match parse_dyn_options(parse_macro_input!(args)) {
        Ok(v) => v,
        Err(e) => return e.into(),
    };

    gen_dyn_init(item,options).into()
}

fn parse_priority(args: TokenStream) -> std::result::Result<Option<u16>,TokenStream2> {
    if !args.is_empty() {
        let n: LitInt = syn::parse(args).map_err(|e| e.to_compile_error())?; 

         Ok(Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?))
    } else {
        Ok(None)
    }
}

fn parse_dyn_options(args: AttributeArgs) -> std::result::Result<DynOptions,TokenStream2> {
    if !args.is_empty() {
      let mut opt = DynOptions{
        init:false,
        init_priority:None,
        drop:false,
        drop_priority:None};
        for arg in args {
            match arg {
                NestedMeta::Meta(Meta::Path(id)) => {
                    let id = if let Some(id) = id.get_ident() { 
                        id
                    } else {
                        return Err(quote_spanned!(id.span()=>compile_error!("Unexpected attribute argument #id")).into())
                    };
                    if id == "init" {
                        opt.init=true;
                    } else if id == "drop" {
                        opt.drop=true;
                    } else {
                        return Err(quote_spanned!(id.span()=>compile_error!("Unexpected attribute argument #id")).into());
                    }
                }
                NestedMeta::Meta(Meta::NameValue(nv)) => {
                    let id = if let Some(id) = nv.path.get_ident() { 
                        id
                    } else {
                        return Err(quote_spanned!(nv.path.span()=>compile_error!("Unexpected attribute argument #id")).into())
                    };
                    if id == "init" {
                        opt.init=true;
                        if let Lit::Int(n) = nv.lit {
                            opt.init_priority=Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                        } else {
                            return Err(quote_spanned!(nv.lit.span()=>compile_error!("Expected an init priority (u16)")).into());
                        }
                    } else 
                    if id == "drop" {
                        opt.drop=true;
                        if let Lit::Int(n) = nv.lit {
                            opt.drop_priority=Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                        } else {
                            return Err(quote_spanned!(nv.lit.span()=>compile_error!("Expected a drop priority (u16)")).into());
                        }
                    } else {
                            return Err(quote_spanned!(id.span()=>compile_error!("Expected eithe 'init' or 'drop'")).into());
                    }

                }
                NestedMeta::Lit(Lit::Int(n)) => {
                    opt.init=true;
                    opt.init_priority=Some(n.base10_parse::<u16>().map_err(|e| e.to_compile_error())?);
                }
                _ => return Err(quote_spanned!(arg.span()=>compile_error!("Expected either 'init' or 'drop'")).into()),
            }
        }

        Ok(opt)

    } else {
      Ok(DynOptions{
        init:true,
        init_priority:None,
        drop:true,
        drop_priority:None})
    }
}

fn gen_ctor_dtor(func: ItemFn, section: &str, mod_name: &str) -> TokenStream2 {
    let mod_name = Ident::new(mod_name, Span::call_site());
    let section = LitStr::new(section,Span::call_site());
    let func_name = &func.sig.ident;

    quote!{
        #func
        #[doc(hidden)]
        pub mod #mod_name {
            #[link_section = #section]
            #[used]
            pub static INIT_FUNC: fn() = super::#func_name;
        }
    }
}

fn gen_dyn_init(mut stat: ItemStatic, options: DynOptions) -> TokenStream2 {
    let mod_name = Ident::new(&format!("_static_init_of_{}",stat.ident), Span::call_site());
    let stat_name = &stat.ident;
    let expr = &*stat.expr;
    let stat_typ = &*stat.ty;
    let (typ, stat_ref): (Type,Expr) = if stat.mutability.is_some() {
        (parse_quote!{
            ::static_init::Static::<#stat_typ>
        },
        parse_quote!{
            &mut #stat_name
        })
    } else {
        (parse_quote!{
            ::static_init::ConstStatic::<#stat_typ>
        },
        parse_quote!{
            &#stat_name
        })
    };
    let initer = if options.init {
            let con_attr:Attribute = if let Some(priority) = options.init_priority {
                parse_quote!(#[constructor(#priority)])
                } else {
                parse_quote!(#[constructor])
                };
            Some(quote!{
                    use ::static_init::{constructor};
                    #con_attr
                    fn init() {
                        use super::*;
                        let r = #expr;
                        unsafe{#typ::set_to(#stat_ref,r)}
                    }
            })
    } else { None};
    let droper = if options.drop {
            let con_attr:Attribute = if let Some(priority) = options.drop_priority {
                parse_quote!(#[destructor(#priority)])
                } else {
                parse_quote!(#[destructor])
                };
            Some(quote!{
                    use ::static_init::{destructor};
                    #con_attr
                    fn droper() {
                        use super::*;
                        unsafe{#typ::drop(#stat_ref)}
                    }
            })
    } else { None};
    *stat.expr = parse_quote!{
        #typ::uninit()
    };
    *stat.ty = typ;

    quote!{
    #stat

    #[doc(hidden)]
    pub mod #mod_name {
        #initer
        #droper
    }
    }
}
