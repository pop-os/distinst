use super::*;
use errors::DistinstError;

pub(crate) fn tables(disks: &mut Disks, tables: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: configuring partition tables");
    if let Some(tables) = tables {
        for table in tables {
            let values: Vec<&str> = table.split(':').collect();
            if values.len() != 2 {
                return Err(DistinstError::TableArgs);
            }

            let disk = find_disk_mut(disks, values[0])?;
            match values[1] {
                "gpt" => disk.mklabel(PartitionTable::Gpt)?,
                "msdos" => disk.mklabel(PartitionTable::Msdos)?,
                _ => {
                    return Err(DistinstError::InvalidTable { table: values[1].into() });
                }
            }
        }
    }

    Ok(())
}
