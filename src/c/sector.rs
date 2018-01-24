use libc;

use Sector;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct DistinstSector {
    flag:  DISTINST_SECTOR_KIND,
    value: libc::uint64_t,
}

#[repr(C)]
#[derive(Copy, Clone)]
pub enum DISTINST_SECTOR_KIND {
    START = 1,
    END = 2,
    UNIT = 3,
    MEGABYTE = 4,
    PERCENT = 5,
}

impl From<DistinstSector> for Sector {
    fn from(sector: DistinstSector) -> Sector {
        match sector.flag {
            DISTINST_SECTOR_KIND::START => Sector::Start,
            DISTINST_SECTOR_KIND::END => Sector::End,
            DISTINST_SECTOR_KIND::UNIT => Sector::Unit(sector.value as u64),
            DISTINST_SECTOR_KIND::MEGABYTE => Sector::Megabyte(sector.value as u64),
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
        flag:  DISTINST_SECTOR_KIND::END,
        value: 0,
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
pub unsafe extern "C" fn distinst_sector_percent(value: libc::uint64_t) -> DistinstSector {
    DistinstSector {
        flag:  DISTINST_SECTOR_KIND::PERCENT,
        value: value,
    }
}

#[no_mangle]
pub unsafe extern "C" fn distinst_sector_unit(value: libc::uint64_t) -> DistinstSector {
    DistinstSector {
        flag: DISTINST_SECTOR_KIND::UNIT,
        value,
    }
}
