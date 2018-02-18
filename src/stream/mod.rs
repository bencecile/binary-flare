mod types;

pub use self::types::{
    Readable,
    UnknownSizeReadable,
    UTF16LE,
};

use std::io::{BufReader, Result as IOResult, SeekFrom};
use std::io::prelude::*;

///An adapter to a reader that can also seek
///This adapter provides high level reading methods from the underlying stream
///Uses a BufReader to wrap the given reader
pub struct ReadStream<R: Read + Seek> {
    stream: BufReader<R>,
    little_endian: bool,
}

impl <R: Read + Seek> ReadStream<R> {
    ///Creates a new ReadStream
    pub fn new(stream: R, little_endian: bool) -> Self {
        ReadStream {
            stream: BufReader::new(stream),
            little_endian,
        }
    }

    ///Changes the stream to read ints as little endian if new is true
    ///Uses big endian if false
    pub fn little_endian(&mut self, new: bool) {
        self.little_endian = new;
    }

    pub fn is_little_endian(&self) -> bool {
        self.little_endian
    }

    ///Seeks to the offset given. Same as the Seek trait
    pub fn seek(&mut self, pos: SeekFrom) -> IOResult<u64> {
        self.stream.seek(pos)
    }

    ///Gets the current position of the stream, from the start (ie. you can seek with
    ///SeekFrom::Start(pos())) to get back to the current position
    pub fn pos(&mut self) -> u64 {
        //Unwrapping is safe here because nothing can go wrong
        self.stream.seek(SeekFrom::Current(0)).unwrap()
    }

    ///Returns the length of the entire stream
    pub fn len(&mut self) -> u64 {
        //Unwrapping in this function is safe because we are seeking to very defined values
        //Save our current position
        let current = self.pos();

        //Seek to the end to get the size of the stream
        let len = self.stream.seek(SeekFrom::End(0)).unwrap();

        //Get back to our original position
        self.stream.seek(SeekFrom::Start(current)).unwrap();
        len
    }

    ///Will try to read the exact number of bytes as specified by size
    ///This will return an error if the exact number couldn't be read
    pub fn read_exact(&mut self, size: usize) -> IOResult<Vec<u8>> {
        let mut bytes: Vec<u8> = vec![0; size];
        self.stream.read_exact(&mut bytes)?;

        Ok(bytes)
    }

    pub fn read_into(&mut self, buffer: &mut [u8]) -> IOResult<()> {
        self.stream.read_exact(buffer)?;
        Ok(())
    }

    /// Reads the given Readable from the stream
    pub fn read<T: Readable>(&mut self) -> IOResult<T::Out> {
        T::read_from(self)
    }

    /// Reads the given Readable from the stream with a supplied length
    /// The length is how many Readables you want to get from the stream
    pub fn read_with_len<T: UnknownSizeReadable>(&mut self, len: usize) -> IOResult<T::Out> {
        T::with_len(self, len)
    }
}
