// Fetch a given GenBank accession.

use fasta_tools::load_genbank_accession_as_fasta_bytes;
use std::env;
use std::fs::File;
use std::io::Write;

fn main() {
    let args: Vec<String> = env::args().collect();
    let acc = &args[1];
    let mut bytes = Vec::<u8>::new();
    load_genbank_accession_as_fasta_bytes(&acc, &mut bytes);
    let mut file = File::create(&acc).unwrap();
    file.write_all(&bytes).unwrap();
}
