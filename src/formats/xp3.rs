use std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use std::io::{Cursor, SeekFrom};
use std::io::prelude::*;
use std::path::{PathBuf};

use flate2::write::{ZlibDecoder};


use super::{Converter};
use file_utils;
use stream::{ReadStream, UTF16LE};

type InMemoryStream = ReadStream<Cursor<Vec<u8>>>;

//Notes taken from kirikiri XP3Archive.cpp
/*
XP3 header mark contains:
1. line feed and carriage return to detect corruption by unnecessary line-feeds convertion
2. 1A EOF mark which indicates file's text readable header ending.
3. 8B67 KANJI-CODE to detect curruption by unnecessary code convertion
4. 01 file structure version and character coding
   higher 4 bits are file structure version, currently 0.
   lower 4 bits are character coding, currently 1, is BMP 16bit Unicode.
*/

// FindChunk(data, name, start, size) (called from the loop inside the loop below)
// Data is the haystack and name is the needle
    // While we haven't read size bytes
        // Look for name in 4 bytes of data
        // Read size from u64 LE
        // Throw if size is larger than 32 bits
        // If the first name lookup worked, set the in size to size and return true
        // Increment the read bytes by the u64 size + 12
        // Start reading the next chunk at end of the u64 size
    // If name can't be found reset start and size the their original state
    // return false

//------------Original Algorithm to find all of the entries------------
// loop
    // Read u64 LE offset
    // Set the stream to offset + start of archive offset (This can overflow)
    // Read u8 flag
    // If flag has zlib encoding (0x07 & flag) == 1
        // Read u64 LE compressed size
        // Read u64 LE index size
            // If compressed size or index size are bigger than 32 bits(?) then throw
        // Create a u8 array of size index size
        // Create a u8 array of size compressed size
        // Read in the compressed array from the file
        // Uncompress the compressed array into the index array
    // If flag indicates raw encoding (0x07 & flag) == 0
        // Read u64 LE index size
            // If index size is bigger than 32 bits(?) then throw
        // Create a u8 array of size index size
        // Read data from the file into index array
    // Else throw
    // Set the start to 0 and the size to the index size
    // loop
        // Break if you can't find file chunk with the current start and size
        // Save the start and size found with the file chunk
        // Use the file start and size to find the info sub-chunk
            // Throw if info chunk can't be found
        // Start an Item struct
        // Read u32 LE flags from the start of the info chunk
        // Throw if the flags are set to protected and we aren't allowed to read protected
            // protected flag is 1 << 31 (0x80000000)
        // Read u64 LE original archive size into Item
        // Read u64 LE in-archive size into Item
        // Read i16 LE name length
        // Read UTF16 string of name length
        // Set the Item's name
        // Normalizes the storage to create a URL?
        // Uses the file start and size to find the segment sub-chunk
            // Throw if segm sub-chunk can't be found
        // Get the segment count from the segm size / 28
        // Set offset_in_archive to 0
        // Loop segment count times
            // Set the reading base to i * 28 + segm start
            // Create segment struct (segm)
            // Read u32 LE flags
            // Set segm.IsCompressed if flags say zlib compressed (0x07 & flags) == 1
            // Throw if the encoding bit mask doesn't return 1 or 0
            // Set segm.Start with (u64 LE read) + (offset of the entire archive)
            // Set segm.Offset (offset in uncompressed storage) to offset_in_archive
            // Set segm.OrgSize (original (uncompressed) size) with u64 LE read
            // Set segm.ArcSize (archived (compressed) size) with u64 LE read
            // Add the segment to Item.Segments vector
            // Increment offset_in_archive by segm.OrgSize
        // Use the file start and size to find the adlr sub-chunk
            // Throw if it can't be found
        // Set Item.FileHash with u32 LE read from adlr start
        // Add the current Item to a vector of them
        // Increment the file start by the file size
        // Set the file size to the remaining index size (index size - new file start)
    // Check the first flag for continuation (flag & 0x80) == 0 to stop
// Sort all of the items


//If 1, uses zLib compression, if 0 then raw, error if anything else
const ENCODING_MASK: u8 = 0x07;

//The mask for the index flag to keep reading entries
const CONTINUE_MASK: u8 = 0x80;

//The mask to check if an index is protected
const PROTECTED_MASK: u32 = 1 << 31;

//Magic: XP3\r\n \x1a\x8b\x67\x01
const HEADER: &'static [u8] = &[ 0x58, 0x50, 0x33, 0x0d, 0x0a, 0x20, 0x0a, 0x1a, 0x8b, 0x67, 0x01 ];

pub struct XP3Archive {

}

impl Converter for XP3Archive {
    fn is_correct_format<R: Read + Seek>(stream: &mut ReadStream<R>) -> bool {
        find_start_offset(stream).is_some()
    }

    fn new() -> XP3Archive {
        XP3Archive {
            
        }
    }

    fn flare<R: Read + Seek>(&mut self, mut stream: ReadStream<R>, save_folder: &PathBuf) {
        // If this is called, the stream is the correct format
        let start_offset = find_start_offset(&mut stream).unwrap();

        let mut items: Vec<ArchiveItem> = Vec::new();

        loop {
            let (mut entry_data, entry_flag) = find_entry_data(&mut stream, start_offset);

            //Keep going while we are finding file chunks
            while let Some(Chunk::File(ref mut file_data)) = find_chunk(&mut entry_data) {
                items.push(ArchiveItem::new(file_data, start_offset));
            }

            if entry_flag & CONTINUE_MASK == 0 {
                break;
            }
        }

        items.sort();

        //Write all of the files from the items
        for item in items {
            let mut path = save_folder.clone();
            path.push(item.name);

            let mut file = file_utils::make_file(&path);
            for segment in item.segments {
                stream.seek(SeekFrom::Start(segment.start)).unwrap();
                let buffer = if segment.compressed {
                    let compressed = stream.read_exact(segment.archive_size as usize).unwrap();

                    let mut decompressor = ZlibDecoder::new(Vec::new());
                    decompressor.write_all(&compressed).unwrap();
                    decompressor.finish().unwrap()
                } else {
                    stream.read_exact(segment.original_size as usize).unwrap()
                };

                //Unencrypt the buffer
                // for byte in buffer.iter_mut() {
                //     *byte ^= i.file_hash as u8;
                // }
                // Seek to the offset specified in the file before we start writing
                file.seek(SeekFrom::Start(segment.offset)).unwrap();
                file.write_all(&buffer).unwrap();
            }
        }
    }
}

///Finds the start of the XP3 Archive and returns the offset
///An XP3 archive can be after a Win32 exe container in the same file
fn find_start_offset<R: Read + Seek>(stream: &mut ReadStream<R>) -> Option<u64> {
    // Make sure that the stream is set correctly
    stream.little_endian(true);
    stream.seek(SeekFrom::Start(0)).unwrap();

    //Try to read the header right from the start
    let mut header_buffer = match stream.read_exact(11) {
        Ok(x) => x,
        Err(_) => return None,
    };

    //See if the file is an XP3 file
    //Also see if it's a WIN32 exe file because an XP3 payload may be hidden within;
    // starts with "MZ"
    //The header must start on a 16 byte boundary
    if header_buffer[0] == 0x4d && header_buffer[1] == 0x5a {
        //Seek to an even 16 byte boundary
        stream.seek(SeekFrom::Current(5)).unwrap();
        let mut offset = 16_u64;
        while let Ok(_) = stream.read_into(&mut header_buffer) {
            if header_buffer == HEADER {
                return Some(offset);
            }
            offset += 16;
            stream.seek(SeekFrom::Current(5)).unwrap();
        }

        //If we got this far, it means we went through the entire file and couldn't find the header
        return None;
    } else if header_buffer != HEADER {
        return None;
    }

    Some(0)
}

///Finds and returns the next entry data with the associated entry flag
///This function assumes that the stream is at the start of an entry
fn find_entry_data<R: Read + Seek>(stream: &mut ReadStream<R>, start_offset: u64)
-> (InMemoryStream, u8) {
    //The entry offset may be required to overflow if the header is not at the beginning of a file
    let entry_offset = stream.read::<u64>().unwrap().wrapping_add(start_offset);
    stream.seek(SeekFrom::Start(entry_offset)).unwrap();

    let entry_flag = stream.read::<u8>().unwrap();
    let entry_data = if entry_flag & ENCODING_MASK == 1 {
        let enc_size = stream.read::<u64>().unwrap();
        let real_size = stream.read::<u64>().unwrap();

        let compressed = stream.read_exact(enc_size as usize).unwrap();

        let mut decompressor = ZlibDecoder::new(Vec::new());
        decompressor.write_all(&compressed).unwrap();
        let entry_data = decompressor.finish().unwrap();

        debug_assert_eq!(entry_data.len(), real_size as usize);
        entry_data
    } else if entry_flag & ENCODING_MASK == 0 {
        let index_size = stream.read::<u64>().unwrap();
        stream.read_exact(index_size as usize).unwrap()
    } else {
        panic!("Bad flag in entry at 0x{:x}", entry_offset);
    };

    (ReadStream::new(Cursor::new(entry_data), true), entry_flag)
}

///Returns the next chunk type with the stream positioned to start reading its data
fn find_chunk<R: Read + Seek>(stream: &mut ReadStream<R>) -> Option<Chunk> {
    //Read the name of the chunk
    let name = match stream.read_exact(4) {
        Ok(x) => x,
        Err(_) => return None,
    };
    let real_size = match stream.read::<u64>() {
        Ok(x) => x,
        Err(_) => return None,
    };
    //The old, original algorithm gives up if real_size uses more than 32 bits

    Chunk::guess(stream, [name[0], name[1], name[2], name[3]], real_size)
}

#[derive(Debug)]
struct ArchiveItem {
    name: String,
    file_hash: u32,
    original_size: u64,
    archive_size: u64,
    segments: Vec<ArchiveSegment>,
}

impl ArchiveItem {
    fn new<R: Read + Seek>(file_data: &mut ReadStream<R>, start_offset: u64) -> ArchiveItem {
        let mut item = ArchiveItem {
            name: String::new(),
            file_hash: 0,
            original_size: 0,
            archive_size: 0,
            segments: Vec::new(),
        };
        
        //We have a max of 3 sub-chunks that can be found
        for _ in 0..3 {
            if let Some(mut chunk) = find_chunk(file_data) {
                match chunk {
                    Chunk::Info(ref mut info_data) => {
                        item.read_info(info_data);
                    },
                    Chunk::Segment(ref mut segm_data) => {
                        item.segments.append(
                            &mut ArchiveSegment::find_all(segm_data, start_offset)
                        );

                        //Sort all of the segments so that the offset of the first segment always starts at 0
                        item.segments.sort();
                    },
                    Chunk::Adlr(ref mut adlr_data) => {
                        item.file_hash = adlr_data.read::<u32>().unwrap();
                    },
                    Chunk::File(_) => panic!("A file chunk cannot be within another file chunk"),
                }
            } else {
                //Just break if we get None and move onto the next
                break;
            }    
        }

        item
    }

    fn read_info<R>(&mut self, info_data: &mut ReadStream<R>)
     where R: Read + Seek {
        let item_flags = info_data.read::<u32>().unwrap();
        if item_flags & PROTECTED_MASK == 1 {
            eprintln!("The current index is protected");
        }

        self.original_size = info_data.read::<u64>().unwrap();
        self.archive_size = info_data.read::<u64>().unwrap();

        //Read the UTF-16 name
        let utf16_len = info_data.read::<u16>().unwrap();
        self.name = info_data.read_with_len::<UTF16LE>(utf16_len as usize).unwrap();
        //We need to shorten the path name if it's longer than 255 bytes
        if self.name.len() > 255 {
            //Find all of the character boundaries
            let mut first_split_index = 0;
            let mut second_split_index = 0;
            let mut bounds: Vec<usize> = Vec::new();
            for i in 0..self.name.len() {
                if self.name.is_char_boundary(i) {
                    //We need to get the reference the last boundary index so that we have less
                    // than or equal to 126 characters in the first and second splits
                    if i > 126 && first_split_index == 0 {
                        first_split_index = bounds.len() - 1;
                    }
                    if i > (self.name.len() - 126) && second_split_index == 0 {
                        second_split_index = bounds.len() - 1;
                    }
                    bounds.push(i);
                }
            }

            //Create slices that go from the beginning up to the 126th byte, then the last 126 bytes
            // after adding an elipsis
            let mut new_name = String::from(&self.name[..bounds[first_split_index]]);
            new_name.push_str(&"...");
            new_name.push_str(&self.name[bounds[second_split_index]..]);

            self.name = new_name;
        }
    }
}

impl Ord for ArchiveItem {
    fn cmp(&self, other: &ArchiveItem) -> Ordering {
        // self.name.cmp(&other.name)
        self.segments[0].start.cmp(&other.segments[0].start)
    }
}

impl PartialOrd for ArchiveItem {
    fn partial_cmp(&self, other: &ArchiveItem) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ArchiveItem {
    fn eq(&self, other: &ArchiveItem) -> bool {
        self.name == other.name
    }
}

impl Eq for ArchiveItem {}

#[derive(Debug)]
struct ArchiveSegment {
    start: u64,
	offset: u64, //This is offset in the new file
	original_size: u64,
	archive_size: u64,
	compressed: bool,
}

impl ArchiveSegment {
    fn find_all<R>(segm_data: &mut ReadStream<R>, start_offset: u64) -> Vec<ArchiveSegment>
     where R: Read + Seek {
        let segm_size = segm_data.len();
        if segm_size % 28 != 0 {
            eprintln!("The segment isn't divisable by 28 bytes");
        }
        let count = segm_size / 28;
        let mut offset_in_archive: u64 = 0;
        (0..count).map(|i| {
            let flags = segm_data.read::<u32>().unwrap();

            // Since the mask is 0b111, other values besides 0 or 1 could possibly appear
            let compressed = if flags & (ENCODING_MASK as u32) == 1 {
                true
            } else if flags & (ENCODING_MASK as u32) == 0 {
                false
            } else {
                panic!("Bad flag in segment {}", i);
            };

            let start = segm_data.read::<u64>().unwrap() + start_offset;
            let offset = offset_in_archive;
            let original_size = segm_data.read::<u64>().unwrap();
            let archive_size = segm_data.read::<u64>().unwrap();

            offset_in_archive += original_size;

            ArchiveSegment {
                start,
                offset,
                original_size,
                archive_size,
                compressed,
            }
        }).collect()
    }
}

impl Ord for ArchiveSegment {
    fn cmp(&self, other: &ArchiveSegment) -> Ordering {
        self.offset.cmp(&other.offset)
    }
}

impl PartialOrd for ArchiveSegment {
    fn partial_cmp(&self, other: &ArchiveSegment) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for ArchiveSegment {
    fn eq(&self, other: &ArchiveSegment) -> bool {
        self.offset == other.offset
    }
}

impl Eq for ArchiveSegment {}

//Chunk Names
const FILE_CHUNK: [u8; 4] = [0x46, 0x69, 0x6c, 0x65]; //"File"
const INFO_CHUNK: [u8; 4] = [0x69, 0x6e, 0x66, 0x6f]; //"info"
const SEGM_CHUNK: [u8; 4] = [0x73, 0x65, 0x67, 0x6d]; //"segm"
const ADLR_CHUNK: [u8; 4] = [0x61, 0x64, 0x6c, 0x72]; //"adlr"

enum Chunk {
    File(InMemoryStream),
    Info(InMemoryStream),
    Segment(InMemoryStream),
    Adlr(InMemoryStream),
}

impl Chunk {
    //Tries to guess the type of the chunk
    fn guess<R>(stream: &mut ReadStream<R>, name: [u8; 4], size: u64) -> Option<Chunk>
    where R: Read + Seek {
        //Try to create the stream first as the order for this doesn't matter and doing this first
        // makes the code cleaner
        //If either the name checking or the stream fails, we need to return None, anyway
        if let Some(stream) = create_stream(stream, size) {
            match name {
                FILE_CHUNK => Some(Chunk::File(stream)),
                INFO_CHUNK => Some(Chunk::Info(stream)),
                SEGM_CHUNK => Some(Chunk::Segment(stream)),
                ADLR_CHUNK => Some(Chunk::Adlr(stream)),
                _ => None,
            }
        } else {
            None
        }
    }
}

///Creates a stream from the given stream from the next size bytes
fn create_stream<R>(stream: &mut ReadStream<R>, size: u64) -> Option<InMemoryStream>
where R: Read + Seek {
    let buffer = match stream.read_exact(size as usize) {
        Ok(x) => x,
        Err(_) => return None,
    };

    Some(ReadStream::new(Cursor::new(buffer), true))
}
