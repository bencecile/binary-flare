extern crate flate2;
extern crate time;

mod file_utils;
mod stream;
mod xp3;

use std::collections::{HashMap};
use std::env;
use std::fs::{File};
use std::io::{BufReader};
use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use time::{SteadyTime};

use stream::{ReadStream};

//Creates an out folder that is unique
fn create_out_folder() -> PathBuf {
    let mut path = PathBuf::new();
    // path.push(format!("out{}{}", MAIN_SEPARATOR, time::now_utc().rfc3339()));
    path.push("out");

    path
}

//Creates the manifest for the flared files
fn create_manifest<P>(save_folder: &P, map: HashMap<&P, Vec<String>>)
 where P: AsRef<Path> {
    let out_folder = save_folder.as_ref().parent().unwrap();
    let name = save_folder.as_ref().components().last().unwrap();
}

//Flares a given file. Returns a list of files saved for each flare
fn flare_file<P>(path: &P, save_folder: &P) -> Vec<String>
 where P: AsRef<Path> {
    let mut file = ReadStream::new(File::open(path).unwrap(), true);
    let mut results: Vec<String> = Vec::new();

    //Go through all of the known Flares for the file
    results.append(&mut xp3::flare(&mut file, save_folder));

    results
}

fn main() {
    let mut files: Vec<PathBuf> = Vec::new();

    //The first argument is usually the executable path
    for file in env::args().skip(1) {
        let mut path = PathBuf::new();
        path.push(file);
        if path.is_dir() {
            files.append(&mut file_utils::all_files(&path, true));
        } else {
            files.push(path);
        }
    }
    if files.len() == 0 {
        panic!("A file or folder needs to supplied");
    }

    let out = create_out_folder();
    let save_folder = out.join(file_utils::find_common_directory(&files));

    //These are all of the files that have be flared from a path
    let mut flares: HashMap<&PathBuf, Vec<String>> = HashMap::new();

    let start = SteadyTime::now();

    //Create flares for each of the given files
    for (i, item) in files.iter().enumerate() {
        flares.insert(&item, flare_file(&item, &&file_utils::make_relative(item, &save_folder)));
        // print!("\rFlaring {}/{}", i + 1, files.len());
    }
    println!("Completed {} files in {} sec", files.len(), SteadyTime::now() - start);

    create_manifest(&save_folder, flares);
}
