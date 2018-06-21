use super::*;
use errors::DistinstError;

pub(crate) fn removed(disks: &mut Disks, ops: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring removed partitions");
    if let Some(ops) = ops {
        for op in ops {
            let mut args = op.split(':');
            let block_dev = match args.next() {
                Some(disk) => disk,
                None => {
                    return Err(DistinstError::NoBlockArg);
                }
            };

            for part in args {
                let part_id = match part.parse::<u32>() {
                    Ok(value) => value,
                    Err(_) => {
                        return Err(DistinstError::ArgNaN { arg: part.into() });
                    }
                };

                find_disk_mut(disks, block_dev)?.remove_partition(part_id as i32)?;
            }
        }
    }

    Ok(())
}
