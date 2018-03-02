use libc;

use distinst::Sector;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DistinstSector {
    flag:  DISTINST_SECTOR_KIND,
    value: libc::uint64_t,
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

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_start() -> DistinstSector {
    DistinstSector {
        flag:  DISTINST_SECTOR_KIND::START,
        value: 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_end() -> DistinstSector {
    DistinstSector {
        flag:  DISTINST_SECTOR_KIND::START,
        value: 0,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_unit(value: libc::uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::UNIT,
        value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_unit_from_end(value: libc::uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::UNIT_FROM_END,
        value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_megabyte(value: libc::uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::MEGABYTE,
        value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_megabyte_from_end(
    value: libc::uint64_t,
) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::MEGABYTE_FROM_END,
        value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_percent(value: libc::uint16_t) -> DistinstSector {
    debug_assert!(value <= 100);
    DistinstSector {
        flag:  DISTINST_SECTOR_KIND::PERCENT,
        value: value as libc::uint64_t,
    }
}
