use distinst::timezones::*;
use crate::gen_object_ptr;
use libc;
use std::ptr;

#[repr(C)]
pub struct DistinstTimezones;

#[no_mangle]
pub unsafe extern "C" fn distinst_timezones_new() -> *mut DistinstTimezones {
    match Timezones::new() {
        Ok(timezones) => gen_object_ptr(timezones) as *mut Timezones as *mut DistinstTimezones,
        Err(why) => {
            eprintln!("distinst: timezone error: {}", why);
            return ptr::null_mut();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_timezones_zones(
    tz: *const DistinstTimezones,
) -> *mut DistinstZones {
    if tz.is_null() {
        error!("distinst_timezones_zones: tz input was null");
        return ptr::null_mut();
    }
    let boxed: Box<dyn Iterator<Item = &Zone>> =
        Box::new((&*(tz as *const Timezones)).zones().into_iter());
    gen_object_ptr(boxed) as *mut DistinstZones
}

#[no_mangle]
pub unsafe extern "C" fn distinst_timezones_destroy(tz: *mut DistinstTimezones) {
    if !tz.is_null() {
        Box::from_raw(tz as *mut Timezones);
    } else {
        error!("distinst_timezones_destroy: tz input was null");
    }
}

#[repr(C)]
pub struct DistinstZones;

#[no_mangle]
pub unsafe extern "C" fn distinst_zones_next(tz: *mut DistinstZones) -> *const DistinstZone {
    let zones = &mut *(tz as *mut Box<dyn Iterator<Item = &Zone>>);
    zones.next().map_or_else(|| ptr::null(), |zone| zone as *const Zone as *const DistinstZone)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_zones_nth(
    tz: *mut DistinstZones,
    nth: libc::c_int,
) -> *const DistinstZone {
    let zones = &mut *(tz as *mut Box<dyn Iterator<Item = &Zone>>);
    zones
        .nth(nth as usize)
        .map_or_else(|| ptr::null(), |zone| zone as *const Zone as *const DistinstZone)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_zones_destroy(tz: *mut DistinstZones) {
    if !tz.is_null() {
        Box::from_raw(tz as *mut Box<dyn Iterator<Item = &Zone>>);
    } else {
        error!("distinst_zones_destroy: tz input was null");
    }
}

#[repr(C)]
pub struct DistinstZone;

#[no_mangle]
pub unsafe extern "C" fn distinst_zone_name(
    zone: *const DistinstZone,
    len: *mut libc::c_int,
) -> *const u8 {
    if zone.is_null() {
        error!("distinst_zone_name: zone input was null");
        return ptr::null();
    }

    let name = (&*(zone as *const Zone)).name().as_bytes();
    *len = name.len() as libc::c_int;
    name.as_ptr()
}

#[no_mangle]
pub unsafe extern "C" fn distinst_zone_regions(zone: *const DistinstZone) -> *mut DistinstRegions {
    if zone.is_null() {
        error!("distinst_zone_name: zone input was null");
        return ptr::null_mut();
    }

    let boxed: Box<dyn Iterator<Item = &Region>> =
        Box::new((&*(zone as *const Zone)).regions().into_iter());
    gen_object_ptr(boxed) as *mut DistinstRegions
}

#[repr(C)]
pub struct DistinstRegions;

#[no_mangle]
pub unsafe extern "C" fn distinst_regions_next(
    regions: *mut DistinstRegions,
) -> *const DistinstRegion {
    let regions = &mut *(regions as *mut Box<dyn Iterator<Item = &Region>>);
    regions
        .next()
        .map_or_else(|| ptr::null(), |region| region as *const Region as *const DistinstRegion)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_regions_nth(
    regions: *mut DistinstRegions,
    nth: libc::c_int,
) -> *const DistinstRegion {
    let regions = &mut *(regions as *mut Box<dyn Iterator<Item = &Region>>);
    regions
        .nth(nth as usize)
        .map_or_else(|| ptr::null(), |region| region as *const Region as *const DistinstRegion)
}

#[no_mangle]
pub unsafe extern "C" fn distinst_regions_destroy(tz: *mut DistinstRegions) {
    if !tz.is_null() {
        Box::from_raw(tz as *mut Box<dyn Iterator<Item = &Region>>);
    } else {
        error!("distinst_regions_destroy: tz input was null");
    }
}

#[repr(C)]
pub struct DistinstRegion;

#[no_mangle]
pub unsafe extern "C" fn distinst_region_name(
    region: *const DistinstRegion,
    len: *mut libc::c_int,
) -> *const u8 {
    if region.is_null() {
        error!("distinst_region_name: region input was null");
        return ptr::null();
    }

    let name = (&*(region as *const Region)).name().as_bytes();
    *len = name.len() as libc::c_int;
    name.as_ptr()
}
