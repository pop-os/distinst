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
