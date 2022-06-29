use super::*;
use errors::DistinstError;

pub(crate) fn decrypt(disks: &mut Disks, decrypt: Option<Values>) -> Result<(), DistinstError> {
    eprintln!("distinst: decrypting luks partitions");
    if let Some(decrypt) = decrypt {
        for device in decrypt {
            let values: Vec<&str> = device.split(':').collect();
            if values.len() != 3 {
                return Err(DistinstError::DecryptArgs);
            }

            let (device, pv) = (Path::new(values[0]), values[1].into());

            let (mut pass, mut keydata) = (None, None);
            parse_key(&values[2], &mut pass, &mut keydata)?;

            disks
                .decrypt_partition(device, &mut LuksEncryption::new(pv, pass, keydata, FileSystem::Btrfs))
                .map_err(|why| DistinstError::DecryptFailed { why })?;
        }
    }

    Ok(())
}
