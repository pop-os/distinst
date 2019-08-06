use super::*;
use errors::DistinstError;

pub(crate) fn moved(disks: &mut Disks, parts: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring moved partitions");
    if let Some(parts) = parts {
        for part in parts {
            let values: Vec<&str> = part.split(':').collect();
            if values.len() != 4 {
                return Err(DistinstError::MoveArgs);
            }

            let (block, partition, start, end) = (
                values[0],
                values[1]
                    .parse::<u32>()
                    .map(|x| x as i32)
                    .ok()
                    .ok_or_else(|| DistinstError::ArgNaN { arg: values[1].into() })?,
                match values[2] {
                    "none" => None,
                    value => Some(parse_sector(value)?),
                },
                match values[3] {
                    "none" => None,
                    value => Some(parse_sector(value)?),
                },
            );

            let disk = find_disk_mut(disks, block)?;
            if let Some(start) = start {
                let start = disk.get_sector(start);
                disk.move_partition(partition, start)?;
            }

            if let Some(end) = end {
                let end = disk.get_sector(end);
                disk.resize_partition(partition, end)?;
            }
        }
    }

    Ok(())
}
