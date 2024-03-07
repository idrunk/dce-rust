//! # Dce macros
//!
//! ## api(): proc_macro_attribute
//! An attribute-like macro, you can use it to define api, it will auto bind the controller func.
//!
//! **Parameters:**
//!
//!- *path `&str`*:\
//! The route path, it will be the controller func name if omitted, the style like "part1/part2", and support path params like the table below:
//!
//! example | matched | unmatched | description
//! - | - | - | -
//! {id}/detail | 1/detail | 1<br>1/info | required param, it could be every place in path
//! fruit/{target?} | fruit<br>fruit/apple | vegetable | optional param, it must be end of the path
//! fruit/{targets*} | fruit<br>fruit/apple/banana | vegetable | optional vec param, it must be end of the path
//! fruit/{targets+} | fruit/apple<br>fruit/apple/banana | fruit<br>vegetable | required vec param, it must be end of the path
//! fruit/{targets+}.\|html | fruit/apple<br>fruit/apple.html | fruit/apple.json | support non suffix or the ".html" suffix but not others
//! fruit/{targets+}.html\|json | fruit/apple.html<br>fruit/apple/banana.json | fruit/apple | support the ".html" or ".json" suffixes but not non or others
//!
//!- *serializer `Vec/struct`*:\
//! Response body data serializer, use to serialize the `DTO` into `sequences`, like `JsonSerializer{}`. It will be `UnreachableSerializer{}` if not defined.
//!
//!- *deserializer `Vec/struct`*:\
//! Request body data deserializer, use to deserialize the `sequences` into `DTO`, like `JsonSerializer{}`. It will be `UnreachableSerializer{}` if not defined.
//!
//!- *id `&str`*:\
//! Api ID, sometimes you want to use the shorter sign to mark an api, then you can define an id for it. Default value `""`.
//!
//!- *omission `bool`*:\
//! Define it is an omission part, for example `api("home/index", omission = true)` means you must use the path "home" to access it, because the "index" part is omission. Default value `false`.
//!
//!- *redirect `&str`*:\
//! Define the api should redirect to another one. Default value `""`.
//!
//!- *name `&str`*:\
//! Name the api. It will be the last part of path if not defined.
//!
//!- *unresponsive `bool`*:\
//! Define the api should not response, sometimes we want request a tcp or another long connection type service but not need response. Default value `false`.
//!
//! The params order is up to down, and you can use assignment expression style define it to break the fixed order.
//!
//
//! ## closed_err!(): proc_macro
//! A function-like macro to new a `DceErr` enum. Closed err means only print to console but not to response to client the specific error code and message.
//!
//! **Parameters:**
//!
//! - *code `isize`*:\
//! The error code, will be `0` if not specified.
//!
//! - *template `&str`*:\
//! The error message or a template.
//!
//! - *args `str-like[]`*:\
//! Template args.
//!
//!
//!  ## open_err!(): proc_macro
//! A function-like macro to new a `DceErr` enum. Openly err means the specific error code and message will respond to client. Params same to `closed_err!()`.
//!

#[macro_use]
mod macros;
mod funcs;
mod router;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};
use router::Api;
use funcs::DceError;

#[proc_macro_attribute]
pub fn api(args: TokenStream, input: TokenStream) -> TokenStream {
    let api = parse_macro_input!(args as Api);
    let input = parse_macro_input!(input as ItemFn);
    let (input, route_fn_name, return_type, api_scripts) = api.processing(input);

    TokenStream::from(quote!(
        #input
        pub fn #route_fn_name() #return_type { #api_scripts }
    ))
}

#[proc_macro]
pub fn openly_err(args: TokenStream) -> TokenStream {
    let err = parse_macro_input!(args as DceError);
    let call = err.gen_func(true);
    TokenStream::from(quote!(#call))
}

#[proc_macro]
pub fn closed_err(args: TokenStream) -> TokenStream {
    let err = parse_macro_input!(args as DceError);
    let call = err.gen_func(false);
    TokenStream::from(quote!(#call))
}