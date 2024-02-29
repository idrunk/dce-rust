use proc_macro2::{Span, TokenStream, Ident};
use syn::{Error, Expr, ExprLit, Lit, LitInt, LitStr, parse_quote, Token};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;

pub struct DceError {
    code: Expr,
    formatter: LitStr,
    args: Punctuated<Expr, Token![,]>
}

impl DceError {
    pub fn gen_func(self, is_openly: bool) -> TokenStream {
        let DceError{code, formatter, args} = self;
        let openly = Ident::new(if is_openly {"Openly"} else {"Closed"}, Span::call_site());
        parse_quote!(dce_util::mixed::DceErr::#openly(dce_util::mixed::DceError {
            code: #code,
            message: format!(#formatter, #args),
        }))
    }
}

impl Parse for DceError {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let (code, formatter) = match input.parse() {
            Ok(expr @ (Expr::Path(_) | Expr::Lit(ExprLit { lit: Lit::Int(_), .. }))) if input.parse::<Token![,]>().is_ok() && ! input.is_empty() => (
                expr,
                if let Ok(Expr::Lit(ExprLit { lit: Lit::Str(formatter), .. })) = input.parse() { formatter } else { unreachable!() }
            ),
            Ok(Expr::Lit(ExprLit { lit: Lit::Str(formatter), .. })) => (
                Expr::Lit(ExprLit{ attrs: vec![], lit: Lit::Int(LitInt::new("0", Span::call_site())) }),
                formatter
            ),
            _ => panic!("{}", Error::new(Span::call_site(), "Must special a valid message")),
        };
        let _ = input.parse::<Token![,]>();
        Ok(DceError{ code, formatter, args: Punctuated::parse_terminated(input)? })
    }
}