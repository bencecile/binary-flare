use std::io::{BufReader, Error, ErrorKind, Read, Result as IOResult, Seek, SeekFrom};
use std::mem::{self};
use std::ops::{AddAssign, Shl};

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
    ///Uses read_exact() on the underlying stream
    pub fn read(&mut self, size: usize) -> IOResult<Vec<u8>> {
        let mut bytes: Vec<u8> = vec![0; size];
        self.stream.read_exact(&mut bytes)?;

        Ok(bytes)
    }

    ///Reads into a supplied buffer. Same as read() without creating a new buffer
    pub fn read_into(&mut self, buffer: &mut [u8]) -> IOResult<()> {
        self.stream.read_exact(buffer)
    }

    ///Reads an unsigned 8 bit integer from the stream using the current little endian option
    pub fn read_u8(&mut self) -> IOResult<u8> {
        let mut bytes = [0_u8; 1];
        self.stream.read_exact(&mut bytes)?;

        Ok(bytes[0])
    }

    ///Reads an unsigned 16 bit integer from the stream using the current little endian option
    pub fn read_u16(&mut self) -> IOResult<u16> {
        let mut bytes = [0_u8; 2];
        self.stream.read_exact(&mut bytes)?;

        let mut int = 0_u16;

        for (shift, byte) in self.shifts_for_int::<u16>().iter().zip(&bytes) {
            int += (*byte as u16) << shift;
        }

        Ok(int)
    }

    ///Reads an unsigned 32 bit integer from the stream using the current little endian option
    pub fn read_u32(&mut self) -> IOResult<u32> {
        let mut bytes = [0_u8; 4];
        self.stream.read_exact(&mut bytes)?;

        let mut int = 0_u32;

        for (shift, byte) in self.shifts_for_int::<u32>().iter().zip(&bytes) {
            int += (*byte as u32) << shift;
        }

        Ok(int)
    }

    ///Reads an unsigned 64 bit integer from the stream using the current little endian option
    pub fn read_u64(&mut self) -> IOResult<u64> {
        let mut bytes = [0_u8; 8];
        self.stream.read_exact(&mut bytes)?;

        let mut int = 0_u64;

        for (shift, byte) in self.shifts_for_int::<u64>().iter().zip(&bytes) {
            int += (*byte as u64) << shift;
        }

        Ok(int)
    }

    ///Finds the shifts required to convert an array of u8 into T
    fn shifts_for_int<T>(&self) -> Vec<usize> {
        let mut shifts: Vec<usize> = Vec::new();

        let size = mem::size_of::<T>();

        for i in 0..size {
            if self.little_endian {
                shifts.push(i * 8);
            } else {
                shifts.push((size - i - 1) * 8);
            }
        }

        shifts
    }

    ///Reads a UTF-16 String from the stream. Reads the bytes in LE order
    ///len is the number of UTF-16 code points to read (len * 2 number of bytes read)
    ///The UTF-16 read from the stream will return an Err if it is not perfectly formed
    pub fn read_utf16(&mut self, len: usize) -> IOResult<String> {
        let mut bytes = self.read(len * 2)?;

        let mut utf16 = vec![0_u16; len];
        for (i, code_point) in utf16.iter_mut().enumerate() {
            *code_point = (bytes[i * 2] as u16) + ((bytes[(i * 2) + 1] as u16) << 8);
        }

        match String::from_utf16(&*utf16) {
            Ok(x) => Ok(x),
            Err(x) => Err(Error::new(ErrorKind::Other, format!("{:?}", x))),
        }
    }
}

