use distinst::Sector;
use crate::get_str;
use libc;
use std::ptr;
use crate::to_cstr;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DistinstSector {
    flag:  DISTINST_SECTOR_KIND,
    value: u64,
}

#[repr(C)]
#[derive(Copy, Clone)]
#[allow(non_camel_case_types)]
pub enum DISTINST_SECTOR_KIND {
    START,
    END,
    UNIT,
    UNIT_FROM_END,
    MEGABYTE,
    MEGABYTE_FROM_END,
    PERCENT,
}

impl From<DistinstSector> for Sector {
    fn from(sector: DistinstSector) -> Sector {
        match sector.flag {
            DISTINST_SECTOR_KIND::START => Sector::Start,
            DISTINST_SECTOR_KIND::END => Sector::End,
            DISTINST_SECTOR_KIND::UNIT => Sector::Unit(sector.value as u64),
            DISTINST_SECTOR_KIND::UNIT_FROM_END => Sector::UnitFromEnd(sector.value as u64),
            DISTINST_SECTOR_KIND::MEGABYTE => Sector::Megabyte(sector.value as u64),
            DISTINST_SECTOR_KIND::MEGABYTE_FROM_END => Sector::MegabyteFromEnd(sector.value as u64),
            DISTINST_SECTOR_KIND::PERCENT => Sector::Percent(sector.value as u16),
        }
    }
}

impl From<Sector> for DistinstSector {
    fn from(sector: Sector) -> DistinstSector {
        match sector {
            Sector::Start => distinst_sector_start(),
            Sector::End => distinst_sector_end(),
            Sector::Unit(value) => distinst_sector_unit(value),
            Sector::UnitFromEnd(value) => distinst_sector_unit_from_end(value),
            Sector::Megabyte(value) => distinst_sector_megabyte(value),
            Sector::MegabyteFromEnd(value) => distinst_sector_megabyte_from_end(value),
            Sector::Percent(value) => distinst_sector_percent(value),
        }
    }
}

#[repr(C)]
pub struct DistinstSectorResult {
    tag:    u8,
    error:  *mut libc::c_char,
    sector: DistinstSector,
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_from_str(
    string: *const libc::c_char,
) -> DistinstSectorResult {
    // First convert the C string into a Rust string
    let string = match get_str(string) {
        Ok(string) => string,
        Err(why) => {
            return DistinstSectorResult {
                tag:    1,
                error:  to_cstr(format!("{}", why)),
                sector: distinst_sector_start(),
            };
        }
    };

    // Then attempt to get the corresponding sector value
    match string.parse::<Sector>().ok() {
        Some(sector) => DistinstSectorResult {
            tag:    0,
            error:  ptr::null_mut(),
            sector: DistinstSector::from(sector),
        },
        None => DistinstSectorResult {
            tag:    1,
            error:  to_cstr("sector_from_str: invalid input".into()),
            sector: distinst_sector_start(),
        },
    }
}

#[no_mangle]
pub extern "C" fn distinst_sector_start() -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::START, value: 0 }
}

#[no_mangle]
pub extern "C" fn distinst_sector_end() -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::START, value: 0 }
}

#[no_mangle]
pub extern "C" fn distinst_sector_unit(value: u64) -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::UNIT, value }
}

#[no_mangle]
pub extern "C" fn distinst_sector_unit_from_end(value: u64) -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::UNIT_FROM_END, value }
}

#[no_mangle]
pub extern "C" fn distinst_sector_megabyte(value: u64) -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::MEGABYTE, value }
}

#[no_mangle]
pub extern "C" fn distinst_sector_megabyte_from_end(value: u64) -> DistinstSector {
    DistinstSector { flag: DISTINST_SECTOR_KIND::MEGABYTE_FROM_END, value }
}

#[no_mangle]
pub extern "C" fn distinst_sector_percent(value: u16) -> DistinstSector {
    debug_assert!(value <= 100);
    DistinstSector { flag: DISTINST_SECTOR_KIND::PERCENT, value: u64::from(value) }
}
