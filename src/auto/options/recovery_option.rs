use envfile::EnvFile;
use partition_identity::PartitionID;
use std::path::Path;

#[derive(Debug)]
pub struct RecoveryOption {
    pub efi_uuid:      Option<String>,
    pub hostname:      String,
    pub kbd_layout:    String,
    pub kbd_model:     Option<String>,
    pub kbd_variant:   Option<String>,
    pub language:      String,
    pub luks_uuid:     Option<String>,
    pub oem_mode:      bool,
    pub recovery_uuid: String,
    pub root_uuid:     String,
    pub mode:          Option<String>,
}

impl RecoveryOption {
    pub fn parse_efi_id(&self) -> Option<PartitionID> {
        self.efi_uuid.as_ref().map(|uuid| Self::parse_id(uuid.clone()))
    }

    pub fn parse_recovery_id(&self) -> PartitionID { Self::parse_id(self.recovery_uuid.clone()) }

    fn parse_id(id: String) -> PartitionID {
        if id.starts_with("PARTUUID=") {
            PartitionID::new_partuuid(id[9..].to_owned())
        } else {
            PartitionID::new_uuid(id)
        }
    }
}

const RECOVERY_CONF: &str = "/cdrom/recovery.conf";

pub(crate) fn detect_recovery() -> Option<RecoveryOption> {
    let recovery_path = Path::new(RECOVERY_CONF);
    if recovery_path.exists() {
        let env = match EnvFile::new(recovery_path) {
            Ok(env) => env,
            Err(why) => {
                warn!("unable to read recovery configuration: {}", why);
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
            mode:          env.get("MODE").map(|x| x.to_owned()),
        });
    }

    None
}
