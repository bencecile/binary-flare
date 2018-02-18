///These are all file utility functions
use std::fs::{DirBuilder, File};
use std::path::{Path, PathBuf};
use std::process::{self};

//Gets all of the files that are in a directory
pub fn all_files(path: &Path, recurse: bool) -> Vec<PathBuf> {
    path.read_dir().unwrap().flat_map(|entry| {
        let path = entry.expect("Failed to read a directory").path();
        if path.is_dir() && recurse {
            all_files(&path, true)
        } else {
            vec![path]
        }
    }).collect()
}

//Makes the given path (old) relative to the (new_base) directory
pub fn make_relative(old: &PathBuf, new_base: &PathBuf) -> PathBuf {
    let last = new_base.components().last().unwrap();
    let mut new_path = new_base.clone();

    //Get rid of the parts of the old path that don't match the last
    new_path.push(old.components()
        .skip_while(|part| part != &last)
        //Since last == current_part we need to skip past current_part
        //If we don't, we will end up with a nested folder with a duplicate name as last
        .skip(1)
        .map(|part| part.as_os_str())
        .collect::<PathBuf>()
    );

    new_path
}

//Creates a file for writing given the path
pub fn make_file(path: &PathBuf) -> File {
    let parent = path.parent().unwrap();
    //Only create the parent directory if we need to
    if !parent.is_dir() {
        DirBuilder::new()
            .recursive(true)
            .create(parent)
            .unwrap();
    }

    File::create(path).unwrap()
}

/// Gets the file name of a file and converts it to a String
pub fn file_stem(file: &PathBuf) -> String {
    if let Some(name) = file.file_stem() {
        if let Some(string) = name.to_str() {
            return String::from(string);
        }
    }

    // The conversion failed or it didn't have a valid file name
    println!("{} must have a valid file name", file.display());
    process::exit(-2);
}

/// Gets the file extension of a file and converts it to a String
pub fn extension(file: &PathBuf) -> String {
    if let Some(ext) = file.extension() {
        if let Some(string) = ext.to_str() {
            return String::from(string);
        }
    }

    // Fallback for a file without an extension
    String::new()
}
