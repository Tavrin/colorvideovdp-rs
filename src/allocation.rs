//! Checked, fallible storage for prediction buffers.
use crate::{Error, Result};

pub(crate) fn bytes<T>(count: usize) -> Result<usize> {
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(Error::SizeOverflow)?;
    if bytes > isize::MAX as usize {
        return Err(Error::SizeOverflow);
    }
    Ok(bytes)
}

pub(crate) fn reserved<T>(count: usize) -> Result<Vec<T>> {
    bytes::<T>(count)?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(count)
        .map_err(|_| Error::AllocationFailed)?;
    Ok(values)
}

pub(crate) fn filled<T: Clone>(count: usize, value: T) -> Result<Vec<T>> {
    let mut values = reserved(count)?;
    values.resize(count, value);
    Ok(values)
}

pub(crate) fn collect<T>(values: impl ExactSizeIterator<Item = Result<T>>) -> Result<Vec<T>> {
    let mut output = reserved(values.len())?;
    for value in values {
        output.push(value?);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_byte_capacity_without_allocating() {
        assert_eq!(
            reserved::<f32>(isize::MAX as usize / 4 + 1).err(),
            Some(Error::SizeOverflow)
        );
        assert_eq!(bytes::<f32>(usize::MAX), Err(Error::SizeOverflow));
        // The wasm32 review reproduction needs 2 GiB for its map alone.
        assert_eq!(536_870_912usize.checked_mul(4), Some(2_147_483_648));
    }
}
