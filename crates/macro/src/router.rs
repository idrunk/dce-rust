use std::collections::HashMap;
use proc_macro2::{Delimiter, Group, Span, TokenStream};
use quote::quote;
use quote::ToTokens;
use syn::{ItemFn, Token, LitStr, LitBool, ExprAssign, ExprStruct, Expr, Lit, ExprLit, Ident, Error, ExprTuple, ExprPath, Path, Member, ExprArray, ExprCall, PathSegment, parse_quote, QSelf, Type, TypePath, FnArg, PathArguments, AngleBracketedGenericArguments, ReturnType, GenericArgument, TypeTraitObject, TypeParamBound, TraitBound, Lifetime, TraitBoundModifier, TypeReference, TypeParen, ExprClosure, Pat, PatPath, ExprMacro, Macro, MacroDelimiter, GenericParam, Generics, LifetimeParam, ExprCast};
use syn::parse::{Parse, ParseStream, Result};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::Bracket;

macro_rules! props {
    ($($o: ident),+$(,)?) => {
        #[allow(non_camel_case_types)]
        #[derive(Clone, Debug)]
        enum Prop {
            $($o),+,
            Extra(String),
        }        
        impl From<String> for Prop {
            fn from(value: String) -> Self {
                match value.as_str() {
                    $(stringify!($o) => Prop::$o),+,
                    _ => Prop::Extra(value),
                }
            }
        }
    };
}
props!(id, path, serializer, deserializer, redirect, omission, name, unresponsive,);
const ORDERED_PROPS: [Prop; 8] = [Prop::path, Prop::serializer, Prop::deserializer, Prop::id, Prop::omission, Prop::redirect, Prop::name, Prop::unresponsive];

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
    fn set_by_key(&mut self, key: Prop, expr: Expr) -> Result<()> {
        const CONSUME_IF: fn(bool, &mut dyn FnMut()) -> bool = |result, callback| { if result { callback(); } result };
        if ! match &key {
            Prop::id => CONSUME_IF(matches!(self.id, None), &mut || self.id = Some(expr.clone())),
            Prop::path => CONSUME_IF(matches!(self.path, None), &mut || self.path = Some(expr.clone())),
            Prop::serializer => CONSUME_IF(matches!(self.serializers, None), &mut || self.serializers = Some(expr.clone())),
            Prop::deserializer => CONSUME_IF(matches!(self.deserializers, None), &mut || self.deserializers = Some(expr.clone())),
            Prop::redirect => CONSUME_IF(matches!(self.redirect, None), &mut || self.redirect = Some(expr.clone())),
            Prop::omission => CONSUME_IF(matches!(self.omission, None), &mut || self.omission = Some(expr.clone())),
            Prop::name => CONSUME_IF(matches!(self.name, None), &mut || self.name = Some(expr.clone())),
            Prop::unresponsive => CONSUME_IF(matches!(self.unresponsive, None), &mut || self.unresponsive = Some(expr.clone())),
            // put all non-standard meta into extras mapping
            Prop::Extra(key) => CONSUME_IF(! self.extras.contains_key(key), &mut || {self.extras.insert(key.clone(), expr.clone()); ()}),
        } {
            throw!(Span::call_site(), r#"Api arg "{:?}" can only set once"#, key);
        }
        Ok(())
    }

    fn process_controller(fn_name: String, mut func: ItemFn, request_type: Type) -> ItemFn {
        func.sig.ident = Ident::new(fn_name.as_str(), Span::call_site());
        if ! matches!(func.sig.generics.params.first(), Some(GenericParam::Lifetime(_))) {
            func.sig.generics = Generics {
                lt_token: Some(parse_quote!(<)),
                params: punctuated_create!(GenericParam::Lifetime(LifetimeParam::new(Lifetime::new("'a", func.sig.generics.span())))),
                gt_token: Some(parse_quote!(>)),
                where_clause: None,
            }
        }
        func.sig.output =  ReturnType::Type(parse_quote!(->), Box::new(Type::Path(TypePath {
            qself: None,
            path: Self::str_gen_path(vec![("dce_util", None), ("mixed", None), ("DceResult", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(
                punctuated_create!(GenericArgument::Type(Type::Path(TypePath{ qself: None, path: Self::str_gen_path(
                    vec![("Option", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(punctuated_create!(
                        GenericArgument::Type(Type::Path(TypePath{ qself: None, path: Self::str_gen_path(
                            vec![("dce_router", None), ("request", None), ("Response", Some(PathArguments::AngleBracketed(Self::gen_ab_generic_args(punctuated_create!(
                                GenericArgument::Type(Type::Path(TypePath{
                                    qself: Some(QSelf { lt_token: parse_quote!(<), ty: Box::new(request_type), position: 3, as_token: Some(parse_quote!(as)), gt_token: parse_quote!(>)}),
                                    path: Self::str_gen_path(vec![("dce_router", None), ("protocol", None), ("RoutableProtocol", None), ("Resp", None)])
                                }))
                            ), None))))]
                        ) }))
                    ), None))))]
                ) }))), None
            ))))])
        })));
        func
    }

    fn get_controller_method_extras(fn_name: &str, func: &mut ItemFn, extras: HashMap<String, Expr>) -> Result<(Expr, Expr, Punctuated<PathSegment, Token![::]>)> {
        let req_type_segments = match func.sig.inputs.first_mut() {
            Some(FnArg::Typed(pt)) => match pt.ty.as_mut() {
                Type::Path(tp) => &mut tp.path.segments,
                _ => throw!(pt.span(), r"Request arg need io parser generics"),
            },
            _ => throw!(func.sig.inputs.span(), r"Controller func need a Request input arg"),
        };
        // try to auto add lifetime generic to head of last path part        
        if let Some(PathSegment{arguments: PathArguments::AngleBracketed(AngleBracketedGenericArguments{args: ref mut generics, ..}), ..}) = req_type_segments.last_mut() {
            if ! generics.first().iter().all(|ga| matches!(ga, GenericArgument::Lifetime(_))) {
                generics.insert(0, GenericArgument::Lifetime(Lifetime::new("'a", generics.span())));
            }
        } else if let Some(ps) = req_type_segments.last_mut() {
            ps.arguments = PathArguments::AngleBracketed(AngleBracketedGenericArguments{
                colon2_token: None,
                lt_token: Default::default(),
                args: punctuated_create!(GenericArgument::Lifetime(Lifetime::new("'a", ps.span()))),
                gt_token: Default::default(),
            })
        }
        let segment_vec = req_type_segments.clone().into_iter().map(|ps|
            (ps.ident.to_string(), if let PathArguments::AngleBracketed(mut abga) = ps.arguments {
                abga.colon2_token = parse_quote!(::); Some(PathArguments::AngleBracketed(abga))
            } else { None })
        ).collect::<Vec<_>>();
        // rebuild the request arg type segments to method call style segments
        let segment_vec = segment_vec.iter().map(|(p, pa)| (p.as_str(), pa.clone())).collect::<Vec<_>>();
        // build method and extras data tuple vec
        let extras: Punctuated<_, Token![,]> = Punctuated::from_iter(extras.into_iter().map(|(k, mut v)| Self::gen_prop_tuple(k.as_str(), {
            // rewrap with vec macro if extra value is an array expr
            if let Expr::Array(ExprArray { elems, .. }) = v { v = Self::gen_bracket_macro(vec![("vec", None)], elems.to_token_stream()) }; v})));
        let method_extras = Self::gen_expr_call(punctuated_create!(Self::gen_bracket_macro(vec![("vec", None)], extras.to_token_stream())), 
            vec![("dce_router", None), ("protocol", None), ("RoutableProtocol", None), ("parse_api_method_and_extras", None)],
            Some(Self::gen_qself(vec![("dce_router", None), ("request", None), ("RequestTrait", None), ("Rp", None)],
                Some(Self::gen_qself(segment_vec.iter().map(|(path, generic)| (*path, generic.clone())).collect(), None, 3)), 3)));

        // build Controller enum
        let controller = if func.sig.asyncness.is_none() {
            Self::gen_expr_call(punctuated_create!(Expr::Path(ExprPath{attrs: vec![], qself: None, path: Self::str_gen_path(vec![(fn_name, None)])})),
                vec![("dce_router", None), ("api", None), ("Controller", None), ("Sync", None)], None)
        } else {
            let path = Self::str_gen_path(vec![("var", None)]);
            Self::gen_expr_call(punctuated_create!(Self::gen_expr_call(punctuated_create!(Expr::Closure(ExprClosure {
                attrs: vec![], lifetimes: None, constness: None, movability: None, asyncness: None, capture: None, or1_token: Default::default(),
                inputs: punctuated_create!(Pat::Path(PatPath {attrs: vec![], qself: None, path: path.clone()})), or2_token: Default::default(), output: ReturnType::Default,
                body: Box::new(Self::gen_expr_call(
                    punctuated_create!(Self::gen_expr_call(punctuated_create!(Expr::Path(ExprPath { attrs: vec![], qself: None, path})), vec![(fn_name, None)], None)),
                    vec![("Box", None), ("pin", None)], None)),
            })), vec![("Box", None), ("new", None)], None)), vec![("dce_router", None), ("api", None), ("Controller", None), ("Async", None)], None)
        };
        Ok((controller, method_extras, req_type_segments.clone()))
    }

    fn gen_prop_tuple(key: &str, value: Expr) -> Expr {
        Expr::Tuple(ExprTuple {
            attrs: vec![],
            paren_token: Default::default(),
            elems: punctuated_create!(
                Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new(key, Span::call_site())) }),
                match value {
                    call @ Expr::Call(_) if matches!(&call, Expr::Call(ExprCall{func, ..}) if func.to_token_stream().to_string().starts_with("Box")) => call,
                    expr => {
                        let is_lit = matches!(expr, Expr::Lit(_));
                        let mut boxed = Self::gen_expr_call(punctuated_create!(match expr {
                            Expr::Struct(struc) => Self::struct_to_converter(&struc, vec![]),
                            expr => expr,
                        }), vec![("Box", None), ("new", None)], None);
                        if is_lit {
                            // need an explicit cast to Box<dyn Any> if value is a literal
                            boxed = Expr::Cast(ExprCast { attrs: vec![], expr: Box::new(boxed), as_token: Default::default(), ty: Box::new(parse_quote!(Box<dyn std::any::Any>))})
                        }
                        boxed
                    },
                }
            ),
        })
    }

    fn struct_to_converter(expr_struct: &ExprStruct, excludes: Vec<&str>) -> Expr {
        let mut paths = expr_struct.path.segments.iter().map(|s| s.ident.to_string()).collect::<Vec<_>>();
        paths.push("from".to_string());
        let props: Punctuated<_, Token![,]> = Punctuated::from_iter(expr_struct.fields.iter().filter_map(|f| Some(match &f.member {
            Member::Named(name) => name.to_string(),
            Member::Unnamed(name) => name.index.to_string(),
        }).filter(|n| ! excludes.contains(&n.as_str())).map(|n| Self::gen_prop_tuple(n.as_str(), f.expr.clone()))));
        Self::gen_expr_call(
            punctuated_create!(Self::gen_bracket_macro(vec![("vec", None)], props.to_token_stream())),
            paths.iter().map(|ps| (ps.as_str(), None)).collect(), None,
        )
    }
    
    fn try_struct_to_boxed_converter(expr: Expr) -> Expr {
        Api::gen_expr_call(punctuated_create!(match expr {
            Expr::Struct(expr_struct) => Self::struct_to_converter(&expr_struct, vec![]),
            expr => expr,
        }), vec![("Box", None), ("new", None)], None)
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

    fn gen_qself(segments: Vec<(&str, Option<PathArguments>)>, qself: Option<QSelf>, position: usize) -> QSelf {
        QSelf {
            lt_token: parse_quote!(<),
            ty: Box::new(Type::Path(TypePath { qself, path: Self::str_gen_path(segments) })),
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
        const PROCESSOR: fn(Option<Expr>, Option<&Expr>) -> Expr = |configured, serializer| {
            match configured {
                // if is a vec just return directly no matter the items is boxed
                Some(ref vec @ Expr::Macro(ExprMacro{mac: Macro{ref path, ..}, ..})) if path.to_token_stream().to_string().starts_with("vec") => vec.clone(),
                Some(Expr::Array(ExprArray { elems, .. })) => Api::gen_bracket_macro(
                    vec![("vec", None)], Punctuated::<_, Token![,]>::from_iter(elems.into_iter().map(Api::try_struct_to_boxed_converter)).to_token_stream()),
                Some(serializer) => Api::gen_bracket_macro(vec![("vec", None)], Api::try_struct_to_boxed_converter(serializer).to_token_stream()),
                // else clone the serializer if passed or new an empty vec macro
                _ => serializer.map_or_else(|| Api::gen_bracket_macro(vec![("vec", None)], TokenStream::new()), Clone::clone),
            }
        };
        let serializers = PROCESSOR(serializers, None);
        let deserializers = PROCESSOR(deserializers, Some(&serializers));
        (serializers, deserializers)
    }

    pub fn processing(self, mut input: ItemFn) -> (ItemFn, Ident, ReturnType, TokenStream) {
        let Self{path, id, serializers, deserializers, omission, redirect, name,unresponsive , extras} = self;
        let route_fn_name = input.sig.ident.clone();
        let mut fn_name = input.sig.ident.to_string();
        let path = path.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new(fn_name.as_str(), Span::call_site())) }));
        let id = id.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new("", Span::call_site())) }));
        let (serializers, deserializers) = Self::gen_serializers(serializers, deserializers);
        let omission = omission.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Bool(LitBool::new(false, Span::call_site())) }));
        let redirect = redirect.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str(LitStr::new("", Span::call_site())) }));
        let name = name.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Str({
            let path = path.clone().into_token_stream().to_string().trim_matches('"').to_string();
            LitStr::new(&path.as_str()[path.rfind('/').map_or(0, |i| i + 1)..], Span::call_site())
        })}));
        let unresponsive = unresponsive.unwrap_or_else(|| Expr::Lit(ExprLit { attrs: vec![], lit: Lit::Bool(LitBool::new(false, Span::call_site())) }));

        fn_name.push_str("_api");
        let (controller, method_extras, req_type_segments) = Self::get_controller_method_extras(fn_name.as_str(), &mut input, extras)
            .unwrap_or_else(|err| panic!("{}", err));
        let paths = req_type_segments.iter().map(|seg| (seg.ident.to_string(), Some(seg.arguments.clone()))).collect::<Vec<(_, _)>>();
        let request_type = Type::Path(TypePath {
            qself: Some(Self::gen_qself(paths.iter().map(|(path, generic)| (path.as_str(), generic.clone())).collect(), None, 3)),
            path: Self::str_gen_path(vec![("dce_router", None), ("request", None), ("RequestTrait", None), ("Rp", None)])
        });
        let return_type = ReturnType::Type(parse_quote!(->), Box::new(Type::Reference(TypeReference{
            and_token: Default::default(),
            lifetime: Some(Lifetime::new("'static", Span::call_site())),
            mutability: None,
            elem: Box::new(Type::Paren(TypeParen { paren_token: Default::default(), elem: Box::new(Type::TraitObject(TypeTraitObject {
                dyn_token: Some(Default::default()), bounds: punctuated_create!(
                    TypeParamBound::Trait(TraitBound{paren_token: None, modifier: TraitBoundModifier::None, lifetimes: None, path: Self::str_gen_path(
                        vec![("dce_router", None), ("api", None),
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
            Box::leak(Box::new(dce_router::api::Api::new(
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
                    Expr::Path(expr) => api.set_by_key(Prop::from(expr.path.get_ident().map_or_else(|| panic!("Cannot unwrap"), ToString::to_string)), *right)?,
                    _ => throw!(Span::call_site(), "Arg name of api was invalid"),
                },
                expr => {
                    api.set_by_key(ORDERED_PROPS.get(prop_index).unwrap_or_else(|| panic!(r#"Api argument index "{}" was invalid"#, prop_index)).clone(), expr)?;
                    prop_index += 1;
                },
            }
            if input.parse::<Token![,]>().is_err() { break }
        }
        Ok(api)
    }
}
