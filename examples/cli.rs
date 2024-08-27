use std::env;
use std::fs::File;
use std::io::{Cursor, Read, Write};

const USAGE: &str = "usage: cargo run --example cli [input_file] [output_file]";

fn main() {
    let infile_path = env::args().nth(1).expect(USAGE);
    let outfile_path = env::args().nth(2).expect(USAGE);

    println!("reading file: {}", infile_path);
    let mut infile = File::open(infile_path).unwrap();

    let mut infile_contents = Vec::new();
    infile.read_to_end(&mut infile_contents).unwrap();

    let mut outfile_contents = Vec::new();
    ogg2caf::convert(Cursor::new(infile_contents), &mut outfile_contents).unwrap();

    println!("writing file: {}", outfile_path);
    let mut outfile = File::create_new(outfile_path).unwrap();
    outfile.write_all(&outfile_contents).unwrap();

    println!("done!");
}
