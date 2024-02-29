use std::collections::HashMap;
use proc_macro2::{Delimiter, Group, Span, TokenStream};
use quote::quote;
use quote::ToTokens;
use syn::{ItemFn, Token, LitStr, LitBool, ExprAssign, ExprStruct, Expr, Lit, ExprLit, Ident, Error, ExprTuple, ExprPath, Path, Member, ExprArray, ExprCall, PathSegment, parse_quote, QSelf, Type, TypePath, FnArg, PathArguments, AngleBracketedGenericArguments, ReturnType, GenericArgument, TypeTraitObject, TypeParamBound, TraitBound, Lifetime, TraitBoundModifier, TypeReference, TypeParen, ExprClosure, Pat, PatPath, ExprMacro, Macro, MacroDelimiter};
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::token::Bracket;

const ORDERED_PROPS: [&str; 8] = ["path", "serializer", "deserializer", "id", "omission", "redirect", "name", "unresponsive"];

pub struct Api {
    pub id: Option<Expr>,
    pub path: Option<Expr>,
    pub serializers: Option<Expr>,
    pub deserializers: Option<Expr>,
    pub omission: Option<Expr>,
    pub redirect: Option<Expr>,
    pub name: Option<Expr>,
    pub unresponsive: Option<Expr>,
    pub extras: HashMap<String, Expr>,
}

impl Api {
    fn set_by_key(&mut self, key: String, mut expr: Expr) -> Result<()> {
        if let Expr::Array(ExprArray { elems, .. }) = expr { expr = Self::gen_bracket_macro(vec![("vec", None)], elems.to_token_stream()) };
        let consume_if = |result: bool, callback: &mut dyn FnMut()| { if result { callback(); } result };
        if ! match key.as_str() {
            "id" => consume_if(matches!(self.id, None), &mut || self.id = Some(expr.clone())),
            "path" => consume_if(matches!(self.path, None), &mut || self.path = Some(expr.clone())),
            "serializer" => consume_if(matches!(self.serializers, None), &mut || self.serializers = Some(expr.clone())),
            "deserializer" => consume_if(matches!(self.deserializers, None), &mut || self.deserializers = Some(expr.clone())),
            "redirect" => consume_if(matches!(self.redirect, None), &mut || self.redirect = Some(expr.clone())),
            "omission" => consume_if(matches!(self.omission, None), &mut || self.omission = Some(expr.clone())),
            "name" => consume_if(matches!(self.name, None), &mut || self.name = Some(expr.clone())),
            "unresponsive" => consume_if(matches!(self.unresponsive, None), &mut || self.unresponsive = Some(expr.clone())),
            _ => consume_if(! self.extras.contains_key(&key), &mut || {self.extras.insert(key.clone(), expr.clone()); ()}),
        } {
            throw!(Span::call_site(), r#"Api arg "{}" can only set once"#, key);
        }
        Ok(())
    }

    fn process_controller(fn_name: String, mut func: ItemFn, request_type: Type) -> ItemFn {
        func.sig.ident = Ident::new(fn_name.as_str(), Span::call_site());
        func.sig.output =  ReturnType::Type(parse_quote!(->), Box::new(Type::Path(TypePath {
            qself: None,
            path: Self::str_gen_path(vec![("dce_router", None), ("util", None), ("DceResult", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(
                punctuated_create!(GenericArgument::Type(Type::Path(TypePath{ qself: None, path: Self::str_gen_path(
                    vec![("Option", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(punctuated_create!(
                        GenericArgument::Type(Type::Path(TypePath{
                            qself: Some(QSelf { lt_token: parse_quote!(<), ty: Box::new(request_type), position: 4, as_token: Some(parse_quote!(as)), gt_token: parse_quote!(>)}),
                            path: Self::str_gen_path(vec![("dce_router", None), ("router", None), ("request", None), ("RawRequest", None), ("Resp", None)])
                        }))
                    ), None))))]
                ) }))), None
            ))))])
        })));
        func
    }

    fn get_controller_method_extras(fn_name: &str, func: &ItemFn, props: HashMap<String, Expr>) -> Result<(Expr, Expr, Punctuated<PathSegment, Token![::]>)> {
        let segments = match func.sig.inputs.first() {
            Some(FnArg::Typed(pt)) => match *pt.ty.clone() {
                Type::Path(tp) => tp.path.segments,
                _ => throw!(Span::call_site(), r"Request arg need io parser generics"),
            },
            _ => throw!(Span::call_site(), r"Controller func need a Request input arg"),
        };
        let generic_segments = segments.clone();
        let mut method_props: Punctuated<_, Token![,]> = Punctuated::new();
        props.iter().for_each(|(k, v)| method_props.push(Self::gen_prop_tuple(k.as_str(), v.clone())));
        let segments_hold = segments.into_iter().map(|ps|
            (ps.ident.to_string(), if let PathArguments::AngleBracketed(mut abga) = ps.arguments {
                abga.colon2_token = parse_quote!(::); Some(PathArguments::AngleBracketed(abga))
            } else { None })
        ).collect::<Vec<_>>();
        let mut segments = segments_hold.iter().map(|(p, pa)| (p.as_str(), pa.clone())).collect::<Vec<_>>();
        segments.push(("parse_api_method_and_extras", None));
        let method_extras = Self::gen_expr_call(punctuated_create!(Self::gen_bracket_macro(vec![("vec", None)], method_props.to_token_stream())), segments, None);

        let controller = if func.sig.asyncness.is_none() {
            Self::gen_expr_call(punctuated_create!(Expr::Path(ExprPath{attrs: vec![], qself: None, path: Self::str_gen_path(vec![(fn_name, None)])})),
                                vec![("dce_router", None), ("router", None), ("api", None), ("Controller", None), ("Sync", None)], None)
        } else {
            let path = Self::str_gen_path(vec![("var", None)]);
            Self::gen_expr_call(punctuated_create!(Self::gen_expr_call(punctuated_create!(Expr::Closure(ExprClosure {
                attrs: vec![], lifetimes: None, constness: None, movability: None, asyncness: None, capture: None, or1_token: Default::default(),
                inputs: punctuated_create!(Pat::Path(PatPath {attrs: vec![], qself: None, path: path.clone()})), or2_token: Default::default(), output: ReturnType::Default,
                body: Box::new(Self::gen_expr_call(
                    punctuated_create!(Self::gen_expr_call(punctuated_create!(Expr::Path(ExprPath { attrs: vec![], qself: None, path})), vec![(fn_name, None)], None)),
                    vec![("Box", None), ("pin", None)], None)),
            })), vec![("Box", None), ("new", None)], None)), vec![("dce_router", None), ("router", None), ("api", None), ("Controller", None), ("Async", None)], None)
        };
        Ok((controller, method_extras, generic_segments))
    }

    fn gen_prop_tuple(key: &str, value: Expr) -> Expr {
        Expr::Tuple(ExprTuple {
            attrs: vec![],
            paren_token: Default::default(),
            elems: punctuated_create!(
                Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new(key, Span::call_site())) }),
                match value {
                    call @ Expr::Call(_) if matches!(&call, Expr::Call(ExprCall{func, ..}) if func.to_token_stream().to_string().starts_with("Box")) => call,
                    expr => Self::gen_expr_call(punctuated_create!(match expr {
                        Expr::Struct(struc) => Self::struct_to_converter(&struc, Punctuated::new(), vec![]),
                        expr => expr,
                    }), vec![("Box", None), ("new", None)], None),
                }
            ),
        })
    }

    fn gen_ab_generic_args(generics: Punctuated<GenericArgument, Token![,]>, colon2_token: Option<Token![::]>) -> AngleBracketedGenericArguments {
        AngleBracketedGenericArguments {colon2_token, lt_token: Default::default(), args: generics, gt_token: Default::default()}
    }

    fn gen_expr_call(args: Punctuated<Expr, Token![,]>, segments: Vec<(&str, Option<PathArguments>)>, qself: Option<QSelf>) -> Expr {
        Expr::Call(ExprCall {
            attrs: vec![],
            func: Box::new(Expr::Path(ExprPath { attrs: vec![], qself, path: Self::str_gen_path(segments), })),
            paren_token: Default::default(),
            args,
        })
    }

    fn str_gen_path(segments: Vec<(&str, Option<PathArguments>)>) -> Path {
        Path { leading_colon: None, segments: segments.into_iter().map(|(path, generics)| PathSegment {
            ident: Ident::new(path, Span::call_site()),
            arguments: if let Some(args) = generics { args } else { Default::default() },
        }).collect() }
    }

    fn struct_to_converter(stru: &ExprStruct, generics: Punctuated<GenericArgument, Token![,]>, excludes: Vec<&str>) -> Expr {
        let mut props = Punctuated::new();
        stru.fields.iter().for_each(|f| if let Some(name) = match &f.member {
            Member::Named(name) => match name.to_string() { name if excludes.contains(&name.as_str()) => None, name => Some(name), },
            Member::Unnamed(name) => Some(name.to_token_stream().to_string()),
        } { props.push(Self::gen_prop_tuple(name.as_str(), f.expr.clone())); });
        let (last_seg_idx, paths) = (stru.path.segments.len() - 1, stru.path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>());
        Self::gen_expr_call(
            punctuated_create!(Expr::Array(ExprArray { attrs: vec![], bracket_token: Default::default(), elems: props, })),
            vec![("dce_router", None),("router", None),("api", None),("ToStruct", None),("from", None)],
            Some(Self::gen_qself(paths.iter().enumerate().map(|(idx, path)| (path.as_str(), if generics.is_empty() || idx < last_seg_idx {None} else {
                Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(generics.clone(), None)))})).collect(), 4)),
        )
    }

    fn gen_qself(segments: Vec<(&str, Option<PathArguments>)>, position: usize) -> QSelf {
        QSelf {
            lt_token: parse_quote!(<),
            ty: Box::new(Type::Path(TypePath { qself: None, path: Self::str_gen_path(segments) })),
            position,
            as_token: Some(parse_quote!(as)),
            gt_token: parse_quote!(>),
        }
    }

    fn gen_bracket_macro(segments: Vec<(&str, Option<PathArguments>)>, tokens: TokenStream) -> Expr {
        Expr::Macro(ExprMacro{ attrs: vec![], mac: Macro{
            path: Self::str_gen_path(segments),
            bang_token: Default::default(),
            delimiter: MacroDelimiter::Bracket(Bracket{ span: Group::new(Delimiter::Bracket, Default::default()).delim_span() }),
            tokens,
        } })
    }

    fn gen_serializers(serializers: Option<Expr>, deserializers: Option<Expr>) -> (Expr, Expr) {
        let serializers = match serializers {
            Some(vec @ Expr::MethodCall(_)) => vec,
            Some(serializer) => Self::gen_bracket_macro(
                vec![("vec", None)],
                Self::gen_expr_call(punctuated_create!(serializer), vec![("Box", None), ("new", None)], None).to_token_stream()
            ),
            _ => Self::gen_bracket_macro(vec![("vec", None)], Self::gen_expr_call(punctuated_create!(Self::struct_to_converter(&ExprStruct {
                attrs: vec![],
                qself: None,
                path: Self::str_gen_path(vec![("dce_router", None), ("router", None), ("serializer", None), ("UnreachableSerializer", None)]),
                brace_token: Default::default(),
                fields: Default::default(),
                dot2_token: None,
                rest: None,
            }, Punctuated::new(), vec![])), vec![("Box", None), ("new", None)], None).to_token_stream()),
        };
        let deserializers = match deserializers {
            Some(vec @ Expr::MethodCall(_)) => vec,
            Some(deserializers) => Self::gen_bracket_macro(
                vec![("vec", None)],
                Self::gen_expr_call(punctuated_create!(deserializers), vec![("Box", None), ("new", None)], None).to_token_stream(),
            ),
            _ => serializers.clone(),
        };
        (serializers, deserializers)
    }

    pub fn processing(self, input: ItemFn) -> (ItemFn, Ident, ReturnType, TokenStream) {
        let Self{path, id, serializers, deserializers, omission, redirect, name,unresponsive , extras} = self;
        let route_fn_name = input.sig.ident.clone();
        let mut fn_name = input.sig.ident.to_string();
        let path = if let Some(e) = path { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new(fn_name.as_str(), Span::call_site())) }) };
        let id = if let Some(e) = id { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new("", Span::call_site())) }) };
        let (serializers, deserializers) = Self::gen_serializers(serializers, deserializers);
        let omission = if let Some(e) = omission { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Bool(LitBool::new(false, Span::call_site())) }) };
        let redirect = if let Some(e) = redirect { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new("", Span::call_site())) }) };
        let name = if let Some(e) = name { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str({
            let path = path.clone().into_token_stream().to_string().trim_matches('"').to_string();
            LitStr::new(&path.as_str()[path.rfind('/').map_or(0, |i| i + 1)..], Span::call_site())
        })}) };
        let unresponsive = if let Some(e) = unresponsive { e } else { Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Bool(LitBool::new(false, Span::call_site())) }) };

        fn_name.push_str("_api");
        let (controller, method_extras, generic_segments) = Self::get_controller_method_extras(fn_name.as_str(), &input, extras)
            .unwrap_or_else(|err| panic!("{}", err));
        let paths_holder = generic_segments.iter().map(|seg| (seg.ident.to_string(), Some(seg.arguments.clone()))).collect::<Vec<(_, _)>>();
        let request_type = Type::Path(TypePath {
            qself: Some(Self::gen_qself(paths_holder.iter().map(|(path, generic)| (path.as_str(), generic.clone())).collect(), 4)),
            path: Self::str_gen_path(vec![("dce_router", None), ("router", None), ("request", None), ("RequestTrait", None), ("Raw", None)])
        });
        let return_type = ReturnType::Type(parse_quote!(->), Box::new(Type::Reference(TypeReference{
            and_token: Default::default(),
            lifetime: Some(Lifetime::new("'static", Span::call_site())),
            mutability: None,
            elem: Box::new(Type::Paren(TypeParen { paren_token: Default::default(), elem: Box::new(Type::TraitObject(TypeTraitObject {
                dyn_token: Some(Default::default()), bounds: punctuated_create!(
                    TypeParamBound::Trait(TraitBound{paren_token: None, modifier: TraitBoundModifier::None, lifetimes: None, path: Self::str_gen_path(
                        vec![("dce_router", None), ("router", None), ("api", None),
                            ("ApiTrait", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(punctuated_create!( GenericArgument::Type(request_type.clone()) ), None))))
                        ]),}),
                    TypeParamBound::Trait(TraitBound{paren_token: None, modifier: TraitBoundModifier::None, lifetimes: None, path: Self::str_gen_path(vec![("Send", None)]),}),
                    TypeParamBound::Trait(TraitBound{paren_token: None, modifier: TraitBoundModifier::None, lifetimes: None, path: Self::str_gen_path(vec![("Sync", None)]),}),
                )
            })) })),
        })));
        let input = Self::process_controller(fn_name, input, request_type);

        (input, route_fn_name, return_type, quote!(
            let (method, extras) = #method_extras;
            Box::leak(Box::new(dce_router::router::api::Api::new(
                #controller,
                #deserializers,
                #serializers,
                method,
                #path,
                #id,
                #omission,
                #redirect,
                #name,
                #unresponsive,
                extras,
            )))
        ))
    }
}

impl Parse for Api {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut api = Api {
            id: None,
            path: None,
            serializers: None,
            deserializers: None,
            omission: None,
            redirect: None,
            name: None,
            unresponsive: None,
            extras: Default::default(),
        };
        let mut prop_index = 0;
        while let Ok(expr) = input.parse() {
            match expr {
                Expr::Assign(ExprAssign{left, right, ..}) => match *left {
                    Expr::Path(expr) => api.set_by_key(expr.path.get_ident().unwrap().to_string(), *right)?,
                    _ => throw!(Span::call_site(), "Arg name of api was invalid"),
                },
                expr => {
                    let prop = *ORDERED_PROPS.get(prop_index).unwrap_or_else(|| panic!(r#"Api argument index "{}" was invalid"#, prop_index));
                    api.set_by_key(prop.to_string(), expr)?;
                    prop_index += 1;
                },
            }
            if input.parse::<Token![,]>().is_err() { break }
        }
        Ok(api)
    }
}
