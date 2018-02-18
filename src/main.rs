extern crate flate2;
extern crate time;
extern crate rayon;

mod file_utils;
mod formats;
pub mod stream;

use std::collections::{HashMap};
use std::env;
use std::fmt::{Write};
use std::fs::{File};
use std::io::{Write as IOWrite};
use std::path::{PathBuf};
use std::process::{self};

use rayon::prelude::*;

use time::{SteadyTime};


const OUT_DIR: &'static str = "out";

fn main() {
    //The first argument is the executable path, so we can skip that
    let mut flares: Vec<Flare> = env::args().skip(1).flat_map(|file| {
        let file_path = match PathBuf::from(&file).canonicalize() {
            Ok(path) => path,
            Err(_) => {
                println!("{} needs to be valid path", file);
                process::exit(-1);
            },
        };
        // We need to make sure that every file exists
        if !file_path.exists() {
            println!("{} must exist", file);
            process::exit(-1);
        }

        if file_path.is_dir() {
            // Create a Flare for each file inside the directory
            file_path.read_dir().unwrap().filter_map(|result| {
                match result {
                    Ok(dir_entry) => {
                        if dir_entry.file_type().unwrap().is_file() {
                            Some(dir_entry.path())
                        } else {
                            None
                        }
                    },
                    Err(err) => {
                        println!("Couldn't read a file in the directory {} due to {}", file_path.display(), err);
                        process::exit(-1);
                    }
                }
            }).map(|file| {
                Flare::new(make_save_path(&file, Some(&file_path)), file)
            }).collect()
        } else {
            vec![Flare::new(make_save_path(&file_path, None), file_path)]
        }
    }).collect();

    if flares.len() == 0 {
        println!("A file or folder needs to supplied");
        process::exit(-1);
    }

    // Keep track of the results of the flaring
    let mut results: HashMap<PathBuf, Vec<PathBuf>> = HashMap::new();
    
    while flares.len() > 0 {
        // Flare each of our files
        let mut new_results: HashMap<PathBuf, Vec<PathBuf>> = flares.into_par_iter()
            .filter_map(|mut flare| {
                flare.flare();
                let (to_convert, converted_files) = flare.results();
                if converted_files.is_empty() {
                    // We couldn't convert any files for this format
                    None
                } else {
                    Some((to_convert, converted_files))
                }
            }).collect();

        // Get new flares from the ones that we just did
        flares = new_results.values()
            .flat_map(|converted_files| { converted_files })
            // We will now have just a list of files that we can make into flares
            .map(|file| {
                // Create the flare in the nested flared base
                Flare::new(make_flared_save_path(file), file.clone())
            }).collect();

        results.extend(new_results.drain());
    }

    // Format the results into a String
    let mut results_string = String::new();
    for (base, flared_files) in results {
        write!(results_string, "In: {}\n", base.display()).unwrap();
        write!(results_string, "Out:\n").unwrap();
        
        for file in flared_files {
            write!(results_string, "    {}\n", file.display()).unwrap();
        }
        
        write!(results_string, "\n========\n").unwrap();
    }

    // Write out all of the results into a file
    let mut results_path = PathBuf::from(OUT_DIR);
    // Get rid of all of the colons so that it's a valid file name
    results_path.push(format!("{}", time::now().rfc822z()).replace(":", ""));
    results_path.set_extension("txt");

    let mut results_file = File::create(results_path)
        .expect("Failed to create the results file");
    results_file.write_all(results_string.as_bytes())
        .expect("Failed to write the results file");
}

/// Creates the save path from the given file name and a parent
/// The parent should be specified if a directory was given initially
fn make_save_path(file: &PathBuf, parent: Option<&PathBuf>) -> PathBuf {
    let mut save_path = PathBuf::from(OUT_DIR);
    if let Some(parent) = parent {
        save_path.push(file_utils::file_stem(parent));
    }
    save_path.push(make_flared_base(file));

    save_path
}

/// Similar to make_save_path() but we assume that the file is already saved in the out folder
/// Since the original only does a single layer of directories we need to be lower than that
fn make_flared_save_path(file: &PathBuf) -> PathBuf {
    let mut save_path = file.parent().unwrap().to_path_buf();
    save_path.push(make_flared_base(file));

    save_path
}

/// Creates the base folder name for a file that will be flared
fn make_flared_base(file: &PathBuf) -> String {
    format!("{}({})", file_utils::file_stem(file), file_utils::extension(file))
}


/// Holds all of the information needed to flare a file
#[derive(Debug, Clone)]
struct Flare {
    /// The folder to save all of the flared files
    save_folder: PathBuf,

    /// The file to perform a conversion on
    to_convert: PathBuf,

    /// The converted files. These are the resulting files from the flaring
    converted_files: Vec<PathBuf>,
}

impl Flare {
    fn new(save_folder: PathBuf, to_convert: PathBuf) -> Flare {
        Flare {
            save_folder,
            to_convert,
            converted_files: Vec::new(),
        }
    }

    fn flare(&mut self) {
        // Figure out if this is a supported file format
        let file_formats = formats::guess_format(&self.to_convert);
        if file_formats.is_empty() {
            return
        };

        let start_time = SteadyTime::now();
        for file_format in file_formats {
            // Actually flare the file for each format
            formats::flare_file(&self.to_convert, &self.save_folder, file_format);
        }

        // Read the save directory to figure out all of the files that were just flared
        self.converted_files = file_utils::all_files(&self.save_folder, true);

        let file_count = self.converted_files.len();
        let seconds = ((SteadyTime::now() - start_time).num_milliseconds() as f64) / 1000.0;
        println!("{} complete! {} files in {:.3} sec", self.to_convert.display(),
            file_count, seconds);
    }

    /// Returns the results of the flaring
    /// Gives the converted file first, with the resulting flared files second
    fn results(self) -> (PathBuf, Vec<PathBuf>) {
        (self.to_convert, self.converted_files)
    }
}
