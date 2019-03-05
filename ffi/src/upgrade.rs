use distinst::{self, Disks, UpgradeError, UpgradeEvent};
use distinst::auto::RecoveryOption;
use super::{DistinstDisks, DistinstRecoveryOption};
use libc;
use std::ptr;

#[repr(C)]
pub struct DistinstUpgradeEvent {
    tag: DISTINST_UPGRADE_TAG,
    message: *const libc::uint8_t,
    percent: libc::uint8_t
}

#[repr(C)]
pub enum DISTINST_UPGRADE_TAG {
    ATTEMPTING_REPAIR,
    ATTEMPTING_UPGRADE,
    DPKG_INFO,
    DPKG_ERR,
    UPGRADE_INFO,
    UPGRADE_ERR,
    PROGRESS,
    RESUMING_UPGRADE
}

impl From<UpgradeEvent<'_>> for DistinstUpgradeEvent {
    fn from(event: UpgradeEvent) -> Self {
        let mut c_event = DistinstUpgradeEvent {
            tag: DISTINST_UPGRADE_TAG::ATTEMPTING_REPAIR,
            message: ptr::null(),
            percent: 0
        };

        match event {
            UpgradeEvent::AttemptingRepair => (),
            UpgradeEvent::AttemptingUpgrade => {
                c_event.tag = DISTINST_UPGRADE_TAG::ATTEMPTING_UPGRADE;
            }
            UpgradeEvent::DpkgInfo(info) => {
                c_event.tag = DISTINST_UPGRADE_TAG::DPKG_INFO;
                c_event.message = info.as_bytes().as_ptr();
            }
            UpgradeEvent::DpkgErr(info) => {
                c_event.tag = DISTINST_UPGRADE_TAG::DPKG_ERR;
                c_event.message = info.as_bytes().as_ptr();
            }
            UpgradeEvent::UpgradeInfo(info) => {
                c_event.tag = DISTINST_UPGRADE_TAG::UPGRADE_INFO;
                c_event.message = info.as_bytes().as_ptr();
            }
            UpgradeEvent::UpgradeErr(info) => {
                c_event.tag = DISTINST_UPGRADE_TAG::UPGRADE_ERR;
                c_event.message = info.as_bytes().as_ptr();
            }
            UpgradeEvent::Progress(percent) => {
                c_event.tag = DISTINST_UPGRADE_TAG::PROGRESS;
                c_event.percent = percent;
            }
            UpgradeEvent::ResumingUpgrade => {
                c_event.tag = DISTINST_UPGRADE_TAG::RESUMING_UPGRADE;
            }
        }

        c_event
    }
}

pub type DistinstUpgradeEventCallback =
    extern "C" fn(event: DistinstUpgradeEvent, user_data: *mut libc::c_void);

pub type DistinstUpgradeRepairCallback =
    extern "C" fn(user_data: *mut libc::c_void) -> libc::uint8_t;

pub unsafe extern "C" fn distinst_upgrade(
    disks: *mut DistinstDisks,
    option: *const DistinstRecoveryOption,
    event_cb: DistinstUpgradeEventCallback,
    repair_cb: DistinstUpgradeRepairCallback,
    user_data: *mut libc::c_void,
) -> libc::c_int {
    let result = distinst::upgrade(
        &mut *(disks as *mut Disks),
        &*(option as *const RecoveryOption),
        move |event| event_cb(DistinstUpgradeEvent::from(event), user_data),
        move || repair_cb(user_data) != 0
    );

    match result {
        Ok(()) => 0,
        Err(why) => {
            error!("{}", why);
            -1
        }
    }
}
