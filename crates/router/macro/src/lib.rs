#[macro_use]
mod macros;
mod funcs;
mod router;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, ItemFn};
use router::Api;
use crate::funcs::DceError;

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