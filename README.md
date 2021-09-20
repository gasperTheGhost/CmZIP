# CmZIP archive utility

Working proof of concept for a new archive type for use with future versions of CmDock. This repository holds a utility for creating and manipulating with CmZ archives, as described below.

## Background

The Community has requested zip support for CmDock; however, this is, in my opinion, either inefficient or unnecessary.

Seeking in zip files is neither easy, nor efficient and would thus have noticeable performance penalties, unless the entire zip file were read into memory. CmDock was also designed with huge (multi-GB) files in mind which makes unzipping into memory impossible or at least impractical in these cases. That being said, unzipping to and subsequently reading from a disk is a viable option, however, it makes little sense to add this functionality directly to CmDock, since there are multiple CLI tools which can already get the job done and can be used in batch or shell scripts effortlessly.

Additionally, the "ZIP" Deflate algorithm is good at compressing text files, however, newer, and better algorithms exist which can just as easily be implemented, so it makes little sense to use ZIP, because it is more ubiquitous.

This repository holds a possible solution to the above problems and concerns. The CmZ archive is a quasi-new filetype, which would allow easier and more sensible implementation directly into CmDock (and the entire Curie Marie Docking Suite). This version uses the LZMA algorithm for compression, but it can easily be used with other compression methods, such as Deflate or PAQ.

## CmZ file format

CmZ archives are a concatenation of multiple LZMA-compressed MDL SD records, with a custom file footer.

### File footer

#### Archive index

The file footer contains a LZMA-compressed index (`Vec<u64>`) of the concatenated archives from the file body. The first index element specifies the ending of the file header if one should be created. Usually, this is set to `0`, as all important data is stored in the file footer.

The subsequent elements are the compressed sizes of the stored records. Thus, the size of the first record in the CmZ archive (index 0) is stored at index 1.

Offsets can then be calculated by adding the preceding archive sizes. 

Ex. (3rd record):
```rust
let index: Vec<u64> = [0, 1024, 1116, 1112, 1064];
let i: usize = 2; // Index of 3rd record

let offset: u64 = (&index[..=i]).iter().sum();
```

#### Last 8 bytes

The last 8 bytes always encode the size of the Archive index. This puts a limit to the size of the index and subsequently the archive, however, it is well above the maximum file size of most filesystems.

## CmZIP modes of operation

### Compression mode (zip)

Usage:

```
cmzip zip -i <INPUT> -o <OUTPUT> -l <LEVEL>
```

The utility accepts MDL SD files (.sd, .sdf, ...) as input and writes a .cmz file to specified output.

`-l --level` is an optional parameter (defaults to 6) that sets the compression strength of the utility. Values range from 0 to 9, with higher values yielding greater compression, but also taking longer. 

### Decompression mode (unzip)

Usage:

```
cmzip unzip -i <INPUT> -o <OUTPUT> -r <RE,CO,RD,S>
```
The utility accepts CmZIP files (.cmz) as input and writes MDL SD files as output. The files may have any or no extension, this choice is left to the user.

`-r --record` is an optional parameter that accepts comma separated ints and specifies which records (indexing starts at 0) should be extracted. The list is never sorted, so the records will be extracted in the specified order.


## Compatibility with 7-zip

As tested, 7-zip (as well as supposedly any LZMA compatible archive utility) is able to fully decompress the files made with CmZIP, however, an error pops up saying that the file is not a valid archive. This is, of course, because of the CmZ file footer.

Upon inspection of the decompressed file, we can see that the ending has some "corrupt" data, this is the uncompressed file footer, unable to be read by text editors, as it only contains a Rust `Vec<u64>` and little-endian representation of a 64-bit integer. The extra data can simply be removed, and the file used as before compression.
