mod xp3;

use std::fs::{File};
use std::io::prelude::*;
use std::path::{PathBuf};


use self::xp3::{XP3Archive};
use stream::{ReadStream};

/// Specifies how something can convert one file format into another
trait Converter {
    /// Check the given stream to see if the format is correct
    /// 
    /// The given stream could be in an odd state, so it is a good idea to reset it's state first
    /// thing.
    fn is_correct_format<R: Read + Seek>(stream: &mut ReadStream<R>) -> bool;

    /// Gives a new initialized object of itself
    fn new() -> Self;

    /// The given stream will start at the beginning of the format
    /// It is assumed that if you are being called, the stream is the correct format.
    /// Save every flared file into the save_folder.
    fn flare<R: Read + Seek>(&mut self, stream: ReadStream<R>, save_folder: &PathBuf);
}

/// This should only be available from guessing a format
#[derive(Debug, Clone, Copy)]
pub enum Format {
    XP3Archive,
}

/// Tries to guess the format of the file. If successful, will return an index that can then be
/// used to flare the file. The roles are split like this because we shouldn't be doing any
/// major error handling here.
/// 
/// Gives a vector of file formats because some file formats can be hidden inside on another.
/// It will be empty if the file format is unsupported
pub fn guess_format(file: &PathBuf) -> Vec<Format> {
    let mut stream = ReadStream::new(File::open(file).unwrap(), true);
    // Feed the stream to all of our supported formats to check for a correct format
    [
        (Format::XP3Archive, XP3Archive::is_correct_format(&mut stream)),
    ].into_iter().filter_map(|&(format, is_correct_format)| {
        if is_correct_format {
            Some(format)
        } else {
            None
        }
    }).collect()
}

pub fn flare_file(file: &PathBuf, save_folder: &PathBuf, format: Format) {
    let stream = ReadStream::new(File::open(file).unwrap(), true);
    
    match format {
        Format::XP3Archive => XP3Archive::new(),
    }.flare(stream, save_folder);
}
