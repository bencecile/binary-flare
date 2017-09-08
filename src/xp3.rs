use std::cmp::{Eq, Ord, Ordering, PartialEq, PartialOrd};
use std::fs::{File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::mem;

use flate2::{Decompress, Flush};

use file_utils;

use stream::{ReadStream};

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

//Header: XP3\r\n \x1a\x8b\x67\x01
static HEADER: [u8; 11] = [
    0x58, 0x50, 0x33, 0x0d, 0x0a, 0x20, 0x0a, 0x1a,
    0x8b, 0x67, 0x01,
];

//Chunk Names
static FILE_CHUNK: [u8; 4] = [0x46, 0x69, 0x6c, 0x65]; //"File"
static INFO_CHUNK: [u8; 4] = [0x69, 0x6e, 0x66, 0x6f]; //"info"
static SEGM_CHUNK: [u8; 4] = [0x73, 0x65, 0x67, 0x6d]; //"segm"
static ADLR_CHUNK: [u8; 4] = [0x61, 0x64, 0x6c, 0x72]; //"adlr"

//If 1, uses zLib compression, if 0 then raw, error if anything else
const ENCODING_MASK: u8 = 0x07;

//The mask for the index flag to keep reading entries
const CONTINUE_MASK: u8 = 0x80;

//The mask to check if an index is protected
const PROTECTED_MASK: u32 = 1 << 31;

//Splits the given XP3 file if it's the correct format
//The vector will be empty if no files are written
pub fn flare<R, P>(stream: &mut ReadStream<R>, folder: &P) -> Vec<String>
 where R: Read + Seek, P: AsRef<Path> {
    stream.little_endian(true);
    stream.seek(SeekFrom::Start(0));

    //Return if we can't find the header
    let start_offset = match find_start_offset(stream) {
        Some(x) => x,
        None => return Vec::new(),
    };

    let mut items: Vec<ArchiveItem> = Vec::new();

    loop {
        let (mut entry_data, entry_flag) = find_entry_data(stream, start_offset);

        let mut file_start: usize = 0;
        let mut file_size: usize = entry_data.len() as usize;
        loop {
            if !find_chunk(&mut entry_data, FILE_CHUNK, &mut file_start, &mut file_size) {
                break;
            }

            let mut info_start = file_start;
            let mut info_size = file_size;
            if !find_chunk(&mut entry_data, INFO_CHUNK, &mut info_start, &mut info_size) {
                panic!("Couldn't find the info chunk after the File chunk at 0x{:x}", file_start);
            }

            let mut item = ArchiveItem {
                name: String::new(),
                file_hash: 0,
                original_size: 0,
                archive_size: 0,
                segments: Vec::new(),
            };
            let item_flags = entry_data.read_u32().unwrap();
            if item_flags & PROTECTED_MASK == 1 {
                eprintln!("The current index is protected at 0x{:x}", info_start);
            }

            item.original_size = entry_data.read_u64().unwrap();
            item.archive_size = entry_data.read_u64().unwrap();

            //Read the UTF-16 name
            let utf16_len = entry_data.read_u16().unwrap();
            item.name = entry_data.read_utf16(utf16_len as usize).unwrap();
            //We need to shorten the path name if it's longer than 255
            if item.name.len() > 255 {
                //Find all of the character boundaries
                let mut first_split_index = 0;
                let mut second_split_index = 0;
                let mut bounds: Vec<usize> = Vec::new();
                for i in 0..item.name.len() {
                    if item.name.is_char_boundary(i) {
                        //We need to get the reference the last boundary index so that we have less
                        // than or equal to 126 characters in the first and second splits
                        if i > 126 && first_split_index == 0 {
                            first_split_index = bounds.len() - 1;
                        }
                        if i > (item.name.len() - 126) && second_split_index == 0 {
                            second_split_index = bounds.len() - 1;
                        }
                        bounds.push(i);
                    }
                }

                //Split it at 126 from the start and 126 from the end so that we can put "..."
                // in the middle
                let mut new_name = String::from(&item.name[..bounds[first_split_index]]);
                new_name.push_str(&"...");
                new_name.push_str(&item.name[bounds[second_split_index]..]);

                item.name = new_name;
            }

            let mut segm_start = file_start;
            let mut segm_size = file_size;
            if !find_chunk(&mut entry_data, SEGM_CHUNK, &mut segm_start, &mut segm_size) {
                panic!("Couldn't find the SEGM chunk after the info chunk at 0x{:x}", info_start);
            }

            if segm_size % 28 != 0 {
                eprintln!("The segment isn't divisable by 28 bytes at 0x{:x}", info_start);
            }
            let count = segm_size / 28;
            let mut offset_in_archive: u64 = 0;
            for i in 0..count {
                let mut seg = ArchiveSegment {
                    start: 0,
                    offset: 0,
                    original_size: 0,
                    archive_size: 0,
                    compressed: true,
                };

                let flags = entry_data.read_u32().unwrap();

                if flags & (ENCODING_MASK as u32) == 1 {
                    seg.compressed = true;
                } else if flags & (ENCODING_MASK as u32) == 0 {
                    seg.compressed = false;
                } else {
                    panic!("Bad flag in segment {} at 0x{:x}", i, i * 28 + segm_start);
                }

                seg.start = entry_data.read_u64().unwrap() + start_offset;
                seg.offset = offset_in_archive;
                seg.original_size = entry_data.read_u64().unwrap();
                seg.archive_size = entry_data.read_u64().unwrap();

                offset_in_archive += seg.original_size;

                item.segments.push(seg);
            }

            //Sort all of the segments so that the offset of the first segment always starts at 0
            item.segments.sort();

            let mut adlr_start = file_start;
            let mut adlr_size = file_size;
            if !find_chunk(&mut entry_data, ADLR_CHUNK, &mut adlr_start, &mut adlr_size) {
                panic!("Couldn't find the ADLR chunk after the file start at 0x{:x}", file_start);
            }

            item.file_hash = entry_data.read_u32().unwrap();

            items.push(item);

            file_start += file_size;
            file_size = (entry_data.len() as usize) - file_start;
        }

        if entry_flag & CONTINUE_MASK == 0 {
			break;
        }
    }

    items.sort();
    println!("{}", items.len());
    for i in items.iter().take(1) {
        println!("{:?}", i);
    }

    let mut files: Vec<String> = Vec::new();
    //Write all of the files from the items
    for i in items {
        let mut path = PathBuf::new();
        path.push(folder);
        path.push(i.name);

        let mut file = file_utils::make_file(&path);
        for j in i.segments {
            let mut buffer: Vec<u8> = vec![0; j.original_size as usize];
            stream.seek(SeekFrom::Start(j.start));

            if j.compressed {
                let mut compressed = stream.read(j.archive_size as usize).unwrap();

                let mut decompressor = Decompress::new(true);
                decompressor.decompress(&*compressed, &mut *buffer, Flush::Finish);
            } else {
                stream.read_into(&mut buffer).unwrap();
            }

            //Unencrypt the buffer
            // for byte in buffer.iter_mut() {
            //     *byte ^= i.file_hash as u8;
            // }
            file.seek(SeekFrom::Start(j.offset));
            file.write(&buffer);
        }

        files.push(format!("XP3 Archive: {:?}", path));
    }

    files
}

///Finds the start of the XP3 Archive and returns the offset
///An XP3 archive can be after a Win32 exe container in the same file
fn find_start_offset<R>(stream: &mut ReadStream<R>) -> Option<u64>
 where R: Read + Seek {
    //Try to read the header right from the start
    let mut header_buffer = match stream.read(11) {
        Ok(x) => x,
        Err(_) => return None,
    };

    //See if the file is an XP3 file
    //Also see if it's a WIN32 exe file because an XP3 payload may be hidden within;
    // starts with "MZ"
    //The header must start on a 16 byte boundary
    if header_buffer[0] == 0x4d && header_buffer[1] == 0x5a {
        //Seek to an even 16 byte boundary
        stream.seek(SeekFrom::Current(5));
        let mut offset = 16_u64;
        while let Ok(_) = stream.read_into(&mut header_buffer) {
            if header_buffer == HEADER {
                return Some(offset);
            }
            offset += 16;
            stream.seek(SeekFrom::Current(5));
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
fn find_entry_data<R>(stream: &mut ReadStream<R>, start_offset: u64)
 -> (ReadStream<Cursor<Vec<u8>>>, u8)
 where R: Read + Seek {
    //The entry offset may be required to overflow if the header is not at the beginning of a file
    let entry_offset = stream.read_u64().unwrap().wrapping_add(start_offset);
    stream.seek(SeekFrom::Start(entry_offset));

    let mut entry_data: Vec<u8>;

    let entry_flag = stream.read_u8().unwrap();
    if entry_flag & ENCODING_MASK == 1 {
        let enc_size = stream.read_u64().unwrap();
        let real_size = stream.read_u64().unwrap();
        
        entry_data = vec![0; real_size as usize];
        let compressed = stream.read(enc_size as usize).unwrap();

        let mut decompressor = Decompress::new(true);
        decompressor.decompress(&*compressed, &mut *entry_data, Flush::Finish);
    } else if entry_flag & ENCODING_MASK == 0 {
        let index_size = stream.read_u64().unwrap();
        entry_data = stream.read(index_size as usize).unwrap();
    } else {
        panic!("Bad flag in entry at 0x{:x}", entry_offset);
    }

    (ReadStream::new(Cursor::new(entry_data), true), entry_flag)
}

fn find_chunk<R>(stream: &mut ReadStream<R>, name: [u8; 4], start: &mut usize, size: &mut usize) -> bool
 where R: Read + Seek {
    let start_save = *start;
	let size_save = *size;

	let mut pos: usize = 0;
	while pos < *size {
        let found = *stream.read(4).unwrap() == name;
		*start += 4;
        let real_size = stream.read_u64().unwrap();
		*start += 8;
		if (u32::max_value() as u64) < real_size {
			eprintln!("Chunk size is larger than 32 bits");
            return false;
        }
		if found {
			*size = real_size as usize;
			return true;
		}
		*start += real_size as usize;
		pos += (real_size as usize) + 4 + 8;
        stream.seek(SeekFrom::Start(*start as u64));
	}

	*start = start_save;
	*size = size_save;
	false
}

#[derive(Debug)]
struct ArchiveItem {
    name: String,
    file_hash: u32,
    original_size: u64,
    archive_size: u64,
    segments: Vec<ArchiveSegment>,
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

enum Chunk {
    File,
    Info,
    Segment,
    Adlr,
}
