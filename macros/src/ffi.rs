#[macro_export]
macro_rules! cstr_methods {
    ($(fn $func:tt ($var:ident) { $act:expr })+) => (
        $(
            #[no_mangle]
            pub unsafe extern "C" fn $func(
                $var: *const libc::c_char
            ) -> *mut libc::c_char {
                get_str($var, "")
                    .ok()
                    .and_then(|$var| $act)
                    .map(|x| to_cstr(x.into()))
                    .unwrap_or(ptr::null_mut())
            }
        )+
    );
}

#[macro_export]
macro_rules! cvec_from {
    (for $var:ident in $from:expr, push $act:expr, record $len:ident) => {{
        let mut output = Vec::new();
        for $var in $from {
            output.push($act);
        }

        *$len = output.len() as libc::c_int;
        Box::into_raw(output.into_boxed_slice()) as *mut *mut _
    }};
}

#[macro_export]
macro_rules! c_expand {
    // Do nothing in the event that nothing is required.
    (norm $var:ident ? $default:expr) => ();

    // Attempt to get a Rust string from a C string.
    (string $var:ident ? $default:expr) => (
        let $var = match get_str($var, "").ok() {
            Some(string) => string,
            None => {
                return $default;
            }
        };
    );

    // Create a boxed value from a pointer.
    (boxed $var:ident ? $default:expr) => (
        if $var.is_null() {
            return $default;
        }

        let $var = Box::from_raw($var as *mut _);
    );
}

#[macro_export]
macro_rules! expand_object {
    (mut $fst:ident as $as_t:ty) => (
        let $fst = &mut *($fst as *mut $as_t);
    );

    (const $fst:ident as $as_t:ty) => (
        let $fst = &*($fst as *const $as_t);
    );
}

#[macro_export]
macro_rules! c_methods {
    (
        use $fst:ident: $fst_t:ty as $as_t:ty;

        $(
            $fn_mod:tt fn $func:tt (
                $($type:tt $var:ident: $var_t:ty),*
            ) -> $ret:ty $action:block : $default:expr
        )+
    ) => (
        $(
            #[no_mangle]
            pub unsafe extern "C" fn $func($fst: *$fn_mod $fst_t, $( $var: $var_t ),*) -> $ret {
                if $fst.is_null() {
                    return $default;
                }

                $( c_expand!($type $var ? $default); )*

                expand_object!($fn_mod $fst as $as_t);

                $action
            }
        )+
    );
}
