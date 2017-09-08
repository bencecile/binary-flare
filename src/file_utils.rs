///These are all file utility functions
use std::fs::{DirBuilder, File};
use std::path::{Component, Path, PathBuf};

//Gets all of the files that are in a directory
pub fn all_files(path: &Path, recurse: bool) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = Vec::new();

    for i in path.read_dir().unwrap() {
        if let Ok(entry) = i {
            let path = entry.path();
            if path.is_dir() && recurse {
                files.append(&mut all_files(&*path, true));
            } else {
                files.push(path);
            }
        }
    }

    files
}

//Find the lowest directory that is commmon to all the paths
pub fn find_common_directory(files: &Vec<PathBuf>) -> PathBuf {
    if files.len() == 0 {
        panic!("Need a non-empty vector");
    }

    //Get the list of possible directories that you can use
    let mut dirs: Vec<Component> = files[0].components().collect();
    for i in files.iter().skip(1) {
        for (index, j) in i.components().enumerate() {
            if index >= dirs.len() {
                break;
            }
            if j != dirs[index] {
                dirs.remove(index);
            }
        }
    }

    match dirs.iter().last() {
        Some(path) => {
            let mut real_path = PathBuf::new();
            real_path.push(path.as_ref());
            real_path
        },
        None => PathBuf::new(),
    }
}

//Makes the given path (old) relative to the (new_base) directory
pub fn make_relative(old: &PathBuf, new_base: &PathBuf) -> PathBuf {
    let last = new_base.components().last().unwrap();
    let mut new_path = PathBuf::new();
    new_path.push(new_base);

    //Get rid of the parts of the old path that don't match the last
    new_path.push(old.components()
     .skip_while(|part| part != &last)
     //Since last == current_part we need to skip past current_part
     //If we don't, we will end up with a nested folder with a duplicate name as last
     .skip(1)
     .map(|part| part.as_os_str())
     .collect::<PathBuf>());

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
