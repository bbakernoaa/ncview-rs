//! Random-access backing source used by the remote HDF5 reader.

use oxih5_core::OxiH5Error;

/// A bounded random-access byte source.
///
/// Implementations must return exactly `length` bytes, or an error. The
/// offset and length are absolute file coordinates; callers use this trait
/// for HDF5 metadata, indexes, and compressed payload chunks.
pub trait ByteSource: Send + Sync + std::fmt::Debug {
    /// Total logical length of the source object.
    fn len(&self) -> u64;

    /// Read exactly `length` bytes beginning at `offset`.
    fn read(&self, offset: u64, length: usize) -> Result<Vec<u8>, OxiH5Error>;
}

