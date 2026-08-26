use super::*;

#[derive(Debug, thiserror::Error)]
pub(crate) enum DistinstError {
    #[error("disk error: {}", why)]
    Disk { why: DiskError },
    #[error("failed to decrypt partition: {}", why)]
    DecryptFailed { why: DecryptionError },
    #[error("table argument requires two values")]
    TableArgs,
    #[error("'{}' is not a valid table. Must be either 'gpt' or 'msdos'.", table)]
    InvalidTable { table: String },
    #[error("partition type must be either 'primary' or 'logical'")]
    InvalidPartitionType,
    #[error("decryption argument requires four values")]
    DecryptArgs,
    #[error("disk at '{}' could not be found", disk)]
    DiskNotFound { disk: String },
    #[error("no block argument provided")]
    NoBlockArg,
    #[error("argument '{}' is not a number", arg)]
    ArgNaN { arg: String },
    #[error("partition '{}' was not found", partition)]
    PartitionNotFound { partition: i32 },
    #[error("four arguments must be supplied to the move operation")]
    MoveArgs,
    #[error("provided sector value, '{}', was invalid", value)]
    InvalidSectorValue { value: String },
    #[error("no physical volume was defined in file system field")]
    NoPhysicalVolume,
    #[error("no volume group was defined in file system field")]
    NoVolumeGroup,
    #[error("provided password was empty")]
    EmptyPassword,
    #[error("provided key value was empty")]
    EmptyKeyValue,
    #[error("invalid field: {}", field)]
    InvalidField { field: String },
    #[error("no logical device named '{}' found", group)]
    LogicalDeviceNotFound { group: String },
    #[error("'{}' was not found on '{}'", volume, group)]
    LogicalPartitionNotFound { group: String, volume: String },
    #[error("invalid number of arguments supplied to --logical-modify")]
    ModifyArgs,
    #[error("could not find volume group associated with '{}'", group)]
    NoVolumeGroupAssociated { group: String },
    #[error("invalid number of arguments supplied to --use")]
    ReusedArgs,
    #[error("invalid number of arguments supplied to --new")]
    NewArgs,
    #[error("invalid number of arguments supplied to --logical")]
    LogicalArgs,
    #[error("invalid number of arguments supplied to --logical-remove")]
    LogicalRemoveArgs,
    #[error("mount path must be specified with key")]
    NoMountPath,
    #[error("mount value is empty")]
    EmptyMount,
    #[error("unable to add partition to lvm device: {}", why)]
    LvmPartitionAdd { why: DiskError },
    #[error("unable to initialize volume groups: {}", why)]
    InitializeVolumes { why: DiskError },
}

impl From<DiskError> for DistinstError {
    fn from(why: DiskError) -> DistinstError { DistinstError::Disk { why } }
}
