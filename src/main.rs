extern crate clap;

use std::{
    process, 
    io::{
        Seek, SeekFrom,
        BufReader, BufWriter,
        prelude::*
    },
    convert::TryInto
};
use clap::{Arg, ArgMatches, App, SubCommand};
use xz2::read::{XzEncoder, XzDecoder};

// Main function only sets up clap then calls run()
fn main() {
    let matches = App::new("CmZIP")
        .version("1.0")
        .author("Gašper Tomšič <gasper.tomsic@covid.si>")
        .about("CmDock archive utility.\nMDL SD file records are encoded individually and concatenated into a file.\nCmZ archives also contain a file footer which allows for individual decompression and easier processing.")
        .subcommand(SubCommand::with_name("zip")
            .about("Compresses MDL SD file into CmZ archive using LZMA")
            .arg(Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("INPUT")
                .help("Sets the input SDF file to use")
                .required(true)
                .takes_value(true)
            )
            .arg(Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("OUTPUT")
                .help("Sets the archive filename and path to write")
                .required(true)
                .takes_value(true)
            )
            .arg(Arg::with_name("level")
                .short("l")
                .long("level")
                .value_name("LEVEL")
                .help("Sets the compression level (0 - 9)")
                .default_value("6")
                .takes_value(true)
            )
        )
        .subcommand(SubCommand::with_name("unzip")
            .about("DeCompresses CmZ archive into MDL SD file")
            .arg(Arg::with_name("input")
                .short("i")
                .long("input")
                .value_name("INPUT")
                .help("Sets the input CmZ file to use")
                .required(true)
                .takes_value(true)
            )
            .arg(Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("OUTPUT")
                .help("Sets the MDL SD filename and path to write")
                .required(true)
                .takes_value(true)
            )
            .arg(Arg::with_name("records")
                .short("r")
                .long("record")
                .value_name("RECORD")
                .help("Only extract specified records in specified order. Indexing starts at 0.")
                .use_delimiter(true)
                .required(false)
                .takes_value(true)
            )
        )
        .get_matches();
    
    if let Err(e) = run(matches) {
        println!("Application error: {}", e);
        process::exit(1);
    }
}

// run() starts appropriate subcommand
fn run(matches: ArgMatches) -> Result<(), String> {
    match matches.subcommand() {
        ("zip", Some(m)) => zip(m),
        ("unzip", Some(m)) => unzip(m),
        _ => {
            eprintln!("Operating mode not selected!");
            eprintln!("Use cmzip -h for reference on how to use the utility.");
            process::exit(0);
        },
    }
}

// Entrypoint for zip subcommand
fn zip(matches: &ArgMatches) -> Result<(), String> {
    // Setup variables from command line input
    let input_filename = matches.value_of("input").unwrap();
    let mut output_filename = matches.value_of("output").unwrap().to_string();
    let level = matches.value_of("level").unwrap().parse::<u32>().expect("Specified level is invalid!");
    if level > 9 {
        eprintln!("Specified level is invalid!");
        process::exit(1);
    }
    // Always end archive file names with .cmz
    if !output_filename.ends_with(".cmz") {
        output_filename = output_filename.to_string() + ".cmz";
    }

    // Initialize the input and output buffers
    let mut input = BufReader::new(std::fs::File::open(input_filename).expect("No such file!"));
    let mut output = BufWriter::new(create_file(&output_filename));
    
    // Create vectors used in compression sequence
    let mut vec_record: Vec<u8> = Vec::new(); // Holds data of each record, delimited with $$$$
    let mut vec_index: Vec<u64> = vec![0]; // Holds the sizes of each created record, used for calculating offsets at decompression
    let mut compressed_data: Vec<u8> = Vec::new(); // Cleared each loop, used for compression
    let mut buf: Vec<u8> = Vec::new(); // Cleared each loop, holds one line from file as bytes
    loop {  // Iterate over lines in file
        match input.read_until(b'\n', &mut buf) {
            Ok(0) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf).to_string();
                vec_record.append(&mut buf);
        
                if !line.contains("$$$$") {
                    continue; // Only finish loop when $$$$ is reached, this way we compress each record by itself
                }
            },
            Err(_) => eprintln!("Error reading input file!")
        };

        // Compress record with specified compression level
        compress(&vec_record, &mut compressed_data, level).expect("Error compressing data!");
        vec_index.push(compressed_data.len() as u64); // Update index
        output.write_all(&compressed_data).expect("Error writing to file!");
        output.flush().unwrap(); // Write to output file

        // Clear vectors, this data is not needed anymore
        compressed_data.clear();
        buf.clear();
        vec_record.clear();
    }
    let mut data: Vec<u8> = Vec::new();
    // Convert index vector into binary vector for compression
    for index in vec_index {
        data.append(&mut index.to_le_bytes().to_vec())
    }
    compress(&data, &mut compressed_data, 9).expect("Error compressing data!");
    
    // Calculate size of compressed index for easier extraction
    let size = (compressed_data.len() as u64).to_le_bytes();

    // Write file footer
    output.write_all(&compressed_data).expect("Error writing to file!");
    output.write_all(&size).expect("Error writing to file!");
    output.flush().unwrap();

    return Ok(());
}

// Entrypoint for unzip subcommand
fn unzip(matches: &ArgMatches) -> Result<(), String> {
    // Setup variables from command line input
    let input_filename = matches.value_of("input").unwrap();
    let output_filename = matches.value_of("output").unwrap().to_string();

    // Initialize the input and output buffers
    let input_file = std::fs::File::open(input_filename).expect("No such file!");
    let file_size = input_file.metadata().unwrap().len() as u64; // Store file size for calculating offsets
    let mut input = BufReader::new(input_file);
    let mut output = BufWriter::new(create_file(&output_filename));

    // Get index from file footer
    // First step: get the compressed index size from last 8 bytes in file footer
    let mut index_size_raw: [u8; 8] = [0; 8];
    input.seek(SeekFrom::Start(file_size-8)).expect("Unable to seek in file!");
    input.read_exact(&mut index_size_raw).expect("Unexpected EOF!");
    let index_size = u64::from_le_bytes(index_size_raw); // Convert raw bytes to u64

    // Second step: read the compressed data from file footer
    let mut index_compressed: Vec<u8> = vec![0u8; index_size as usize];
    input.seek(SeekFrom::Start(file_size-index_size-8)).expect("Unable to seek in file!");
    input.read_exact(&mut index_compressed).expect("Unexpected EOF!");
    
    // Third step: decompress index and store in Vec[u64]
    let mut index_decompressed: Vec<u8> = Vec::new();
    decompress(&index_compressed, &mut index_decompressed).expect("Decompression failed");
    let mut index: Vec<u64> = Vec::new();
    for byte in index_decompressed.chunks(8) {
        index.push(u64::from_le_bytes(byte.try_into().unwrap())); // Numbers in index are raw little endian bytes, convert them to u64
    }

    let t_records: Vec<usize>; // This vector stores record indices of records to be extracted, should --record be specified
    if matches.is_present("records") {
        t_records = matches.values_of("records").unwrap().map(|x| x.parse::<usize>().expect("Invalid record index!")).collect();
    } else { // Else just decompress everything. Last elt is ignored as it points to the beginning of file footer.
        t_records = (0..(index.len() - 1)).collect();
    }

    // Decompression loop
    for i in t_records {
        let offset: u64 = (&index[0..=i]).iter().sum(); // Calculate offset
        input.seek(SeekFrom::Start(offset)).expect("Unable to seek in file!");
        let mut buf: Vec<u8> = vec![0u8; (index[i + 1]) as usize]; // Stores compressed record. Must be exactly the size of compressed data!
        input.read_exact(&mut buf).expect("Unexpected EOF!");
        let mut decompressor = XzDecoder::new(&buf[..]); // Create decompress stream
        std::io::copy(&mut decompressor, &mut output).expect("Error writing to file!"); // Decompress directly to file
    }
    
    return Ok(());
}

fn compress(input_buffer: &Vec<u8>, output_buffer: &mut Vec<u8>, level: u32) -> Result<usize, std::io::Error> {
    let mut compressor = XzEncoder::new(&input_buffer[..], level);
    return compressor.read_to_end(output_buffer)
}

fn decompress(input_buffer: &Vec<u8>, output_buffer: &mut Vec<u8>) -> Result<usize, std::io::Error> {
    let mut decompressor = XzDecoder::new(&input_buffer[..]);
    return decompressor.read_to_end(output_buffer);
}

fn create_file(filename: &str) -> std::fs::File {
    let path = std::path::Path::new(&filename);
    if filename.contains("/") {
        let prefix = path.parent().unwrap();
        std::fs::create_dir_all(prefix).unwrap();
    }
    let display = path.display();

    let file = match std::fs::File::create(&path) {
        Err(why) => panic!("Couldn't create {}: {}", display, why),
        Ok(file) => file,
    };

    return file;
}