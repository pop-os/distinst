use super::null_check;
use distinst::locale::{self, KeyboardLayout, KeyboardLayouts, KeyboardVariant};
use libc;
use std::ptr;

#[repr(C)]
pub struct DistinstKeyboardLayout;

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layout_get_name(
    keyboard_layout: *const DistinstKeyboardLayout,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(keyboard_layout).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let keyboard_layout = &*(keyboard_layout as *const KeyboardLayout);
    let name = keyboard_layout.get_name().as_bytes();
    *len = name.len() as libc::c_int;
    name.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layout_get_description(
    keyboard_layout: *const DistinstKeyboardLayout,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(keyboard_layout).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let keyboard_layout = &*(keyboard_layout as *const KeyboardLayout);
    let desc = keyboard_layout.get_description().as_bytes();
    *len = desc.len() as libc::c_int;
    desc.as_ptr()
}

#[repr(C)]
pub struct DistinstKeyboardVariant;

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_variant_get_name(
    keyboard_variant: *const DistinstKeyboardVariant,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(keyboard_variant).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let keyboard_variant = &*(keyboard_variant as *const KeyboardVariant);
    let name = keyboard_variant.get_name().as_bytes();
    *len = name.len() as libc::c_int;
    name.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_variant_get_description(
    keyboard_variant: *const DistinstKeyboardVariant,
    len: *mut libc::c_int,
) -> *const u8 {
    if null_check(keyboard_variant).or_else(|_| null_check(len)).is_err() {
        return ptr::null();
    }

    let keyboard_variant = &*(keyboard_variant as *const KeyboardVariant);
    let desc = keyboard_variant.get_description().as_bytes();
    *len = desc.len() as libc::c_int;
    desc.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layout_get_variants(
    keyboard_layout: *const DistinstKeyboardLayout,
    len: *mut libc::c_int,
) -> *mut *const DistinstKeyboardVariant {
    if null_check(keyboard_layout).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let layout = &mut *(keyboard_layout as *mut KeyboardLayout);

    let mut output: Vec<*const DistinstKeyboardVariant> = Vec::new();
    match layout.get_variants() {
        Some(variants) => {
            for variant in variants.iter() {
                output.push(variant as *const KeyboardVariant as *const DistinstKeyboardVariant);
            }

            *len = output.len() as libc::c_int;
            Box::into_raw(output.into_boxed_slice()) as *mut *const DistinstKeyboardVariant
        }
        None => {
            *len = 0;
            ptr::null_mut()
        }
    }
}

#[repr(C)]
pub struct DistinstKeyboardLayouts;

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layouts_new() -> *mut DistinstKeyboardLayouts {
    match locale::get_keyboard_layouts() {
        Ok(layout) => Box::into_raw(Box::new(layout)) as *mut DistinstKeyboardLayouts,
        Err(why) => {
            error!("distinst_keyboard_layouts_new: {}", why);
            ptr::null_mut()
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layouts_get_layouts(
    layouts: *mut DistinstKeyboardLayouts,
    len: *mut libc::c_int,
) -> *mut *mut DistinstKeyboardLayout {
    if null_check(layouts).or_else(|_| null_check(len)).is_err() {
        return ptr::null_mut();
    }

    let layouts = &mut *(layouts as *mut KeyboardLayouts);

    let mut output: Vec<*mut DistinstKeyboardLayout> = Vec::new();
    for layout in layouts.get_layouts_mut().iter_mut() {
        output.push(layout as *mut KeyboardLayout as *mut DistinstKeyboardLayout);
    }

    *len = output.len() as libc::c_int;
    Box::into_raw(output.into_boxed_slice()) as *mut *mut DistinstKeyboardLayout
}

#[no_mangle]
pub unsafe extern "C" fn distinst_keyboard_layouts_destroy(
    layouts: *mut DistinstKeyboardLayouts,
    len: libc::size_t,
) {
    if layouts.is_null() {
        error!("DistinstKeyboardLayouts was to be destroyed even though it was null");
    } else {
        Vec::from_raw_parts(layouts, len, len);
    }
}
