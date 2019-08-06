mod decrypt;
mod lvm;
mod moved;
mod new;
mod removed;
mod reuse;
mod table;

use self::{decrypt::*, lvm::*, moved::*, new::*, removed::*, reuse::*, table::*};

use super::*;
use errors::DistinstError;

pub(crate) fn configure_disks(matches: &ArgMatches) -> Result<Disks, DistinstError> {
    let mut disks = Disks::default();

    {
        let disks = &mut disks;

        for block in matches.values_of("disk").unwrap() {
            eprintln!("distinst: adding {} to disks configuration", block);
            disks.add(Disk::from_name(block)?);
        }

        tables(disks, matches.values_of("table"))
            .and_then(|_| removed(disks, matches.values_of("delete")))
            .and_then(|_| moved(disks, matches.values_of("move")))
            .and_then(|_| reused(disks, matches.values_of("use")))
            .and_then(|_| new(disks, matches.values_of("new")))
            .and_then(|_| initialize_logical(disks))
            .and_then(|_| decrypt(disks, matches.values_of("decrypt")))
            .and_then(|_| {
                lvm(
                    disks,
                    matches.values_of("logical"),
                    matches.values_of("logical-modify"),
                    matches.values_of("logical-remove"),
                    matches.is_present("logical-remove-all"),
                )
            })?;

        eprintln!("distinst: disks configured");
    }

    Ok(disks)
}

fn initialize_logical(disks: &mut Disks) -> Result<(), DistinstError> {
    eprintln!("distinst: initializing LVM groups");
    disks.initialize_volume_groups().map_err(|why| DistinstError::InitializeVolumes { why })
}
