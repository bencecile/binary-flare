# Binary Flare
This is a project to extract different binary file formats.
The extraction is called Flaring because the result can have more than 1 file, if for example,
the file format is an archive.

# Currently supported file formats
- XP3 Archive

# Usage
`binaryflare file_path [...file_path]`

# Arguments
|Argument|Use|
|-------:|:--|
|file_path|A path pointing to either a single file or a directory. If it's a directory, the entire directory's contents will be read. It won't be deeply recursive.
