use super::*;

#[derive(Debug, Fail)]
pub(crate) enum DistinstError {
    #[fail(display = "disk error: {}", why)]
    Disk { why: DiskError },
    #[fail(display = "failed to decrypt partition: {}", why)]
    DecryptFailed { why: DecryptionError },
    #[fail(display = "table argument requires two values")]
    TableArgs,
    #[fail(display = "'{}' is not a valid table. Must be either 'gpt' or 'msdos'.", table)]
    InvalidTable { table: String },
    #[fail(display = "partition type must be either 'primary' or 'logical'")]
    InvalidPartitionType,
    #[fail(display = "decryption argument requires four values")]
    DecryptArgs,
    #[fail(display = "disk at '{}' could not be found", disk)]
    DiskNotFound { disk: String },
    #[fail(display = "no block argument provided")]
    NoBlockArg,
    #[fail(display = "argument '{}' is not a number", arg)]
    ArgNaN { arg: String },
    #[fail(display = "partition '{}' was not found", partition)]
    PartitionNotFound { partition: i32 },
    #[fail(display = "four arguments must be supplied to the move operation")]
    MoveArgs,
    #[fail(display = "provided sector value, '{}', was invalid", value)]
    InvalidSectorValue { value: String },
    #[fail(display = "no physical volume was defined in file system field")]
    NoPhysicalVolume,
    #[fail(display = "no volume group was defined in file system field")]
    NoVolumeGroup,
    #[fail(display = "provided password was empty")]
    EmptyPassword,
    #[fail(display = "provided key value was empty")]
    EmptyKeyValue,
    #[fail(display = "invalid field: {}", field)]
    InvalidField { field: String },
    #[fail(display = "no logical device named '{}' found", group)]
    LogicalDeviceNotFound { group: String },
    #[fail(display = "'{}' was not found on '{}'", volume, group)]
    LogicalPartitionNotFound { group: String, volume: String },
    #[fail(display = "invalid number of arguments supplied to --logical-modify")]
    ModifyArgs,
    #[fail(display = "could not find volume group associated with '{}'", group)]
    NoVolumeGroupAssociated { group: String },
    #[fail(display = "invalid number of arguments supplied to --use")]
    ReusedArgs,
    #[fail(display = "invalid number of arguments supplied to --new")]
    NewArgs,
    #[fail(display = "invalid number of arguments supplied to --logical")]
    LogicalArgs,
    #[fail(display = "invalid number of arguments supplied to --logical-remove")]
    LogicalRemoveArgs,
    #[fail(display = "mount path must be specified with key")]
    NoMountPath,
    #[fail(display = "mount value is empty")]
    EmptyMount,
    #[fail(display = "unable to add partition to lvm device: {}", why)]
    LvmPartitionAdd { why: DiskError },
    #[fail(display = "unable to initialize volume groups: {}", why)]
    InitializeVolumes { why: DiskError },
}

impl From<DiskError> for DistinstError {
    fn from(why: DiskError) -> DistinstError { DistinstError::Disk { why } }
}
