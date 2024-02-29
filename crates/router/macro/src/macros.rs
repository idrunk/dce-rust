macro_rules! punctuated_create {
    ($($expr:expr),+ $(,)?) => { {
        let mut punctuated = Punctuated::new();
        $(punctuated.push($expr);)*
        punctuated
    } };
}

macro_rules! throw {
    ($span:expr, $message:expr $(, $arg:expr)* $(,)?) => { {
        return Err(Error::new($span, format!($message, $($arg),*)));
    } };
}
