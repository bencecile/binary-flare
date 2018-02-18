use std::io::{Error, ErrorKind, Result as IOResult};
use std::io::prelude::*;
use std::ops::{Add, Shl};
use std::mem;


use super::ReadStream;

/// Implement this for any type that should be readable from a stream
/// 
/// The Out type is useful for when we have different internal representations for the same output
///  Different String encodings is one such example
pub trait Readable {
    type Out;
    fn read_from<R: Read + Seek>(stream: &mut ReadStream<R>) -> IOResult<Self::Out>;
}

/// This is for any Readables that need to have a size given to them, as they are an inherently
/// flexible data type. The len should be how many Readables you want
pub trait UnknownSizeReadable {
    type Out;
    fn with_len<R: Read + Seek>(stream: &mut ReadStream<R>, len: usize) -> IOResult<Self::Out>;
}

impl Readable for u8 {
    type Out = u8;
    fn read_from<R: Read + Seek>(stream: &mut ReadStream<R>) -> IOResult<u8> {
        let bytes = stream.read_exact(mem::size_of::<u8>())?;
        Ok(bytes[0])
    }
}
impl Readable for u16 {
    type Out = u16;
    fn read_from<R: Read + Seek>(stream: &mut ReadStream<R>) -> IOResult<u16> {
        Ok(reduce_to_int(stream.read_exact(mem::size_of::<u16>())?, stream.is_little_endian()))
    }
}
impl Readable for u32 {
    type Out = u32;
    fn read_from<R: Read + Seek>(stream: &mut ReadStream<R>) -> IOResult<u32> {
        Ok(reduce_to_int(stream.read_exact(mem::size_of::<u32>())?, stream.is_little_endian()))
    }
}
impl Readable for u64 {
    type Out = u64;
    fn read_from<R: Read + Seek>(stream: &mut ReadStream<R>) -> IOResult<u64> {
        Ok(reduce_to_int(stream.read_exact(mem::size_of::<u64>())?, stream.is_little_endian()))
    }
}

/// Reduces the given bytes to the integer value specified
/// This assumes that the given bytes vector is the exact correct size
fn reduce_to_int<I>(bytes: Vec<u8>, little_endian: bool) -> I
where I: Default + Add<I, Output = I> + Shl<usize, Output = I> + From<u8> {
    debug_assert_eq!(bytes.len(), mem::size_of::<I>());
    
    bytes.iter().enumerate().fold(I::default(), |sum, (i, &byte)| {
        let shift = if little_endian {
            i * 8
        } else {
            (bytes.len() - i - 1) * 8
        };

        let byte: I = byte.into();
        sum + (byte << shift)
    })
}

/// Use this type when you want to get a UTF16 LittleEndian string from a stream
pub struct UTF16LE;

impl UnknownSizeReadable for UTF16LE {
    type Out = String;
    fn with_len<R: Read + Seek>(stream: &mut ReadStream<R>, len: usize) -> IOResult<String> {
        let bytes = stream.read_exact(len * 2)?;

        let mut utf16 = vec![0_u16; len];
        for (i, code_point) in utf16.iter_mut().enumerate() {
            *code_point = (bytes[i * 2] as u16) + ((bytes[(i * 2) + 1] as u16) << 8);
        }

        match String::from_utf16(&*utf16) {
            Ok(string) => Ok(string),
            Err(err) => Err(Error::new(ErrorKind::Other, format!("{:?}", err))),
        }
    }
}
