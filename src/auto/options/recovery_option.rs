use std::path::Path;
use envfile::EnvFile;

#[derive(Debug)]
pub struct RecoveryOption {
    pub efi_uuid:      Option<String>,
    pub hostname:      String,
    pub kbd_layout:    String,
    pub kbd_model:     Option<String>,
    pub kbd_variant:   Option<String>,
    pub language:      String,
    pub oem_mode:      bool,
    pub recovery_uuid: String,
    pub root_uuid:     String,
    pub luks_uuid:     Option<String>,
}

const RECOVERY_CONF: &str = "/cdrom/recovery.conf";

pub(crate) fn detect_recovery() -> Option<RecoveryOption> {
    let recovery_path = Path::new(RECOVERY_CONF);
    if recovery_path.exists() {
        let env = match EnvFile::new(recovery_path) {
            Ok(env) => env,
            Err(why) => {
                warn!(
                    "libdistinst: unable to read recovery configuration: {}",
                    why
                );
                return None;
            }
        };

        return Some(RecoveryOption {
            hostname:      env.get("HOSTNAME")?.to_owned(),
            language:      env.get("LANG")?.to_owned(),
            kbd_layout:    env.get("KBD_LAYOUT")?.to_owned(),
            kbd_model:     env.get("KBD_MODEL").map(|x| x.to_owned()),
            kbd_variant:   env.get("KBD_VARIANT").map(|x| x.to_owned()),
            efi_uuid:      env.get("EFI_UUID").map(|x| x.to_owned()),
            recovery_uuid: env.get("RECOVERY_UUID")?.to_owned(),
            root_uuid:     env.get("ROOT_UUID")?.to_owned(),
            oem_mode:      env.get("OEM_MODE").map_or(false, |oem| oem == "1"),
            luks_uuid:     env.get("LUKS_UUID").map(|x| x.to_owned()),
        });
    }

    None
}
