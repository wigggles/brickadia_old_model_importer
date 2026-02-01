//! # Binary Reading Utilities
//!
//! This module provides utilities for reading binary data from files,
//! similar to the C# `BinaryFileReader/BinLib.cs`.
//!
//! ## Key functions
//!
//! - `read_struct` — Read a packed struct from a reader.
//! - `read_struct_array` — Read multiple packed structs.
//! - `read_bytes` — Read raw bytes.

use bytemuck::{Pod, Zeroable};
use std::io::{Read, Seek, SeekFrom};

use crate::BspResult;

// -----------------------------------------------------------------------------
// Binary reading functions
// -----------------------------------------------------------------------------

/// Read a single packed struct from a reader.
///
/// The struct must implement `Pod` and `Zeroable` (from `bytemuck`),
/// which ensures it is safe to transmute from raw bytes.
///
/// # Type Parameters
///
/// * `T` - The struct type to read. Must be `#[repr(C, packed)]` and derive `Pod, Zeroable`.
///
/// # Arguments
///
/// * `reader` - Any type implementing `Read`.
///
/// # Returns
///
/// The deserialized struct, or an IO error.
///
/// # Example
///
/// ```ignore
/// #[repr(C, packed)]
/// #[derive(Copy, Clone, Pod, Zeroable)]
/// struct MyHeader {
///     magic: [u8; 4],
///     version: i32,
/// }
///
/// let header: MyHeader = read_struct(&mut file)?;
/// ```
pub fn read_struct<T: Pod + Zeroable, R: Read>(reader: &mut R) -> BspResult<T> {
    // Allocate a zeroed instance of T
    let mut value = T::zeroed();

    // Get a mutable byte slice view of the struct
    let bytes = bytemuck::bytes_of_mut(&mut value);

    // Read exactly the right number of bytes
    reader.read_exact(bytes)?;

    Ok(value)
}

/// Read an array of packed structs from a reader.
///
/// # Arguments
///
/// * `reader` - Any type implementing `Read`.
/// * `count` - Number of structs to read.
///
/// # Returns
///
/// A `Vec<T>` containing the deserialized structs.
pub fn read_struct_array<T: Pod + Zeroable + Copy, R: Read>(
    reader: &mut R,
    count: usize,
) -> BspResult<Vec<T>> {
    let mut result = Vec::with_capacity(count);

    for _ in 0..count {
        result.push(read_struct(reader)?);
    }

    Ok(result)
}

/// Seek to a specific offset in a reader.
///
/// # Arguments
///
/// * `reader` - Any type implementing `Seek`.
/// * `offset` - Absolute byte offset from the start of the file.
pub fn seek_to<R: Seek>(reader: &mut R, offset: u64) -> BspResult<()> {
    reader.seek(SeekFrom::Start(offset))?;
    Ok(())
}

/// Get the current position in a reader.
///
/// # Arguments
///
/// * `reader` - Any type implementing `Seek`.
///
/// # Returns
///
/// The current byte offset from the start.
pub fn current_position<R: Seek>(reader: &mut R) -> BspResult<u64> {
    Ok(reader.stream_position()?)
}

/// Read raw bytes from a reader.
///
/// # Arguments
///
/// * `reader` - Any type implementing `Read`.
/// * `count` - Number of bytes to read.
///
/// # Returns
///
/// A `Vec<u8>` containing the raw bytes.
pub fn read_bytes<R: Read>(reader: &mut R, count: usize) -> BspResult<Vec<u8>> {
    let mut buffer = vec![0u8; count];
    reader.read_exact(&mut buffer)?;
    Ok(buffer)
}

/// Convert a null-terminated byte array to a String.
///
/// Stops at the first null byte or end of array.
///
/// # Arguments
///
/// * `bytes` - The byte slice to convert.
///
/// # Returns
///
/// A `String` containing the text up to the first null byte.
pub fn bytes_to_string(bytes: &[u8]) -> String {
    // Find the first null byte, or use the whole slice
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());

    // Convert to string, replacing invalid UTF-8 with replacement char
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_to_string() {
        let bytes = b"hello\0world";
        assert_eq!(bytes_to_string(bytes), "hello");

        let no_null = b"hello";
        assert_eq!(bytes_to_string(no_null), "hello");
    }
}
