// Copyright (c) 2018 10X Genomics, Inc. All rights reserved.

// Build reference sequence from Ensembl files.  This uses .gtf, .gff3, and .fa
// files.  The .gff3 file is used only to demangle gene names.  It is not possible
// to use the .gff3 (and not the .gff) to get all the gene info because it's not
// all present in the .gff3.
//
// This finds the coordinates of all TCR/BCR genes on the reference and then
// extracts the sequence from the reference.
// NO LONGER ACCURATE:
// In cases where a given gene is
// present on a chromosome record and also on one or more alt records, we pick
// the chromosome record.  Otherwise if it is present on more than one alt record,
// we pick the lexicographically minimal record name.  See "exceptions" below for
// special handling for particular genes.
//
// HOW TO USE THIS
//
// 1. Download files from Ensembl:
//    build_vdj_ref DOWNLOAD
//    You don't need to do this unless you're updating to a new Ensembl release.
//    This puts files in a directory ensembl.
//
// 2. Create reference files:
//    build_vdj_ref HUMAN
//    build_vdj_ref MOUSE
//    *** These must be run from the root of the repo! ***
//    You don't need to do this unless you're changing this code.
//
//    These files get ultimately moved to:
//    /mnt/opt/refdata_cellranger/vdj/
//        vdj_GRCh38_alts_ensembl-*.*.*
//        vdj_GRCm38_alts_ensembl-*.*.*
//    with the cellranger release version substituted in.  This will not work
//    immediately on pluto, and may depend on overnight auto-syncronization to
//    the pluto filesystem.
//
//    To make jenkins work, the human files also need to be copied to
//    /mnt/test/refdata/testing/vdj_GRCh38_alts_ensembl
//    by someone who has permission to do that.
//
//    Experimental (assuming the right files have been downloaded from ensembl):
//    build_vdj_ref BALBC
//    (won't work now because will try to write to a directory that doesn't exist)
//    However, this works poorly.  Many genes are missing.  Here are a few examples:
//
//    gene            in whole-genome   in BALB/c data
//                    BALB/c assembly   e.g. lena 77990
//
//    IGKV4-53        no                yes
//    IGHV12-1        no                yes
//    IGHV1-unknown1  no                yes.
//
// 3. For debugging:
//    build_vdj_ref NONE  [no fasta output].
//
// TODO
// ◼ Decide what the exon structure of C segments should be.
//
// ◼ Genes added by sequence appear as if in GRCh or GRCm, but this is wrong.
// ◼ Should look for GenBank accessions that have these.
//
// See also build_supp_ref.rs.
//
// Observed differences with IMGT for human TCR:
//
// 1. Output here includes 5' UTRs.
// 2. Our TRAV6 is 57 bases longer on the 5' end.  We see full length alignemnts
//    of our TRAV6 to transcripts so our TRAV6 appears to be correct.
// 3. We don't have the broken transcript TRBV20-1*01.
// 4. We exclude many pseudogenes.
//
// Observed differences with "GRCh" reference for human TCR:
//
// 1. Our C segments are correct.
// 2. Our L+V segments start with start codons.
// 3. Our TRBV11-2 and TRAJ37 are correct.
//
// For both: this code has the advantage of producing reproducible results from
// defined external files.

use debruijn::{
    dna_string::{DnaString, DnaStringSlice},
    Mer,
};
use fasta_tools::load_genbank_accession;
use flate2::read::MultiGzDecoder;
use perf_stats::elapsed;
use pretty_trace::PrettyTrace;
use process::Command;
use sha2::{Digest, Sha256};
use std::io::copy;
use std::io::Write;
use std::{
    assert, char,
    collections::HashMap,
    env, eprintln, format, fs,
    fs::File,
    i32,
    io::{BufRead, BufReader},
    println, process, str,
    time::Instant,
    usize, vec, write, writeln,
};
use string_utils::{cap1, TextUtils};
use vector_utils::{bin_member, bin_position1_2, erase_if, next_diff12_8, unique_sort};

use io_utils::{fwrite, fwriteln, open_for_read, open_for_write_new};

fn header_from_gene(
    gene: &str,
    is_5utr: bool,
    is_3utr: bool,
    record: &mut usize,
    source: &str,
) -> String {
    let mut gene = gene.to_string();
    if gene.ends_with(' ') {
        gene = gene.rev_before(" ").to_string();
    }
    let genev = gene.as_bytes();
    let mut xx = "None";
    if gene == "IGHD"
        || gene == "IGHE"
        || gene == "IGHM"
        || gene.starts_with("IGHG")
        || gene.starts_with("IGHA")
    {
        xx = gene.after("IGH");
    }
    let header_tail = format!(
        "{}{}|{}{}{}|{}|00",
        genev[0] as char,
        genev[1] as char,
        genev[0] as char,
        genev[1] as char,
        genev[2] as char,
        xx
    );
    *record += 1;
    let region_type: String;
    if is_5utr {
        region_type = "5'UTR".to_string();
    } else if is_3utr {
        region_type = "3'UTR".to_string();
    } else if gene == "IGHD"
        || gene == "IGHE"
        || gene == "IGHM"
        || gene.starts_with("IGHG")
        || gene.starts_with("IGHA")
    {
        region_type = "C-REGION".to_string();
    } else if genev[3] == b'V' {
        region_type = "L-REGION+V-REGION".to_string();
    } else {
        region_type = format!("{}-REGION", genev[3] as char);
    }
    format!(
        "{}|{} {}|{}|{}|{}",
        record, gene, source, gene, region_type, header_tail
    )
}

fn print_fasta<R: Write>(out: &mut R, header: &str, seq: &DnaStringSlice, none: bool) {
    if none {
        return;
    }
    fwriteln!(out, ">{}\n{}", header, seq.to_string());
}

fn print_oriented_fasta<R: Write>(
    out: &mut R,
    header: &str,
    seq: &DnaStringSlice,
    fw: bool,
    none: bool,
) {
    if none {
        return;
    }
    if fw {
        print_fasta(out, header, seq, none);
    } else {
        let seq_rc = seq.rc();
        print_fasta(out, header, &seq_rc, none);
    }
}

// add_gene: coordinates are one-based

fn add_gene<R: Write>(
    out: &mut R,
    gene: &str,
    record: &mut usize,
    chr: &str,
    start: usize,
    stop: usize,
    to_chr: &HashMap<String, usize>,
    refs: &[DnaString],
    none: bool,
    is_5utr: bool,
    is_3utr: bool,
    source: &str,
) {
    if none {
        return;
    }
    if !to_chr.contains_key(&chr.to_string()) {
        eprintln!("gene = {}, chr = {}", gene, chr);
    }
    let chrid = to_chr[chr];
    let seq = refs[chrid].slice(start - 1, stop);
    let header = header_from_gene(gene, is_5utr, is_3utr, record, source);
    print_fasta(out, &header, &seq.slice(0, seq.len()), none);
}

// two exon version

fn add_gene2<R: Write>(
    out: &mut R,
    gene: &str,
    record: &mut usize,
    chr: &str,
    start1: usize,
    stop1: usize,
    start2: usize,
    stop2: usize,
    to_chr: &HashMap<String, usize>,
    refs: &[DnaString],
    none: bool,
    fw: bool,
    source: &str,
) {
    if none {
        return;
    }
    let chrid = to_chr[chr];
    let seq1 = refs[chrid].slice(start1 - 1, stop1);
    let seq2 = refs[chrid].slice(start2 - 1, stop2);
    let mut seq = seq1.to_owned();
    for i in 0..seq2.len() {
        seq.push(seq2.get(i));
    }
    if !fw {
        seq = seq.rc();
    }
    let header = header_from_gene(gene, false, false, record, source);
    print_fasta(out, &header, &seq.slice(0, seq.len()), none);
}

type ExonSpec = (String, String, String, i32, i32, String, bool, String);

fn parse_gtf_file(gtf: &str, demangle: &HashMap<String, String>, exons: &mut Vec<ExonSpec>) {
    let f = open_for_read![&gtf];
    exons.clear();
    for line in f.lines() {
        let s = line.unwrap();

        let fields: Vec<&str> = s.split_terminator('\t').collect();
        if fields.len() < 9 {
            continue;
        }
        let fields8: Vec<&str> = fields[8].split_terminator(';').collect();
        if fields8.len() < 6 {
            continue;
        }

        // Get type of entry.  If it's called a pseudogene and the type is exon,
        // change it to CDS.

        let mut biotype = String::new();
        for i in 0..fields8.len() {
            if fields8[i].starts_with(" gene_biotype") {
                biotype = fields8[i].between("\"", "\"").to_string();
            }
        }
        let mut cat = fields[2];
        if biotype.contains("pseudogene") && cat == "exon" {
            cat = "CDS";
        }
        if !biotype.starts_with("TR_") && !biotype.starts_with("IG_") {
            continue;
        }

        // Exclude certain types.

        if cat == "gene" {
            continue;
        }
        if cat == "transcript" || cat == "exon" {
            continue;
        }
        if cat == "start_codon" || cat == "stop_codon" {
            continue;
        }
        if cat == "three_prime_utr" && biotype != "IG_C_gene" {
            continue;
        }

        // Get gene name and demangle.

        let mut gene = String::new();
        for i in 0..fields8.len() {
            if fields8[i].starts_with(" gene_name") {
                gene = fields8[i].between("\"", "\"").to_string();
            }
        }
        gene = gene.to_uppercase();
        if gene.starts_with("TCRG-C") {
            gene = format!("TRGC{}", gene.after("TCRG-C"));
        }
        if gene.starts_with("TCRG-V") {
            gene = format!("TRGV{}", gene.after("TCRG-V"));
        }
        let gene2 = demangle.get(&gene);
        if gene2.is_none() {
            continue;
        }
        let mut gene2 = gene2.unwrap().clone();

        // Special fixes.  Here the gff3 file is trying to impose a saner naming
        // scheme on certain genes, but we're sticking with the scheme that people
        // use.

        if gene2.starts_with("IGHCA") {
            gene2 = gene2.replace("IGHCA", "IGHA");
        }
        if gene2 == "IGHCD" {
            gene2 = "IGHD".to_string();
        }
        if gene2 == "IGHCE" {
            gene2 = "IGHE".to_string();
        }
        if gene2.starts_with("IGHCG") {
            gene2 = gene2.replace("IGHCG", "IGHG");
        }
        if gene2 == "IGHCM" {
            gene2 = "IGHM".to_string();
        }

        // For now, require havana (except for mouse strains).  Could try turning
        // this off, but there may be some issues.

        if !fields[1].contains("havana") && fields[1] != "mouse_genomes_project" {
            continue;
        }

        // Get transcript name.

        let mut tr = String::new();
        for i in 0..fields8.len() {
            if fields8[i].starts_with(" transcript_name") {
                tr = fields8[i].between("\"", "\"").to_string();
            }
        }

        // Get transcript id.

        let mut trid = String::new();
        for i in 0..fields8.len() {
            if fields8[i].starts_with(" transcript_id") {
                trid = fields8[i].between("\"", "\"").to_string();
            }
        }

        // Save in exons.

        let chr = fields[0];
        let start = fields[3].force_i32() - 1;
        let stop = fields[4].force_i32();
        let mut fw = false;
        if fields[6] == "+" {
            fw = true;
        }
        exons.push((
            gene2,
            tr,
            chr.to_string(),
            start,
            stop,
            cat.to_string(),
            fw,
            trid,
        ));
    }
    exons.sort();
}

fn main() {
    let t = Instant::now();

    // Force panic to yield a traceback, and make it a pretty one.

    PrettyTrace::new().on();

    // Parse arguments.

    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Please supply exactly one argument.");
        std::process::exit(1);
    }
    let mut none = false;
    let mut download = false;
    let species = match args[1].as_str() {
        "DOWNLOAD" => {
            download = true;
            ""
        }
        "HUMAN" => "human",
        "MOUSE" => "mouse",
        "BALBC" => "balbc",
        "NONE" => {
            none = true;
            "human"
        }
        _ => {
            eprintln!("Call with DOWNLOAD or HUMAN or MOUSE or NONE.");
            std::process::exit(1);
        }
    };

    // Get ensembl location.

    let mut ensembl_loc = String::new();
    for (key, value) in env::vars() {
        if key == "VDJ_ANN_REF_ENSEMBL" {
            ensembl_loc = value.clone();
        }
    }
    if ensembl_loc.len() == 0 {
        eprintln!(
            "\nTo use build_vdj_ref, you first need to set the environment variable \
            VDJ_ANN_REF_ENSEMBL\nto the path of your ensembl directory.\n"
        );
        std::process::exit(1);
    }

    // Define release.  If this is ever changed, the effect on the fasta output
    // files should be very carefully examined.  Specify sequence source.

    let release = 94;
    let version = "7.0.0";
    let (source, source2) = match species {
        "human" => (
            format!("GRCh38-release{}", release),
            "vdj_GRCh38_alts_ensembl".to_string(),
        ),
        "mouse" => (
            format!("GRCm38-release{}", release),
            "vdj_GRCm38_alts_ensembl".to_string(),
        ),
        _ => {
            let source = format!("BALB_cJ_v1.{}", release);
            (source.clone(), source)
        }
    };

    // Define local directory.

    let internal = &ensembl_loc;

    // Set up for exceptions.  Coordinates are the usual 1-based coordinates used in
    // genomics.  If the bool field ("fw") is false, the given coordinates are used
    // to extract a sequence, and then it is reversed.

    let excluded_genes = vec![];
    let mut allowed_pseudogenes = Vec::<&str>::new();
    let mut deleted_genes = Vec::<&str>::new();
    let mut added_genes = Vec::<(&str, &str, usize, usize, bool)>::new();
    let mut added_genes2 = Vec::<(&str, &str, usize, usize, usize, usize, bool)>::new();
    let mut added_genes2_source = Vec::<(&str, usize, usize, usize, usize, bool, String)>::new();
    let mut left_trims = Vec::<(&str, usize)>::new();
    let mut right_trims = Vec::<(&str, i32)>::new();
    let mut added_genes_seq = Vec::<(&str, &str, bool)>::new();
    let mut added_genes_seq3 = Vec::<(&str, &str, bool)>::new();

    // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

    // Define lengthenings of human BCR 3' UTRs, starting February 2023.  We do not have a good
    // mechanism for this and so just delete the gene and then add it back.

    if species == "human" {
        // 1
        deleted_genes.push("IGLC1");
        added_genes_seq3.push((
            "IGLC1",
            "GTCAGCCCAAGGCCAACCCCACTGTCACTCTGTTCCCGCCCTCCTCTGAGGAGCTCCAAGCCAACAAGGCCACACTAGTGTGTCTGATCAGTGACTTCTACCCGGGAGCTGTGACAGTGGCCTGGAAGGCAGATGGCAGCCCCGTCAAGGCGGGAGTGGAGACCACCAAACCCTCCAAACAGAGCAACAACAAGTACGCGGCCAGCAGCTACCTGAGCCTGACGCCCGAGCAGTGGAAGTCCCACAGAAGCTACAGCTGCCAGGTCACGCATGAAGGGAGCACCGTGGAGAAGACAGTGGCCCCTACAGAATGTTCATAG",
            false,
        ));
        added_genes_seq3.push((
            "IGLC1",
            "GTTCCCAACTCTAACCCCACCCACGGGAGCCTGGAGCTGCAGGATCCCAGGGGAGGGGTCTCTCTCCCCATCCCAAGTCATCCAGCCCTTCTCCCTGCACTCATGAAACCCCAATAAATATCCTCATTGACAACCAGAAATCTTGTTTTATCTCATTTTTTTTCTCACATAAATTGCTAGCCTCCCCGGGGTTCTCAGTGTGGGGTACAGGGAATTCTGCACCCAGTGTGAAAATCACCCAAGGGAGGAGGCTCACAGCCTCCCTGAGTCATCTCCCCAGAGGGTCCTTCCTCTCCCAGTCACCCCTTCTCCAACTCTCCACTGTACCCCTGAGCTACCAGTCTGGCATCAGTTCAGACCAGTCCCACACCCTCCTAAATTTTACTTCTCAATAAATACCTGATCATGT",
            true,
        ));
    }

    // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

    // Define lengthenings of human BCR 5' UTRs, starting December 2022.  We do not have a good
    // mechanism for this and so just delete the gene and then add it back.
    //
    // In some cases these are combined with previous gene additions, or previous UTR lengthenings.

    if species == "human" {
        // 1
        deleted_genes.push("IGHV1-8");
        added_genes_seq.push((
            "IGHV1-8",
            "ATGGACTGGACCTGGAGGATCCTCTTCTTGGTGGCAGCAGCTACAAGTGCCCACTCCCAGGTGCAGCTGGTGCAGTCTGGGGCTGAGGTGAAGAAGCCTGGGGCCTCAGTGAAGGTCTCCTGCAAGGCTTCTGGATACACCTTCACCAGTTATGATATCAACTGGGTGCGACAGGCCACTGGACAAGGGCTTGAGTGGATGGGATGGATGAACCCTAACAGTGGTAACACAGGCTATGCACAGAAGTTCCAGGGCAGAGTCACCATGACCAGGAACACCTCCATAAGCACAGCCTACATGGAGCTGAGCAGCCTGAGATCTGAGGACACGGCCGTGTATTACTGTGCGAGAGG",
            false,
        ));
        added_genes_seq.push((
            "IGHV1-8",
            "ACTGAGAGCATCACTCAACAACCACATCTGTCCTCTAGAGAAAACCCTGTGAGCACAGCTCCTCACC",
            true,
        ));

        // 2
        deleted_genes.push("IGLV7-46");
        added_genes_seq.push((
            "IGLV7-46",
            "ATGGCCTGGACTCCTCTCTTTCTGTTCCTCCTCACTTGCTGCCCAGGGTCCAATTCCCAGGCTGTGGTGACTCAGGAGCCCTCACTGACTGTGTCCCCAGGAGGGACAGTCACTCTCACCTGTGGCTCCAGCACTGGAGCTGTCACCAGTGGTCATTATCCCTACTGGTTCCAGCAGAAGCCTGGCCAAGCCCCCAGGACACTGATTTATGATACAAGCAACAAACACTCCTGGACACCTGCCCGGTTCTCAGGCTCCCTCCTTGGGGGCAAAGCTGCCCTGACCCTTTTGGGTGCGCAGCCTGAGGATGAGGCTGAGTATTACTGCTTGCTCTCCTATAGTGGTGCTCGG",
            false,
        ));
        added_genes_seq.push((
            "IGLV7-46",
            "AGCACACAGCACACCCCCTCCGTGCGGAGAGCTCAATAGGAGATAAAGAGCCATCAGAATCCAGCCCCAGCTCTGGCACCAGGGGTCCCTTCCAATATCAGCACC",
            true,
        ));

        // 3
        deleted_genes.push("IGKV2-28");
        added_genes_seq.push((
            "IGKV2-28",
            "ATGAGGCTCCCTGCTCAGCTCCTGGGGCTGCTAATGCTCTGGGTCTCTGGATCCAGTGGGGATATTGTGATGACTCAGTCTCCACTCTCCCTGCCCGTCACCCCTGGAGAGCCGGCCTCCATCTCCTGCAGGTCTAGTCAGAGCCTCCTGCATAGTAATGGATACAACTATTTGGATTGGTACCTGCAGAAGCCAGGGCAGTCTCCACAGCTCCTGATCTATTTGGGTTCTAATCGGGCCTCCGGGGTCCCTGACAGGTTCAGTGGCAGTGGATCAGGCACAGATTTTACACTGAAAATCAGCAGAGTGGAGGCTGAGGATGTTGGGGTTTATTACTGCATGCAAGCTCTACAAACTCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV2-28",
            "AGCTCAGCTGTAACTGTGCCTTGACTGATCAGGACTCCTCAGTTCACCTTCTCACA",
            true,
        ));

        // 4
        deleted_genes.push("IGKV2-30");
        added_genes_seq.push((
            "IGKV2-30",
            "ATGAGGCTCCCTGCTCAGCTCCTGGGGCTGCTAATGCTCTGGGTCCCAGGATCCAGTGGGGATGTTGTGATGACTCAGTCTCCACTCTCCCTGCCCGTCACCCTTGGACAGCCGGCCTCCATCTCCTGCAGGTCTAGTCAAAGCCTCGTATACAGTGATGGAAACACCTACTTGAATTGGTTTCAGCAGAGGCCAGGCCAATCTCCAAGGCGCCTAATTTATAAGGTTTCTAACCGGGACTCTGGGGTCCCAGACAGATTCAGCGGCAGTGGGTCAGGCACTGATTTCACACTGAAAATCAGCAGGGTGGAGGCTGAGGATGTTGGGGTTTATTACTGCATGCAAGGTACACACTGGCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV2-30",
            "AAAAGCTCAGCTCTACCCTTGCCTTGACTGATCAGGACTCCTCAGTTCACCTTCTCACA",
            true,
        ));

        // 5
        deleted_genes.push("IGLV8-61");
        added_genes_seq.push((
            "IGLV8-61",
            "ATGAGTGTCCCCACCATGGCCTGGATGATGCTTCTCCTCGGACTCCTTGCTTATGGATCAGGAGTGGATTCTCAGACTGTGGTGACCCAGGAGCCATCGTTCTCAGTGTCCCCTGGAGGGACAGTCACACTCACTTGTGGCTTGAGCTCTGGCTCAGTCTCTACTAGTTACTACCCCAGCTGGTACCAGCAGACCCCAGGCCAGGCTCCACGCACGCTCATCTACAGCACAAACACTCGCTCTTCTGGGGTCCCTGATCGCTTCTCTGGCTCCATCCTTGGGAACAAAGCTGCCCTCACCATCACGGGGGCCCAGGCAGATGATGAATCTGATTATTACTGTGTGCTGTATATGGGTAGTGGCATTTC",
            false,
        ));
        added_genes_seq.push((
            "IGLV8-61",
            "ATGAAAAGGCCCTGAGGAAAACAAACCCCAGCTGGGAAGCCTGAGAACACTTAGCCTTC",
            true,
        ));

        // 6
        deleted_genes.push("IGKV1-6");
        added_genes_seq.push((
            "IGKV1-6",
            "ATGGACATGAGGGTCCCCGCTCAGCTCCTGGGGCTCCTGCTGCTCTGGCTCCCAGGTGCCAGATGTGCCATCCAGATGACCCAGTCTCCATCCTCCCTGTCTGCATCTGTTGGAGACAGAGTCACCATCACTTGCCGGGCAAGTCAGGGCATTAGAAATGATTTAGGCTGGTATCAGCAGAAACCAGGGAAAGCCCCTAAGCTCCTGATCTATGCTGCATCCAGTTTACAAAGTGGGGTCCCATCAAGGTTCAGCGGCAGTGGATCTGGCACAGATTTCACTCTCACCATCAGCAGCCTGCAGCCTGAAGATTTTGCAACTTATTACTGTCTACAAGATTACAATTACCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV1-6",
            "CTCCTGACCTGAAGACTTATTAACAGGCTGATCACACCCTGTGCAGGAGTCAGACCCACTCAGGACACAGC",
            true,
        ));

        // 7
        deleted_genes.push("IGLV7-43");
        added_genes_seq.push((
            "IGLV7-43",
            "ATGGCCTGGACTCCTCTCTTTCTGTTCCTCCTCACTTGCTGCCCAGGGTCCAATTCTCAGACTGTGGTGACTCAGGAGCCCTCACTGACTGTGTCCCCAGGAGGGACAGTCACTCTCACCTGTGCTTCCAGCACTGGAGCAGTCACCAGTGGTTACTATCCAAACTGGTTCCAGCAGAAACCTGGACAAGCACCCAGGGCACTGATTTATAGTACAAGCAACAAACACTCCTGGACCCCTGCCCGGTTCTCAGGCTCCCTCCTTGGGGGCAAAGCTGCCCTGACACTGTCAGGTGTGCAGCCTGAGGACGAGGCTGAGTATTACTGCCTGCTCTACTATGGTGGTGCTCAG",
            false,
        ));
        added_genes_seq.push((
            "IGLV7-43",
            "AGCACACAGCACACCCCCTCCATGGAGAGAGCTCAATAGGAGATAAAGAGCCATCAGAATCCAGCCCCAGCTCTGGCGCCAGGGGTCCCTTCCAATATCAGCACC",
            true,
        ));

        // 8
        deleted_genes.push("IGKV2-24");
        added_genes_seq.push((
            "IGKV2-24",
            "ATGAGGCTCCTTGCTCAGCTTCTGGGGCTGCTAATGCTCTGGGTCCCTGGATCCAGTGGGGATATTGTGATGACCCAGACTCCACTCTCCTCACCTGTCACCCTTGGACAGCCGGCCTCCATCTCCTGCAGGTCTAGTCAAAGCCTCGTACACAGTGATGGAAACACCTACTTGAGTTGGCTTCAGCAGAGGCCAGGCCAGCCTCCAAGACTCCTAATTTATAAGATTTCTAACCGGTTCTCTGGGGTCCCAGACAGATTCAGTGGCAGTGGGGCAGGGACAGATTTCACACTGAAAATCAGCAGGGTGGAAGCTGAGGATGTCGGGGTTTATTACTGCATGCAAGCTACACAATTTCCT",
            false,
        ));
        added_genes_seq.push(("IGKV2-24", "AACTAATTAGGACTCCTCAGGTCACCTTCTCACA", true));

        // 9
        deleted_genes.push("IGLV1-40");
        added_genes_seq.push((
            "IGLV1-40",
            "ATGGCCTGGTCTCCTCTCCTCCTCACTCTCCTCGCTCACTGCACAGGGTCCTGGGCCCAGTCTGTGCTGACGCAGCCGCCCTCAGTGTCTGGGGCCCCAGGGCAGAGGGTCACCATCTCCTGCACTGGGAGCAGCTCCAACATCGGGGCAGGTTATGATGTACACTGGTACCAGCAGCTTCCAGGAACAGCCCCCAAACTCCTCATCTATGGTAACAGCAATCGGCCCTCAGGGGTCCCTGACCGATTCTCTGGCTCCAAGTCTGGCACCTCAGCCTCCCTGGCCATCACTGGGCTCCAGGCTGAGGATGAGGCTGATTATTACTGCCAGTCCTATGACAGCAGCCTGAGTGGTTC",
            false,
        ));
        added_genes_seq.push((
            "IGLV1-40",
            "AGGCTCTGCTTCAGCTGTGGGCACAAGAGGCAGCACTCAGGACAATCTCCAGC",
            true,
        ));

        // 10
        deleted_genes.push("IGLV3-10");
        added_genes_seq.push((
            "IGLV3-10",
            "ATGGCCTGGACCCCTCTCCTGCTCCCCCTCCTCACTTTCTGCACAGTCTCTGAGGCCTCCTATGAGCTGACACAGCCACCCTCGGTGTCAGTGTCCCCAGGACAAACGGCCAGGATCACCTGCTCTGGAGATGCATTGCCAAAAAAATATGCTTATTGGTACCAGCAGAAGTCAGGCCAGGCCCCTGTGCTGGTCATCTATGAGGACAGCAAACGACCCTCCGGGATCCCTGAGAGATTCTCTGGCTCCAGCTCAGGGACAATGGCCACCTTGACTATCAGTGGGGCCCAGGTGGAGGATGAAGCTGACTACTACTGTTACTCAACAGACAGCAGTGGTAATCATAG",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-10",
            "ATAAGAGAGGCCTGGGGAGCCCAGCTGTGCTGTGGGCTCAGGAGGCAGAGCTCTGGGAATCTCACC",
            true,
        ));

        // 11
        deleted_genes.push("IGLV3-25");
        added_genes_seq.push((
            "IGLV3-25",
            "ATGGCCTGGATCCCTCTACTTCTCCCCCTCCTCACTCTCTGCACAGGCTCTGAGGCCTCCTATGAGCTGACACAGCCACCCTCGGTGTCAGTGTCCCCAGGACAGACGGCCAGGATCACCTGCTCTGGAGATGCATTGCCAAAGCAATATGCTTATTGGTACCAGCAGAAGCCAGGCCAGGCCCCTGTGCTGGTGATATATAAAGACAGTGAGAGGCCCTCAGGGATCCCTGAGCGATTCTCTGGCTCCAGCTCAGGGACAACAGTCACGTTGACCATCAGTGGAGTCCAGGCAGAAGACGAGGCTGACTATTACTGTCAATCAGCAGACAGCAGTG",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-25",
            "AGAGAGAATAAGAGAGGCCTGGGGAGCCTAGCTGTGCTGTGGGTCCAGGAGGCAGAACTCTGGGTGTCTCACC",
            true,
        ));

        // 12
        deleted_genes.push("IGLV1-51");
        added_genes_seq.push((
            "IGLV1-51",
            "ATGACCTGCTCCCCTCTCCTCCTCACCCTTCTCATTCACTGCACAGGGTCCTGGGCCCAGTCTGTGTTGACGCAGCCGCCCTCAGTGTCTGCGGCCCCAGGACAGAAGGTCACCATCTCCTGCTCTGGAAGCAGCTCCAACATTGGGAATAATTATGTATCCTGGTACCAGCAGCTCCCAGGAACAGCCCCCAAACTCCTCATTTATGACAATAATAAGCGACCCTCAGGGATTCCTGACCGATTCTCTGGCTCCAAGTCTGGCACGTCAGCCACCCTGGGCATCACCGGACTCCAGACTGGGGACGAGGCCGATTATTACTGCGGAACATGGGATAGCAGCCTGAGTGCTGG",
            false,
        ));
        added_genes_seq.push((
            "IGLV1-51",
            "ATGGACCCTCCTTCTCTCAGAGTATAAAGAGGGGCAGGGAGAGACTTGGGGAAGCTCTGCTTCAGCTGTGAGCGCAGAAGGCAGGACTCGGGACAATCTTCATC",
            true,
        ));

        // 13
        // two reference sequences, we delete both and then add both back, after lengthening
        // one UTR
        deleted_genes.push("IGHV3-33");
        added_genes_seq.push((
            "IGHV3-33",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTCGTTGCTCTTTTAAGAGGTGTCCAGTGTCAGGTGCAGCTGGTGGAGTCTGGGGGAGGCGTGGTCCAGCCTGGGAGGTCCCTGAGACTCTCCTGTGCAGCGTCTGGATTCACCTTCAGTAGCTATGGCATGCACTGGGTCCGCCAGGCTCCAGGCAAGGGGCTGGAGTGGGTGGCAGTTATATGGTATGATGGAAGTAATAAATACTATGCAGACTCCGTGAAGGGCCGATTCACCATCTCCAGAGACAATTCCAAGAACACGCTGTATCTGCAAATGAACAGCCTGAGAGCCGAGGACACGGCTGTGTATTACTGTGCGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-33",
            "CAGCTCTGGGAGAGGAGCCCAGCACTAGAAGTCGGCGGTGTTTCCATTCGGTGATCAGCACTGAACACAGAGGACTCACC",
            true,
        ));
        added_genes_seq.push((
            "IGHV3-33",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTCGTTGCTCTTTTAAGAGGTGTCCAGTGTCAGGTGCAGCTGGTGGAGTCTGGGGGAGGCGTGGTCCAGCCTGGGAGGTCCCTGAGACTCTCCTGTGCAGCCTCTGGATTCACCTTCAGTAGCTATGCTATGCACTGGGTCCGCCAGGCTCCAGGCAAGGGGCTGGAGTGGGTGGCAGTTATATCATATGATGGAAGCAATAAATACTACGCAGACTCCGTGAAGGGCCGATTCACCATCTCCAGAGACAATTCCAAGAACACGCTGTATCTGCAAATGAACAGCCTGAGAGCTGAGGACACGGCTGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-33",
            "AGCTCTGGGAGACGAGCCCAGCACTGGAAGTCGCCGGTGTTTCCATTCGGTGATCATCACTGAACACAGAGGACTCACC",
            true,
        ));

        // 14
        // two reference sequences, we delete both and then both back, after lengthening
        // both UTRs
        deleted_genes.push("IGHV1-69D");
        added_genes_seq.push((
            "IGHV1-69D",
            "ATGGACTGGACCTGGAGGTTCCTCTTTGTGGTGGCAGCAGCTACAGGTGTCCAGTCCCAGGTGCAGCTGGTGCAGTCTGGGGCTGAGGTGAAGAAGCCTGGGTCCTCGGTGAAGGTCTCCTGCAAGGCTTCTGGAGGCACCTTCAGCAGCTATGCTATCAGCTGGGTGCGACAGGCCCCTGGACAAGGGCTTGAGTGGATGGGAGGGATCATCCCTATCTTTGGTACAGCAAACTACGCACAGAAGTTCCAGGGCAGAGTCACGATTACCGCGGACGAATCCACGAGCACAGCCTACATGGAGCTGAGCAGCCTGAGATCTGAGGACACGGCCGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV1-69D",
            "AGAGCATCACATAACAACCACATTCCTCCTCTAAAGAAGCCCCTGGGAGCACAGCTCATCACC",
            true,
        ));
        added_genes_seq.push((
            "IGHV1-69D",
            "ATGGACTGGACCTGGAGGTTCCTCTTTGTGGTGGCAGCAGCTACAGGTGTCCAGTCCCAGGTCCAGCTGGTGCAGTCTGGGGCTGAGGTGAAGAAGCCTGGGTCCTCGGTGAAGGTCTCCTGCAAGGCTTCTGGAGGCACCTTCAGCAGCTATGCTATCAGCTGGGTGCGACAGGCCCCTGGACAAGGGCTTGAGTGGATGGGAGGGATCATCCCTATCTTTGGTACAGCAAACTACGCACAGAAGTTCCAGGGCAGAGTCACGATTACCGCGGACGAATCCACGAGCACAGCCTACATGGAGCTGAGCAGCCTGAGATCTGAGGACACGGCCGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV1-69D",
            "AGCATCACATAACAACCAGATTCCTCCTCTAAAGAAGCCCCTGGGAGCACAGCTCATCACC",
            true,
        ));

        // 15
        deleted_genes.push("IGLV10-54");
        added_genes_seq.push((
            "IGLV10-54",
            "ATGCCCTGGGCTCTGCTCCTCCTGACCCTCCTCACTCACTCTGCAGTGTCAGTGGTCCAGGCAGGGCTGACTCAGCCACCCTCGGTGTCCAAGGGCTTGAGACAGACCGCCACACTCACCTGCACTGGGAACAGCAACATTGTTGGCAACCAAGGAGCAGCTTGGCTGCAGCAGCACCAGGGCCACCCTCCCAAACTCCTATCCTACAGGAATAACAACCGGCCCTCAGGGATCTCAGAGAGATTCTCTGCATCCAGGTCAGGAAACACAGCCTCCCTGACCATTACTGGACTCCAGCCTGAGGACGAGGCTGACTATTACTGCTCAGCATTGGACAGCAGCCTCAGTGCTC",
            false,
        ));
        added_genes_seq.push((
            "IGLV10-54",
            "TCTCCAAACAGAGCTTCAGCAAGCATAGTGGGAATCTGCACC",
            true,
        ));

        // 16
        deleted_genes.push("IGKV2D-29");
        added_genes_seq.push((
            "IGKV2D-29",
            "ATGAGGCTCCCTGCTCAGCTCCTGGGGCTGCTAATGCTCTGGATACCTGGATCCAGTGCAGATATTGTGATGACCCAGACTCCACTCTCTCTGTCCGTCACCCCTGGACAGCCGGCCTCCATCTCCTGCAAGTCTAGTCAGAGCCTCCTGCATAGTGATGGAAAGACCTATTTGTATTGGTACCTGCAGAAGCCAGGCCAGCCTCCACAGCTCCTGATCTATGAAGTTTCCAACCGGTTCTCTGGAGTGCCAGATAGGTTCAGTGGCAGCGGGTCAGGGACAGATTTCACACTGAAAATCAGCCGGGTGGAGGCTGAGGATGTTGGGGTTTATTACTGCATGCAAAGTATACAGCTTCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV2D-29",
            "AGCTCTAACCTTGCCTTGACTGATCAGGACTTCTCAGTTCATCTTCTCACC",
            true,
        ));

        // 17
        deleted_genes.push("IGHV7-4-1");
        added_genes_seq.push((
            "IGHV7-4-1",
            "ATGGACTGGACCTGGAGGATCCTCTTCTTGGTGGCAGCAGCAACAGGTGCCCACTCCCAGGTGCAGCTGGTGCAATCTGGGTCTGAGTTGAAGAAGCCTGGGGCCTCAGTGAAGGTTTCCTGCAAGGCTTCTGGATACACCTTCACTAGCTATGCTATGAATTGGGTGCGACAGGCCCCTGGACAAGGGCTTGAGTGGATGGGATGGATCAACACCAACACTGGGAACCCAACGTATGCCCAGGGCTTCACAGGACGGTTTGTCTTCTCCTTGGACACCTCTGTCAGCACGGCATATCTGCAGATCTGCAGCCTAAAGGCTGAGGACACTGCCGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV7-4-1",
            "ATCACCCAACAACCACACCCCTCCTAAGAAGAAGCCCCTAGACCACAGCTCCACACC",
            true,
        ));

        // 18
        deleted_genes.push("IGLV2-18");
        added_genes_seq.push((
            "IGLV2-18",
            "ATGGCCTGGGCTCTGCTCCTCCTCACCCTCCTCACTCAGGGCACAGGATCCTGGGCTCAGTCTGCCCTGACTCAGCCTCCCTCCGTGTCCGGGTCTCCTGGACAGTCAGTCACCATCTCCTGCACTGGAACCAGCAGTGACGTTGGTAGTTATAACCGTGTCTCCTGGTACCAGCAGCCCCCAGGCACAGCCCCCAAACTCATGATTTATGAGGTCAGTAATCGGCCCTCAGGGGTCCCTGATCGCTTCTCTGGGTCCAAGTCTGGCAACACGGCCTCCCTGACCATCTCTGGGCTCCAGGCTGAGGACGAGGCTGATTATTACTGCAGCTTATATACAAGCAGCAGCACTTTC",
            false,
        ));
        added_genes_seq.push(("IGLV2-18", "CTGGGATCTCAGGAGGCAGCTCTCTCGGAATATCTCCACC", true));

        // 19
        deleted_genes.push("IGHV5-51");
        added_genes_seq.push((
            "IGHV5-51",
            "ATGGGGTCAACCGCCATCCTCGCCCTCCTCCTGGCTGTTCTCCAAGGAGTCTGTGCCGAGGTGCAGCTGGTGCAGTCTGGAGCAGAGGTGAAAAAGCCCGGGGAGTCTCTGAAGATCTCCTGTAAGGGTTCTGGATACAGCTTTACCAGCTACTGGATCGGCTGGGTGCGCCAGATGCCCGGGAAAGGCCTGGAGTGGATGGGGATCATCTATCCTGGTGACTCTGATACCAGATACAGCCCGTCCTTCCAAGGCCAGGTCACCATCTCAGCCGACAAGTCCATCAGCACCGCCTACCTGCAGTGGAGCAGCCTGAAGGCCTCGGACACCGCCATGTATTACTGTGCGAGACA",
            false,
        ));
        added_genes_seq.push((
            "IGHV5-51",
            "AGGGCTCCCCTCCACAGTGAGTCTCCCTCACTGCCCAGCTGGGATCTCAGGGCTTCATTTTCTGTCCTCCACCATC",
            true,
        ));

        // 20
        // Continuation of earlier change.
        // Add missing gene IGHV1-2.  This is in GRCm38 and in 10x data.  There is an IMGT
        // sequence that agrees with it perfectly except for the leader (approximately), but that
        // sequence is not in our data.

        added_genes_seq.push((
            "IGHV1-2",
            "ATGGACTGGACCTGGAGGATCCTCTTCTTGGTGGCAGCAGCCACAGGAGCCCACTCCCAGGTGCAGCTGGTGCAGTCTGGGGCTGAGGTGAAGAAGCCTGGGGCCTCAGTGAAGGTCTCCTGCAAGGCTTCTGGATACACCTTCACCGGCTACTATATGCACTGGGTGCGACAGGCCCCTGGACAAGGGCTTGAGTGGATGGGATGGATCAACCCTAACAGTGGTGGCACAAACTATGCACAGAAGTTTCAGGGCTGGGTCACCATGACCAGGGACACGTCCATCAGCACAGCCTACATGGAGCTGAGCAGGCTGAGATCTGACGACACGGCCGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV1-2",
            "TGTGCCCTGAGAGCATCACCCAGCAACCACATCTGTCCTCTAGAGAATCCCCTGAGAGCTCCGTTCCTCACC",
            true,
        ));

        // 21
        deleted_genes.push("IGHV3-73");
        added_genes_seq.push((
            "IGHV3-73",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTTGTTGCTATTTTAAAAGGTGTCCAGTGTGAGGTGCAGCTGGTGGAGTCCGGGGGAGGCTTGGTCCAGCCTGGGGGGTCCCTGAAACTCTCCTGTGCAGCCTCTGGGTTCACCTTCAGTGGCTCTGCTATGCACTGGGTCCGCCAGGCTTCCGGGAAAGGGCTGGAGTGGGTTGGCCGTATTAGAAGCAAAGCTAACAGTTACGCGACAGCATATGCTGCGTCGGTGAAAGGCAGGTTCACCATCTCCAGAGATGATTCAAAGAACACGGCGTATCTGCAAATGAACAGCCTGAAAACCGAGGACACGGCCGTGTATTACTGTACTAGACA",
            false,
        ));
        added_genes_seq.push(("IGHV3-73", "ACCCTGCAGCTCTGGGAGAGGAGCTCCAGCCTTGGGATTCCCAGCTGTCTCCACTCGGTGATCGGCACTGAATACAGGAGACTCACC", true));

        // 22
        // two reference sequences, we delete both and then both back, after lengthening
        // both UTRs
        deleted_genes.push("IGLV5-45");
        added_genes_seq.push((
            "IGLV5-45",
            "ATGGCCTGGACTCCTCTCCTCCTCCTGTTCCTCTCTCACTGCACAGGTTCCCTCTCGCAGGCTGTGCTGACTCAGCCGTCTTCCCTCTCTGCATCTCCTGGAGCATCAGCCAGTCTCACCTGCACCTTGTGCAGTGGCATCAATGTTGGTACCTACAGGATATACTGGTACCAGCAGAAGCCAGGGAGTCCTCCCCAGTATCTCCTGAGGTACAAATCAGACTCAGATAAGCAGCAGGGCTCTGGAGTCCCCAGCCGCTTCTCTGGATCCAAAGATGCTTCGGCCAATGCAGGGATTTTACTCATCTCTGGGCTCCAGTCTGAGGATGAGGCTGACTATTACTGTATGATTTGGCACAGCAGCGCTTCT",
            false,
        ));
        added_genes_seq.push(("IGLV5-45", "AGTCCCACTGCGGGGGTAAGAGGTTGTGTCCACC", true));
        added_genes_seq.push((
            "IGLV5-45",
            "ATGGCCTGGACTCCTCTCCTCCTCCTGTTCCTCTCTCACTGCACAGGTTCCCTCTCGCAGGCTGTGCTGACTCAGCCGTCTTCCCTCTCTGCATCTCCTGGAGCATCAGCCAGTCTCACCTGCACCTTGCGCAGTGGCATCAATGTTGGTACCTACAGGATATACTGGTACCAGCAGAAGCCAGGGAGTCCTCCCCAGTATCTCCTGAGGTACAAATCAGACTCAGATAAGCAGCAGGGCTCTGGAGTCCCCAGCCGCTTCTCTGGATCCAAAGATGCTTCGGCCAATGCAGGGATTTTACTCATCTCTGGGCTCCAGTCTGAGGATGAGGCTGACTATTACTGTATGATTTGGCACAGCAGCGCTTCT",
            false,
        ));
        added_genes_seq.push(("IGLV5-45", "CACTGCGGGGGTAAGAGGTTGTGTCCACC", true));

        // 23
        deleted_genes.push("IGHV1-3");
        added_genes_seq.push((
            "IGHV1-3",
            "ATGGACTGGACCTGGAGGATCCTCTTTTTGGTGGCAGCAGCCACAGGTGCCCACTCCCAGGTCCAGCTTGTGCAGTCTGGGGCTGAGGTGAAGAAGCCTGGGGCCTCAGTGAAGGTTTCCTGCAAGGCTTCTGGATACACCTTCACTAGCTATGCTATGCATTGGGTGCGCCAGGCCCCCGGACAAAGGCTTGAGTGGATGGGATGGATCAACGCTGGCAATGGTAACACAAAATATTCACAGAAGTTCCAGGGCAGAGTCACCATTACCAGGGACACATCCGCGAGCACAGCCTACATGGAGCTGAGCAGCCTGAGATCTGAAGACACGGCTGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV1-3",
            "ATCACCCAACAACCACATCCCTCCTCAGAAGCCCCCAGAGCACAACGCCTCACC",
            true,
        ));

        // 24
        deleted_genes.push("IGHV3-15");
        added_genes_seq.push((
            "IGHV3-15",
            "ATGGAGTTTGGGCTGAGCTGGATTTTCCTTGCTGCTATTTTAAAAGGTGTCCAGTGTGAGGTGCAGCTGGTGGAGTCTGGGGGAGGCTTGGTAAAGCCTGGGGGGTCCCTTAGACTCTCCTGTGCAGCCTCTGGATTCACTTTCAGTAACGCCTGGATGAGCTGGGTCCGCCAGGCTCCAGGGAAGGGGCTGGAGTGGGTTGGCCGTATTAAAAGCAAAACTGATGGTGGGACAACAGACTACGCTGCACCCGTGAAAGGCAGATTCACCATCTCAAGAGATGATTCAAAAAACACGCTGTATCTGCAAATGAACAGCCTGAAAACCGAGGACACAGCCGTGTATTACTGTACCACAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-15",
            "AGTCCTGACCCTGCAGCTCTGGGAGAGGAGCCCCAGCCTTGGGATTCCCAAGTGTTTTCATTCAGTGATCAGGACTGAACACAGAGGACTCACC",
            true,
        ));

        // 25
        deleted_genes.push("IGLV6-57");
        added_genes_seq.push((
            "IGLV6-57",
            "ATGGCCTGGGCTCCACTACTTCTCACCCTCCTCGCTCACTGCACAGGTTCTTGGGCCAATTTTATGCTGACTCAGCCCCACTCTGTGTCGGAGTCTCCGGGGAAGACGGTAACCATCTCCTGCACCGGCAGCAGTGGCAGCATTGCCAGCAACTATGTGCAGTGGTACCAGCAGCGCCCGGGCAGTGCCCCCACCACTGTGATCTATGAGGATAACCAAAGACCCTCTGGGGTCCCTGATCGGTTCTCTGGCTCCATCGACAGCTCCTCCAACTCTGCCTCCCTCACCATCTCTGGACTGAAGACTGAGGACGAGGCTGACTACTACTGTCAGTCTTATGATAGCAGCAATCA",
            false,
        ));
        added_genes_seq.push((
            "IGLV6-57",
            "TGTGCAACCTCCAGAAAGGGAGAAATTTGCATGGAGCCCTACCACTCTGAGGATACGCGTGACAGATAAGAAGGGCTGGTGGGATCAGTCCTGGTGGTAGCTCAGGAAGCAGAGCCTGGAGCATCTCCACT",
            true,
        ));

        // 26
        deleted_genes.push("IGLV2-14");
        added_genes_seq.push((
            "IGLV2-14",
            "ATGGCCTGGGCTCTGCTGCTCCTCACCCTCCTCACTCAGGGCACAGGGTCCTGGGCCCAGTCTGCCCTGACTCAGCCTGCCTCCGTGTCTGGGTCTCCTGGACAGTCGATCACCATCTCCTGCACTGGAACCAGCAGTGACGTTGGTGGTTATAACTATGTCTCCTGGTACCAACAGCACCCAGGCAAAGCCCCCAAACTCATGATTTATGAGGTCAGTAATCGGCCCTCAGGGGTTTCTAATCGCTTCTCTGGCTCCAAGTCTGGCAACACGGCCTCCCTGACCATCTCTGGGCTCCAGGCTGAGGACGAGGCTGATTATTACTGCAGCTCATATACAAGCAGCAGCACTCTCCACAGTG",
            false,
        ));
        added_genes_seq.push((
            "IGLV2-14",
            "CAGGCCCAGTGCTGGGGTCTCAGGAGGCAGCGCTCTCAGGACATCTCCACC",
            true,
        ));

        // 27
        deleted_genes.push("IGKV3-20");
        added_genes_seq.push((
            "IGKV3-20",
            "ATGGAAACCCCAGCGCAGCTTCTCTTCCTCCTGCTACTCTGGCTCCCAGATACCACCGGAGAAATTGTGTTGACGCAGTCTCCAGGCACCCTGTCTTTGTCTCCAGGGGAAAGAGCCACCCTCTCCTGCAGGGCCAGTCAGAGTGTTAGCAGCAGCTACTTAGCCTGGTACCAGCAGAAACCTGGCCAGGCTCCCAGGCTCCTCATCTATGGTGCATCCAGCAGGGCCACTGGCATCCCAGACAGGTTCAGTGGCAGTGGGTCTGGGACAGACTTCACTCTCACCATCAGCAGACTGGAGCCTGAAGATTTTGCAGTGTATTACTGTCAGCAGTATGGTAGCTCACCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV3-20",
            "ATTCTGTGGCTCAATCTAGGTGATGGTGAGACAAGAGGACACAGGGGTTAAATTCTGTGGCCGCAGGGGAGAAGTTCTACCCTCAGACTGAGCCAACGGCCTTTTCTGGCCTGATCACCTGGGCATGGGCTGCTGAGAGCAGAAAGGGGAGGCAGATTGTCTCTGCAGCTGCAAGCCCAGCACCCGCCCCAGCTGCTTTGCATGTCCCTCCCAGCCGCCCTGCAGTCCAGAGCCCATATCAATGCCTGGGTCAGAGCTCTGGAGAAGAGCTGCTCAGTTAGGACCCAGAGGGAACC",
            true,
        ));

        // 28
        deleted_genes.push("IGHV3-23");
        added_genes_seq.push((
            "IGHV3-23",
            "ATGGAGTTTGGGCTGAGCTGGCTTTTTCTTGTGGCTATTTTAAAAGGTGTCCAGTGTGAGGTGCAGCTGGTGGAGTCTGGGGGAGGCTTGGTACAGCCTGGGGGGTCCCTGAGACTCTCCTGTGCAGCCTCTGGATTCACCTTTAGCAGCTATGCCATGAGCTGGGTCCGCCAGGCTCCAGGGAAGGGGCTGGAGTGGGTCTCAGCTATTAGTGGTAGTGGTGGTAGCACATACTACGCAGACTCCGTGAAGGGCCGGTTCACCATCTCCAGAGACAATTCCAAGAACACGCTGTATCTGCAAATGAACAGCCTGAGAGCCGAGGACACGGCCGTATATTACTGTGCGAAAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-23",
            "CAGCTCTGAGAGAGGAGCCCAGCCCTGGGATTTTCAGGTGTTTTCATTTGGTGATCAGGACTGAACAGAGAGAACTCACC",
            true,
        ));

        // 29
        deleted_genes.push("IGLV1-40");
        added_genes_seq.push((
            "IGLV1-40",
            "ATGGCCTGGTCTCCTCTCCTCCTCACTCTCCTCGCTCACTGCACAGGGTCCTGGGCCCAGTCTGTGCTGACGCAGCCGCCCTCAGTGTCTGGGGCCCCAGGGCAGAGGGTCACCATCTCCTGCACTGGGAGCAGCTCCAACATCGGGGCAGGTTATGATGTACACTGGTACCAGCAGCTTCCAGGAACAGCCCCCAAACTCCTCATCTATGGTAACAGCAATCGGCCCTCAGGGGTCCCTGACCGATTCTCTGGCTCCAAGTCTGGCACCTCAGCCTCCCTGGCCATCACTGGGCTCCAGGCTGAGGATGAGGCTGATTATTACTGCCAGTCCTATGACAGCAGCCTGAGTGGTTC",
            false,
        ));
        added_genes_seq.push((
            "IGLV1-40",
            "AGGGACCTGACCCAGGGCCCAGGGTGGGATTAGAAAGCTGGGGGTCTGATTTGCATGGATGGACCCTCCCACTCTCAGAGTATGAAGAGGGGCAGGGAGAGATTTGGGGAGGCTCTGCTTCAGCTGTGGGCACAAGAGGCAGCACTCAGGACAATCTCCAGC",
            true,
        ));

        // 30
        deleted_genes.push("IGHV4-30-4");
        added_genes_seq.push((
            "IGHV4-30-4",
            "ATGAAACACCTGTGGTTCTTCCTCCTGCTGGTGGCAGCTCCCAGATGGGTCCTGTCCCAGCTGCAGCTGCAGGAGTCGGGCCCAGGACTGGTGAAGCCTTCACAGACCCTGTCCCTCACCTGCACTGTCTCTGGTGGCTCCATCAGCAGTGGTGATTACTACTGGAGCTGGATCCGCCAGCCCCCAGGGAAGGGCCTGGAGTGGATTGGGTACATCTATTACAGTGGGAGCACCTACTACAACCCGTCCCTCAAGAGTCGAGTTACCATATCAGTAGACACGTCCAAGAACCAGTTCTCCCTGAAGCTGAGCTCTGTGACTGCCGCAGACACGGCCGTGTATTACTGT",
            false,
        ));
        added_genes_seq.push((
            "IGHV4-30-4",
            "ACATGCAAATCTCACTTAGGCACCCACAGAAAACCACCACACATTTCCTTAAATTCAGGGTCCTGCTCACATGGGAAATACTTTCTGAGAGTCCTGGACCTCCTGTGCAAGAAC",
            true,
        ));

        // 31
        deleted_genes.push("IGLV9-49");
        added_genes_seq.push((
            "IGLV9-49",
            "ATGGCCTGGGCTCCTCTGCTCCTCACCCTCCTCAGTCTCCTCACAGGGTCCCTCTCCCAGCCTGTGCTGACTCAGCCACCTTCTGCATCAGCCTCCCTGGGAGCCTCGGTCACACTCACCTGCACCCTGAGCAGCGGCTACAGTAATTATAAAGTGGACTGGTACCAGCAGAGACCAGGGAAGGGCCCCCGGTTTGTGATGCGAGTGGGCACTGGTGGGATTGTGGGATCCAAGGGGGATGGCATCCCTGATCGCTTCTCAGTCTTGGGCTCAGGCCTGAATCGGTACCTGACCATCAAGAACATCCAGGAAGAGGATGAGAGTGACTACCACTGTGGGGCAGACCATGGCAGTGGGAGCAACTTCGTG",
            false,
        ));
        added_genes_seq.push((
            "IGLV9-49",
            "ACTTCTTCACTGAGGGAATAAGAGGCTTTAGGGCCTCAGGCTCAGCTGAGAGACTGAAGAACCCAGCATTGCAGCAGCTCCACC",
            true,
        ));

        // 32
        deleted_genes.push("IGLV3-27");
        added_genes_seq.push((
            "IGLV3-27",
            "ATGGCCTGGATCCCTCTCCTGCTCCCCCTCCTCATTCTCTGCACAGTCTCTGTGGCCTCCTATGAGCTGACACAGCCATCCTCAGTGTCAGTGTCTCCGGGACAGACAGCCAGGATCACCTGCTCAGGAGATGTACTGGCAAAAAAATATGCTCGGTGGTTCCAGCAGAAGCCAGGCCAGGCCCCTGTGCTGGTGATTTATAAAGACAGTGAGCGGCCCTCAGGGATCCCTGAGCGATTCTCCGGCTCCAGCTCAGGGACCACAGTCACCTTGACCATCAGCGGGGCCCAGGTTGAGGATGAGGCTGACTATTACTGTTACTCTGCGGCTGACAACAAT",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-27",
            "GAGCCCAGCTGTGCTGTAGGCTCAGGAGGCAGAGCTCTGAATGTCTCACC",
            true,
        ));

        // 33
        deleted_genes.push("IGKV1-39");
        added_genes_seq.push((
            "IGKV1-39",
            "ATGGACATGAGGGTCCCCGCTCAGCTCCTGGGGCTCCTGCTACTCTGGCTCCGAGGTGCCAGATGTGACATCCAGATGACCCAGTCTCCATCCTCCCTGTCTGCATCTGTAGGAGACAGAGTCACCATCACTTGCCGGGCAAGTCAGAGCATTAGCAGCTATTTAAATTGGTATCAGCAGAAACCAGGGAAAGCCCCTAAGCTCCTGATCTATGCTGCATCCAGTTTGCAAAGTGGGGTCCCATCAAGGTTCAGTGGCAGTGGATCTGGGACAGATTTCACTCTCACCATCAGCAGTCTGCAACCTGAAGATTTTGCAACTTACTACTGTCAACAGAGTTACAGTACCCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV1-39",
            "CTGCCCCATGCCCTGCTGATTGATTTGCATGTTCCAGAGCACAGCCCCCAGCCCTGAAGACTTTTTTATGGGCTGGTCGCACCCTGTGCAGGAGTCAGTCTCAGTCAGGACACAGC",
            true,
        ));

        // 34
        deleted_genes.push("IGHV4-31");
        added_genes_seq.push((
            "IGHV4-31",
            "ATGAAACACCTGTGGTTCTTCCTCCTGCTGGTGGCAGCTCCCAGATGGGTCCTGTCCCAGCTGCAGCTGCAGGAGTCCGGCTCAGGACTGGTGAAGCCTTCACAGACCCTGTCCCTCACCTGCGCTGTCTCTGGTGGCTCCATCAGCAGTGGTGGTTACTCCTGGAGCTGGATCCGGCAGCCACCAGGGAAGGGCCTGGAGTGGATTGGGTACATCTATCATAGTGGGAGCACCTACTACAACCCGTCCCTCAAGAGTCGAGTCACCATATCAGTAGACAGGTCCAAGAACCAGTTCTCCCTGAAGCTGAGCTCTGTGACCGCCGCGGACACGGCCGTGTATTACTGTGCCAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV4-31",
            "ATCTCACTTAGGCACCCACAGGAAACCACCACACATTTCCTTAAATTCAGGGTCCAGCTCACATGGGAAATACTTTCTGAGAGTCCTGGACCTCCTGTGCAAGAAC",
            true,
        ));

        // 35
        deleted_genes.push("IGLV3-1");
        added_genes_seq.push((
            "IGLV3-1",
            "ATGGCATGGATCCCTCTCTTCCTCGGCGTCCTTGCTTACTGCACAGGATCCGTGGCCTCCTATGAGCTGACTCAGCCACCCTCAGTGTCCGTGTCCCCAGGACAGACAGCCAGCATCACCTGCTCTGGAGATAAATTGGGGGATAAATATGCTTGCTGGTATCAGCAGAAGCCAGGCCAGTCCCCTGTGCTGGTCATCTATCAAGATAGCAAGCGGCCCTCAGGGATCCCTGAGCGATTCTCTGGCTCCAACTCTGGGAACACAGCCACTCTGACCATCAGCGGGACCCAGGCTATGGATGAGGCTGACTATTACTGTCAGGCGTGGGACAGCAGCACTGCACACA",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-1",
            "CTCTGGAAACCACACAGCTCCTCCTGCAGCAGCCCCTGACTGCTGATTTGCATCACGGGCCGCTCTTTCCAGCAAGGGGATAAGAGAGGCCTGGAAGAACCTGCCCAGCCTGGGCCTCAGGAAGCAGCATCGGAGGTGCCTCAGCC",
            true,
        ));

        // 36
        deleted_genes.push("IGKV2D-30");
        added_genes_seq.push((
            "IGKV2D-30",
            "ATGAGGCTCCCTGCTCAGCTCCTGGGGCTGCTAATGCTCTGGGTCCCAGGATCCAGTGGGGATGTTGTGATGACTCAGTCTCCACTCTCCCTGCCCGTCACCCTTGGACAGCCGGCCTCCATCTCCTGCAGGTCTAGTCAAAGCCTCGTATACAGTGATGGAAACACCTACTTGAATTGGTTTCAGCAGAGGCCAGGCCAATCTCCAAGGCGCCTAATTTATAAGGTTTCTAACTGGGACTCTGGGGTCCCAGACAGATTCAGCGGCAGTGGGTCAGGCACTGATTTCACACTGAAAATCAGCAGGGTGGAGGCTGAGGATGTTGGGGTTTATTACTGCATGCAAGGTACACACTGGCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV2D-30",
            "CCTACCCTCCCCTTGGCTCTTTCCACCCCACTACACCCACCAGGTGATTTGCATATTATCCCTTGGTGAAGACTTTCCTTGTGAGTCTGAGATAAAAGCTCAGCTCTAACCTTGCCTTGACTGATCAGGACTCCTCAGTTCACCTTCTCACA",
            true,
        ));

        // 37
        deleted_genes.push("IGLV3-21");
        added_genes_seq.push((
            "IGLV3-21",
            "ATGGCCTGGACCGTTCTCCTCCTCGGCCTCCTCTCTCACTGCACAGGCTCTGTGACCTCCTATGTGCTGACTCAGCCACCCTCGGTGTCAGTGGCCCCAGGACAGACGGCCAGGATTACCTGTGGGGGAAACAACATTGGAAGTAAAAGTGTGCACTGGTACCAGCAGAAGCCAGGCCAGGCCCCTGTGCTGGTCGTCTATGATGATAGCGACCGGCCCTCAGGGATCCCTGAGCGATTCTCTGGCTCCAACTCTGGGAACACGGCCACCCTGACCATCAGCAGGGTCGAAGCCGGGGATGAGGCCGACTATTACTGTCAGGTGTGGGATAGTAGTAGTGATCATCCCACG",
            false,
        ));
        added_genes_seq.push(("IGLV3-21", "CTGAGTCCTTCTCTGGAAACCACAGATCTCCTCCAGCAGCAGCCTCTGACTCTGCTGATTTGCATCATGGGCCGCTCTCTCCAGCAAGGGGATAAGAGAGGCCTGGGAGGAACCTGCTCAGTCTGGGCCTAAGGAAGCAGCACTGGTGGTGCCTCAGCC", true));

        // 38
        deleted_genes.push("IGLV3-19");
        added_genes_seq.push((
            "IGLV3-19",
            "ATGGCCTGGACCCCTCTCTGGCTCACTCTCCTCACTCTTTGCATAGGTTCTGTGGTTTCTTCTGAGCTGACTCAGGACCCTGCTGTGTCTGTGGCCTTGGGACAGACAGTCAGGATCACATGCCAAGGAGACAGCCTCAGAAGCTATTATGCAAGCTGGTACCAGCAGAAGCCAGGACAGGCCCCTGTACTTGTCATCTATGGTAAAAACAACCGGCCCTCAGGGATCCCAGACCGATTCTCTGGCTCCAGCTCAGGAAACACAGCTTCCTTGACCATCACTGGGGCTCAGGCGGAAGATGAGGCTGACTATTACTGTAACTCCCGGGACAGCAGTG",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-19",
            "CTTCCCTTCCTATGATAAGAGAGGCCTGGAGGTTCCTCCTTAGCTGTGGGCTCAGAAGCAGAGTTCTGGGGTGTCTCCACC",
            true,
        ));

        // 39
        deleted_genes.push("IGHV4-38-2");
        added_genes_seq.push((
            "IGHV4-38-2",
            "ATGAAGCACCTGTGGTTTTTCCTCCTGCTGGTGGCAGCTCCCAGATGGGTCCTGTCCCAGGTGCAGCTGCAGGAGTCGGGCCCAGGACTGGTGAAGCCTTCGGAGACCCTGTCCCTCACCTGCACTGTCTCTGGTTACTCCATCAGCAGTGGTTACTACTGGGGCTGGATCCGGCAGCCCCCAGGGAAGGGGCTGGAGTGGATTGGGAGTATCTATCATAGTGGGAGCACCTACTACAACCCGTCCCTCAAGAGTCGAGTCACCATATCAGTAGACACGTCCAAGAACCAGTTCTCCCTGAAGCTGAGCTCTGTGACCGCCGCAGACACGGCCGTGTATTACTGT",
            false,
        ));
        added_genes_seq.push((
            "IGHV4-38-2",
            "AAATGCTTTCTGAGAGTCATGGACCTCCTGTGCAAGAAC",
            true,
        ));

        // 40
        deleted_genes.push("IGHV3-20");
        added_genes_seq.push((
            "IGHV3-20",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTTGTTGCTATTTTAAAAGGTGTCCAGTGTGAGGTGCAGCTGGTGGAGTCTGGGGGAGGTGTGGTACGGCCTGGGGGGTCCCTGAGACTCTCCTTTGCAGCCTCTGGATTCACCTTTGATGATTATGGCATGAGCTGGGTCCGCCAAGCTCCAGGGAAGGGGCTGGAGTGGGTCTCTGGTATTAATTGGAATGGTGGTAGCACAGGTTATGCAGACTCTGTGAAGGGCCGATTCACCATCTCCAGAGACAACGCCAAGAACTCCCTGTATCTGCAAATGAACAGTCTGAGAGCCGAGGACACGGCCTTGTATCACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-20",
            "AGCCTACTCTGAGGCATCCCTTCCAGAAGTCACTATATAGTAGGAGACATGCAAATGGGGTCCTCCCTCTGCCGATGAAAACCAGCCCAGCCCTGACCCTGCAGCTCTGGGAGAGGAGCCCCAGCCCTGAGATTCCCACGTGTTTCCATTCAGTGATCAGCACTGAACACAGAGGACTCGCC",
            true,
        ));

        // 41
        deleted_genes.push("IGKV4-1");
        added_genes_seq.push((
            "IGKV4-1",
            "ATGGTGTTGCAGACCCAGGTCTTCATTTCTCTGTTGCTCTGGATCTCTGGTGCCTACGGGGACATCGTGATGACCCAGTCTCCAGACTCCCTGGCTGTGTCTCTGGGCGAGAGGGCCACCATCAACTGCAAGTCCAGCCAGAGTGTTTTATACAGCTCCAACAATAAGAACTACTTAGCTTGGTACCAGCAGAAACCAGGACAGCCTCCTAAGCTGCTCATTTACTGGGCATCTACCCGGGAATCCGGGGTCCCTGACCGATTCAGTGGCAGCGGGTCTGGGACAGATTTCACTCTCACCATCAGCAGCCTGCAGGCTGAAGATGTGGCAGTTTATTACTGTCAGCAATATTATAGTACTCCT",
            false,
        ));
        added_genes_seq.push(("IGKV4-1", "CTTTTCTATTCATACAATTACACATTCTGTGATGATATTTTTGGCTCTTGATTTACATTGGGTACTTTCACAACCCACTGCTCATGAAATTTGCTTTTGTACTCACTGGTTGTTTTTGCATAGGCCCCTCCAGGCCACGACCAGCTGTTTGGATTTTATAAACGGGCCGTTTGCATTGTGAACTGAGCTACAACAGGCAGGCAGGGGCAGCAAG", true));

        // 42
        deleted_genes.push("IGHV3-9");
        added_genes_seq.push((
            "IGHV3-9",
            "ATGGAGTTGGGACTGAGCTGGATTTTCCTTTTGGCTATTTTAAAAGGTGTCCAGTGTGAAGTGCAGCTGGTGGAGTCTGGGGGAGGCTTGGTACAGCCTGGCAGGTCCCTGAGACTCTCCTGTGCAGCCTCTGGATTCACCTTTGATGATTATGCCATGCACTGGGTCCGGCAAGCTCCAGGGAAGGGCCTGGAGTGGGTCTCAGGTATTAGTTGGAATAGTGGTAGCATAGGCTATGCGGACTCTGTGAAGGGCCGATTCACCATCTCCAGAGACAACGCCAAGAACTCCCTGTATCTGCAAATGAACAGTCTGAGAGCTGAGGACACGGCCTTGTATTACTGTGCAAAAGATA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-9",
            "CCCTGCAGCTCTGGGAGAGGAGCCCCAGCCCTGAGATTCCCAGGTGTTTCCATTCAGTGATCAGCACTGAACACAGAGGACTCACC",
            true,
        ));

        // 43
        deleted_genes.push("IGKV3-11");
        added_genes_seq.push((
            "IGKV3-11",
            "ATGGAAGCCCCAGCTCAGCTTCTCTTCCTCCTGCTACTCTGGCTCCCAGATACCACCGGAGAAATTGTGTTGACACAGTCTCCAGCCACCCTGTCTTTGTCTCCAGGGGAAAGAGCCACCCTCTCCTGCAGGGCCAGTCAGAGTGTTAGCAGCTACTTAGCCTGGTACCAACAGAAACCTGGCCAGGCTCCCAGGCTCCTCATCTATGATGCATCCAACAGGGCCACTGGCATCCCAGCCAGGTTCAGTGGCAGTGGGTCTGGGACAGACTTCACTCTCACCATCAGCAGCCTAGAGCCTGAAGATTTTGCAGTTTATTACTGTCAGCAGCGTAGCAACTGGCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV3-11",
            "CAGAGCCCATATCAATGCCTGTGTCAGAGCCCTGGGGAGGAACTGCTCAGTTAGGACCCAGAGGGAACC",
            true,
        ));

        // 44
        deleted_genes.push("IGKV1D-13");
        added_genes_seq.push((
            "IGKV1D-13",
            "ATGGACATGAGGGTCCCCGCTCAGCTCCTGGGGCTTCTGCTGCTCTGGCTCCCAGGTGCCAGATGTGCCATCCAGTTGACCCAGTCTCCATCCTCCCTGTCTGCATCTGTAGGAGACAGAGTCACCATCACTTGCCGGGCAAGTCAGGGCATTAGCAGTGCTTTAGCCTGGTATCAGCAGAAACCAGGGAAAGCTCCTAAGCTCCTGATCTATGATGCCTCCAGTTTGGAAAGTGGGGTCCCATCAAGGTTCAGCGGCAGTGGATCTGGGACAGATTTCACTCTCACCATCAGCAGCCTGCAGCCTGAAGATTTTGCAACTTATTACTGTCAACAGTTTAATAGTTACCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV1D-13",
            "AGGCTGGTCACACTTCTTGCAGGAGTCAGACCCACTCAGGACACAGC",
            true,
        ));

        // 45
        deleted_genes.push("IGLV1-47");
        added_genes_seq.push((
            "IGLV1-47",
            "ATGGCCGGCTTCCCTCTCCTCCTCACCCTCCTCACTCACTGTGCAGGGTCCTGGGCCCAGTCTGTGCTGACTCAGCCACCCTCAGCGTCTGGGACCCCCGGGCAGAGGGTCACCATCTCTTGTTCTGGAAGCAGCTCCAACATCGGAAGTAATTATGTATACTGGTACCAGCAGCTCCCAGGAACGGCCCCCAAACTCCTCATCTATAGTAATAATCAGCGGCCCTCAGGGGTCCCTGACCGATTCTCTGGCTCCAAGTCTGGCACCTCAGCCTCCCTGGCCATCAGTGGGCTCCGGTCCGAGGATGAGGCTGATTATTACTGTGCAGCATGGGATGACAGCCTGAGTGGT",
            false,
        ));
        added_genes_seq.push((
            "IGLV1-47",
            "AGGGTGGGGTCAAAAACCGGGGGGATCTGATTTGCATGGATGGACTCTCCCCCTCTCAGAGTATGAAGAGAGGGAGAGATCTGGGGGAAGCTCAGCTTCAGCTGTGGTAGAGAAGACAGGATTCAGGACAATCTCCAGC",
            true,
        ));

        // 46
        deleted_genes.push("IGHV3-66");
        added_genes_seq.push((
            "IGHV3-66",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTTGTTGCTATTTTAAAAGGTGTCCAGTGTGAGGTGCAGCTGGTGGAGTCTGGAGGAGGCTTGATCCAGCCTGGGGGGTCCCTGAGACTCTCCTGTGCAGCCTCTGGGTTCACCGTCAGTAGCAACTACATGAGCTGGGTCCGCCAGGCTCCAGGGAAGGGGCTGGAGTGGGTCTCAGTTATTTATAGCTGTGGTAGCACATACTACGCAGACTCCGTGAAGGGCCGATTCACCATCTCCAGAGACAATTCCAAGAACACGCTGTATCTTCAAATGAACAGCCTGAGAGCTGAGGACACGGCTGTGTATTACTGTGCGAGAGA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-66",
            "CTCTGCTGATGAAAACCAGCCCAGCCCTGACCCTGCAGCTCTGGGAGAGGAGCCCAGCACTGGGATTCCGAGGTGTTTCCATTCAGTGATCTGCACTGAACACAGAGGACTCGCC",
            true,
        ));

        // 47
        deleted_genes.push("IGKV1-17");
        added_genes_seq.push((
            "IGKV1-17",
            "ATGGACATGAGGGTCCCCGCTCAGCTCCTGGGGCTCCTGCTGCTCTGGTTCCCAGGTGCCAGGTGTGACATCCAGATGACCCAGTCTCCATCCTCCCTGTCTGCATCTGTAGGAGACAGAGTCACCATCACTTGCCGGGCAAGTCAGGGCATTAGAAATGATTTAGGCTGGTATCAGCAGAAACCAGGGAAAGCCCCTAAGCGCCTGATCTATGCTGCATCCAGTTTGCAAAGTGGGGTCCCATCAAGGTTCAGCGGCAGTGGATCTGGGACAGAATTCACTCTCACAATCAGCAGCCTGCAGCCTGAAGATTTTGCAACTTATTACTGTCTACAGCATAATAGTTACCCT",
            false,
        ));
        added_genes_seq.push((
            "IGKV1-17",
            "CTCCTGCCCTGAAGCCTTATTAATAGGCTGGACACACTTCATGCAGGAATCAGTCCCACTCAGGACACAGC",
            true,
        ));

        // 48
        deleted_genes.push("IGLV1-44");
        added_genes_seq.push((
            "IGLV1-44",
            "ATGGCCAGCTTCCCTCTCCTCCTCACCCTCCTCACTCACTGTGCAGGGTCCTGGGCCCAGTCTGTGCTGACTCAGCCACCCTCAGCGTCTGGGACCCCCGGGCAGAGGGTCACCATCTCTTGTTCTGGAAGCAGCTCCAACATCGGAAGTAATACTGTAAACTGGTACCAGCAGCTCCCAGGAACGGCCCCCAAACTCCTCATCTATAGTAATAATCAGCGGCCCTCAGGGGTCCCTGACCGATTCTCTGGCTCCAAGTCTGGCACCTCAGCCTCCCTGGCCATCAGTGGGCTCCAGTCTGAGGATGAGGCTGATTATTACTGTGCAGCATGGGATGACAGCCTGAATGGTCC",
            false,
        ));
        added_genes_seq.push((
            "IGLV1-44",
            "GGGTGGGGTCACAAAGCTGGGGGGGTCTGATTTGCATGGATGGACTCTCCCCCTCTCAGAGTATGAAGAGAGGGAGAGATCTGGGGGAAGCTCAGCTTCAGCTGTGGGTAGAGAAGACAGGACTCAGGACAATCTCCAGC",
            true,
        ));

        // 49
        deleted_genes.push("IGHV3-30");
        added_genes_seq.push((
            "IGHV3-30",
            "ATGGAGTTTGGGCTGAGCTGGGTTTTCCTCGTTGCTCTTTTAAGAGGTGTCCAGTGTCAGGTGCAGCTGGTGGAGTCTGGGGGAGGCGTGGTCCAGCCTGGGAGGTCCCTGAGACTCTCCTGTGCAGCCTCTGGATTCACCTTCAGTAGCTATGGCATGCACTGGGTCCGCCAGGCTCCAGGCAAGGGGCTGGAGTGGGTGGCAGTTATATCATATGATGGAAGTAATAAATACTATGCAGACTCCGTGAAGGGCCGATTCACCATCTCCAGAGACAATTCCAAGAACACGCTGTATCTGCAAATGAACAGCCTGAGAGCTGAGGACACGGCTGTGTATTACTGTGCGAAA",
            false,
        ));
        added_genes_seq.push((
            "IGHV3-30",
            "CCTCTACTGATGAAAACCAGCCCAGCCCTGACCCTGCAGCTCTGGGAGAGGAGCCCAGCACTAGAAGTCGGCGGTGTTTCCATTCGGTGATCAGCACTGAACACAGAGGACTCACC",
            true,
        ));

        // 50
        deleted_genes.push("IGHV4-34");
        added_genes_seq.push((
            "IGHV4-34",
            "ATGGACCTCCTGCACAAGAACATGAAACACCTGTGGTTCTTCCTCCTCCTGGTGGCAGCTCCCAGATGGGTCCTGTCCCAGGTGCAGCTACAGCAGTGGGGCGCAGGACTGTTGAAGCCTTCGGAGACCCTGTCCCTCACCTGCGCTGTCTATGGTGGGTCCTTCAGTGGTTACTACTGGAGCTGGATCCGCCAGCCCCCAGGGAAGGGGCTGGAGTGGATTGGGGAAATCAATCATAGTGGAAGCACCAACTACAACCCGTCCCTCAAGAGTCGAGTCACCATATCAGTAGACACGTCCAAGAACCAGTTCTCCCTGAAGCTGAGCTCTGTGACCGCCGCGGACACGGCTGTGTATTACTGTGCGAGAGG",
            false,
        ));
        added_genes_seq.push(("IGHV4-34", "AGGGTCCAGCTCACATGGGAAGTGCTTTCTGAGAGTC", true));

        // 51
        deleted_genes.push("IGLV3-12");
        added_genes_seq.push((
            "IGLV3-12",
            "ATGGCCTGGACCCCTCTCCTCCTCAGCCTCCTCGCTCACTGCACAGGCTCTGCGACCTCCTATGAGCTGACTCAGCCACACTCAGTGTCAGTGGCCACAGCACAGATGGCCAGGATCACCTGTGGGGGAAACAACATTGGAAGTAAAGCTGTGCACTGGTACCAGCAAAAGCCAGGCCAGGACCCTGTGCTGGTCATCTATAGCGATAGCAACCGGCCCTCAGGGATCCCTGAGCGATTCTCTGGCTCCAACCCAGGGAACACCGCCACCCTAACCATCAGCAGGATCGAGGCTGGGGATGAGGCTGACTATTACTGTCAGGTGTGGGACAGTAGTAGTGATCATCC",
            false,
        ));
        added_genes_seq.push(("IGLV3-12", "TGGGCTGTTCTCTCCAGCAAGGGGATAAGAGAGGTCTGGGAGGAACCTGCCTAGCCTGGGCCTCAGGAAGCAGCATCAGCAGTGCCTCAGCC", true));

        // 52
        deleted_genes.push("IGHV4-39");
        added_genes_seq.push((
            "IGHV4-39",
            "ATGGATCTCATGTGCAAGAAAATGAAGCACCTGTGGTTCTTCCTCCTGCTGGTGGCGGCTCCCAGATGGGTCCTGTCCCAGCTGCAGCTGCAGGAGTCGGGCCCAGGACTGGTGAAGCCTTCGGAGACCCTGTCCCTCACCTGCACTGTCTCTGGTGGCTCCATCAGCAGTAGTAGTTACTACTGGGGCTGGATCCGCCAGCCCCCAGGGAAGGGGCTGGAGTGGATTGGGAGTATCTATTATAGTGGGAGCACCTACTACAACCCGTCCCTCAAGAGTCGAGTCACCATATCCGTAGACACGTCCAAGAACCAGTTCTCCCTGAAGCTGAGCTCTGTGACCGCCGCAGACACGGCTGTGTATTACTGTGCGAGACA",
            false,
        ));
        added_genes_seq.push((
            "IGHV4-39",
            "ACATTTCCTTAAATTCAGGTCCAACTCATAAGGGAAATGCTTTCTGAGAGTC",
            true,
        ));

        // 53
        deleted_genes.push("IGLV4-69");
        added_genes_seq.push((
            "IGLV4-69",
            "ATGGCTTGGACCCCACTCCTCTTCCTCACCCTCCTCCTCCACTGCACAGGGTCTCTCTCCCAGCTTGTGCTGACTCAATCGCCCTCTGCCTCTGCCTCCCTGGGAGCCTCGGTCAAGCTCACCTGCACTCTGAGCAGTGGGCACAGCAGCTACGCCATCGCATGGCATCAGCAGCAGCCAGAGAAGGGCCCTCGGTACTTGATGAAGCTTAACAGTGATGGCAGCCACAGCAAGGGGGACGGGATCCCTGATCGCTTCTCAGGCTCCAGCTCTGGGGCTGAGCGCTACCTCACCATCTCCAGCCTCCAGTCTGAGGATGAGGCTGACTATTACTGTCAGACCTGGGGCACTGGCATTCA",
            false,
        ));
        added_genes_seq.push((
            "IGLV4-69",
            "ACTACAGGGTGGGTAAGAAATACCTGCAACTGTCAGCCTCAGCAGAGCTCTGGGGAGTCTGCACC",
            true,
        ));

        // 54
        deleted_genes.push("IGLV5-37");
        added_genes_seq.push((
            "IGLV5-37",
            "ATGGCCTGGACTCCTCTTCTTCTCTTGCTCCTCTCTCACTGCACAGGTTCCCTCTCCCAGCCTGTGCTGACTCAGCCACCTTCCTCCTCCGCATCTCCTGGAGAATCCGCCAGACTCACCTGCACCTTGCCCAGTGACATCAATGTTGGTAGCTACAACATATACTGGTACCAGCAGAAGCCAGGGAGCCCTCCCAGGTATCTCCTGTACTACTACTCAGACTCAGATAAGGGCCAGGGCTCTGGAGTCCCCAGCCGCTTCTCTGGATCCAAAGATGCTTCAGCCAATACAGGGATTTTACTCATCTCCGGGCTCCAGTCTGAGGATGAGGCTGACTATTACTGTATGATTTGGCCAAGCAATGCTTCT",
            false,
        ));
        added_genes_seq.push(("IGLV5-37", "AGTCCCACTGTGCATGTCAGGCTGTGTCCACC", true));

        // 55
        deleted_genes.push("IGLV5-52");
        added_genes_seq.push((
            "IGLV5-52",
            "ATGGCCTGGACTCTTCTCCTTCTCGTGCTCCTCTCTCACTGCACAGGTTCCCTCTCCCAGCCTGTGCTGACTCAGCCATCTTCCCATTCTGCATCTTCTGGAGCATCAGTCAGACTCACCTGCATGCTGAGCAGTGGCTTCAGTGTTGGGGACTTCTGGATAAGGTGGTACCAACAAAAGCCAGGGAACCCTCCCCGGTATCTCCTGTACTACCACTCAGACTCCAATAAGGGCCAAGGCTCTGGAGTTCCCAGCCGCTTCTCTGGATCCAACGATGCATCAGCCAATGCAGGGATTCTGCGTATCTCTGGGCTCCAGCCTGAGGATGAGGCTGACTATTACTGTGGTACATGGCACAGCAACTCTAAGACTCA",
            false,
        ));
        added_genes_seq.push(("IGLV5-52", "CCCACTGTTAGGGCTCAGGGGCTGTGTCCACC", true));

        // 56
        deleted_genes.push("IGLV4-3");
        added_genes_seq.push((
            "IGLV4-3",
            "ATGGCCTGGGTCTCCTTCTACCTACTGCCCTTCATTTTCTCCACAGGTCTCTGTGCTCTGCCTGTGCTGACTCAGCCCCCGTCTGCATCTGCCTTGCTGGGAGCCTCGATCAAGCTCACCTGCACCCTAAGCAGTGAGCACAGCACCTACACCATCGAATGGTATCAACAGAGACCAGGGAGGTCCCCCCAGTATATAATGAAGGTTAAGAGTGATGGCAGCCACAGCAAGGGGGACGGGATCCCCGATCGCTTCATGGGCTCCAGTTCTGGGGCTGACCGCTACCTCACCTTCTCCAACCTCCAGTCTGACGATGAGGCTGAGTATCACTGTGGAGAGAGCCACACGATTGATGGCCAAGTCGGT",
            false,
        ));
        added_genes_seq.push((
            "IGLV4-3",
            "CCAGGTTCCACTGGGCAGTCTCGAATAGAGCTCTTGGAAGTCCCTCCAACC",
            true,
        ));

        // 57
        deleted_genes.push("IGLV3-9");
        added_genes_seq.push((
            "IGLV3-9",
            "ATGGCCTGGACCGCTCTCCTTCTGAGCCTCCTTGCTCACTTTACAGGTTCTGTGGCCTCCTATGAGCTGACTCAGCCACTCTCAGTGTCAGTGGCCCTGGGACAGACGGCCAGGATTACCTGTGGGGGAAACAACATTGGAAGTAAAAATGTGCACTGGTACCAGCAGAAGCCAGGCCAGGCCCCTGTGCTGGTCATCTATAGGGATAGCAACCGGCCCTCTGGGATCCCTGAGCGATTCTCTGGCTCCAACTCGGGGAACACGGCCACCCTGACCATCAGCAGAGCCCAAGCCGGGGATGAGGCTGACTATTACTGTCAGGTGTGGGACAGCAGCACTGCACACA",
            false,
        ));
        added_genes_seq.push((
            "IGLV3-9",
            "AGTAGCAGCCCTTGACTCTGCTGATTTGCATCACAGGCTGCTCTCTTCAGCAAGGGGATAAGAGAGGGCTGGAAGGAACCTGCCCAGCCTGGGCCTCAGGAAGCAGCATCGGGGGTGCCGCAGCC",
            true,
        ));

        // 58
        deleted_genes.push("IGKV2D-26");
        added_genes_seq.push((
            "IGKV2D-26",
            "ATGAGGCTCCCTGCTCAGCTCTTGGGGCTGCTAATGCTCTGGGTCCCTGGATCCAGTGCAGAGATTGTGATGACCCAGACTCCACTCTCCTTGTCTATCACCCCTGGAGAGCAGGCCTCCATGTCCTGCAGGTCTAGTCAGAGCCTCCTGCATAGTGATGGATACACCTATTTGTATTGGTTTCTGCAGAAAGCCAGGCCAGTCTCCACGCTCCTGATCTATGAAGTTTCCAACCGGTTCTCTGGAGTGCCAGATAGGTTCAGTGGCAGCGGGTCAGGGACAGATTTCACACTGAAAATCAGCCGGGTGGAGGCTGAGGATTTTGGAGTTTATTACTGCATGCAAGATGCACAAGATCCT",
            false,
        ));
        added_genes_seq.push(("IGKV2D-26", "ACTGATCAGGACTCCTCAGTTCACCTTCTCACT", true));

        // 59
        deleted_genes.push("IGLV4-60");
        added_genes_seq.push((
            "IGLV4-60",
            "ATGGCCTGGACCCCACTCCTCCTCCTCTTCCCTCTCCTCCTCCACTGCACAGGGTCTCTCTCCCAGCCTGTGCTGACTCAATCATCCTCTGCCTCTGCTTCCCTGGGATCCTCGGTCAAGCTCACCTGCACTCTGAGCAGTGGGCACAGTAGCTACATCATCGCATGGCATCAGCAGCAGCCAGGGAAGGCCCCTCGGTACTTGATGAAGCTTGAAGGTAGTGGAAGCTACAACAAGGGGAGCGGAGTTCCTGATCGCTTCTCAGGCTCCAGCTCTGGGGCTGACCGCTACCTCACCATCTCCAACCTCCAGTTTGAGGATGAGGCTGATTATTACTGTGAGACCTGGGACAGTAACACTCA",
            false,
        ));
        added_genes_seq.push(("IGLV4-60", "AGCGTGGCTGCCTCAGCAGAGCTCTGGGGAGTCTGCACC", true));

        // 60
        deleted_genes.push("IGKV2D-40");
        added_genes_seq.push((
            "IGKV2D-40",
            "ATGAGGCTCCCTGCTCAGCTCCTGGGGCTGCTAATGCTCTGGGTCCCTGGATCCAGTGAGGATATTGTGATGACCCAGACTCCACTCTCCCTGCCCGTCACCCCTGGAGAGCCGGCCTCCATCTCCTGCAGGTCTAGTCAGAGCCTCTTGGATAGTGATGATGGAAACACCTATTTGGACTGGTACCTGCAGAAGCCAGGGCAGTCTCCACAGCTCCTGATCTATACGCTTTCCTATCGGGCCTCTGGAGTCCCAGACAGGTTCAGTGGCAGTGGGTCAGGCACTGATTTCACACTGAAAATCAGCAGGGTGGAGGCTGAGGATGTTGGAGTTTATTACTGCATGCAACGTATAGAGTTTCCTTC",
            false,
        ));
        added_genes_seq.push(("IGKV2D-40", "ACTGATCAGGACTCCTCAGTTCACCTTCTCACC", true));

        // 61
        deleted_genes.push("IGKV1-16");
        added_genes_seq.push((
            "IGKV1-16",
            "ATGGACATGAGAGTCCTCGCTCAGCTCCTGGGGCTCCTGCTGCTCTGTTTCCCAGGTGCCAGATGTGACATCCAGATGACCCAGTCTCCATCCTCACTGTCTGCATCTGTAGGAGACAGAGTCACCATCACTTGTCGGGCGAGTCAGGGCATTAGCAATTATTTAGCCTGGTTTCAGCAGAAACCAGGGAAAGCCCCTAAGTCCCTGATCTATGCTGCATCCAGTTTGCAAAGTGGGGTCCCATCAAAGTTCAGCGGCAGTGGATCTGGGACAGATTTCACTCTCACCATCAGCAGCCTGCAGCCTGAAGATTTTGCAACTTATTACTGCCAACAGTATAATAGTTACCCT",
            false,
        ));
        added_genes_seq.push(("IGKV1-16", "CAGGAATCAGACCCAGTCAGGACACAGC", true));
    }

    // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

    // Define older exceptions.

    if species == "human" {
        deleted_genes.push("IGHV1/OR15-9");
        deleted_genes.push("TRGV11");
        // deleting because nearly identical to TRBV6-2 and not in gtf:
        deleted_genes.push("TRBV6-3");
        allowed_pseudogenes.push("TRAJ8");
        allowed_pseudogenes.push("TRAV35");
        added_genes.push(("TRBD2", "7", 142795705, 142795720, false));
        added_genes.push(("TRAJ15", "14", 22529629, 22529688, false));

        // Not sure why this was here.  It doesn't start with ATG and is rc to
        // another gene perfectly IGKV2D-40 except longer.
        /*
        added_genes2.push( ( "IGKV2-40", "2", 89852177, 89852493,
            89851758, 89851805, false ) );
        */

        added_genes2.push((
            "TRBV11-2", "7", 142433956, 142434001, 142434094, 142434389, true,
        ));
        added_genes2_source.push((
            "TRGV11",
            107142,
            107184,
            107291,
            107604,
            true,
            "AC244625.2".to_string(),
        ));
        right_trims.push(("TRAJ36", -1));
        right_trims.push(("TRAJ37", 3));
        left_trims.push(("IGLJ1", 89));
        left_trims.push(("IGLJ2", 104));
        left_trims.push(("IGLJ3", 113));
        left_trims.push(("TRBV20/OR9-2", 57));
        left_trims.push(("IGHA1", 1));
        left_trims.push(("IGHA2", 1));
        left_trims.push(("IGHE", 1));
        left_trims.push(("IGHG1", 1));
        left_trims.push(("IGHG2", 1));
        left_trims.push(("IGHG4", 1));
        left_trims.push(("IGHM", 1));

        // Add another allele of IGHJ6.

        added_genes_seq.push((
            "IGHJ6",
            "ATTACTACTACTACTACGGTATGGACGTCTGGGGCCAAGGGACCACGGTCACCGTCTCCTCAG",
            false,
        ));

        // Insertion of 3 bases on TRBV20-1 as indicated below.  Note that we use
        // the same name.

        added_genes_seq.push((
            "TRBV20-1",
            "ATGCTGCTGCTTCTGCTGCTTCTGGGGCCAG\
             CAG\
             GCTCCGGGCTTGGTGCTGTCGTCTCTCAACATCCGAGCAGGGTTATCTGTAAGAGTGGAACCTCTGTGAAG\
             ATCGAGTGCCGTTCCCTGGACTTTCAGGCCACAACTATGTTTTGGTATCGTCAGTTCCCGAAACAGAGTCT\
             CATGCTGATGGCAACTTCCAATGAGGGCTCCAAGGCCACATACGAGCAAGGCGTCGAGAAGGACAAGTTTC\
             TCATCAACCATGCAAGCCTGACCTTGTCCACTCTGACAGTGACCAGTGCCCATCCTGAAGACAGCAGCTTC\
             TACATCTGCAGTGCTAGAGA",
            false,
        ));

        // Insertion of 15 bases on TRBV7-7 as indicated below.  Note that we use
        // the same name.

        added_genes_seq.push((
            "TRBV7-7",
            "ATGGGTACCAGTCTCCTATGCTGGGTGGTCCTGGGTTTCCTAGGG\
             ACAGATTCTGTTTCC\
             ACAGATCACACAGGTGCTGGAGTCTCCCAGTCTCCCAGGTACAAAGTCACAAAGAGGGGACAGGATGTAAC\
             TCTCAGGTGTGATCCAATTTCGAGTCATGCAACCCTTTATTGGTATCAACAGGCCCTGGGGCAGGGCCCAG\
             AGTTTCTGACTTACTTCAATTATGAAGCTCAACCAGACAAATCAGGGCTGCCCAGTGATCGGTTCTCTGCA\
             GAGAGGCCTGAGGGATCCATCTCCACTCTGACGATTCAGCGCACAGAGCAGCGGGACTCAGCCATGTATCG\
             CTGTGCCAGCAGCTTAGC",
            false,
        ));

        // Add IGLJ6.

        added_genes_seq.push((
            "IGLJ6",
            "GGAGGGTTTGTGTGCAGGGTTATATCACAGTGTAATGTGTTCGGCAGTGGCACCAAGGTGACCGTCCTCG",
            false,
        ));

        // Add IGLC6.

        added_genes_seq3.push((
            "IGLC6",
            "GTCAGCCCAAGGCTGCCCCATCGGTCACTCTGTTCCCGCCCTCCTCTGAGGAGCTTCAAGCCAACAAGGCCACACTGGTGTGCCTGATCAGTGACTTCTACCCGGGAGCTGTGAAAGTGGCCTGGAAGGCAGATGGCAGCCCCGTCAACACGGGAGTGGAGACCACCACACCCTCCAAACAGAGCAACAACAAGTACGCGGCCAGCAGCTAGCTACCTGAGCCTGACGCCTGAGCAGTGGAAGTCCCACAGAAGCTACAGTTGCCAGGTCACGCATGAAGGGAGCACCGTGGAGAAGACAGTGGCCCCTGCAGAATG",
            false,
        ));
        added_genes_seq3.push((
            "IGLC6",
            "CTCTTAGGCCCCCGACCCTCACCCCACCCACAGGGGCCTGGAGCTGCAGGTTCCCAGGGGAGGGGGTCTCTCTCCCCATCCCAAGTCATCCAGCCCTTCT
",
            true,
        ));

        // Also for cell ranger 7.0.
        // Delete the following human genes from the reference:
        // IGHV4-30-2, IGKV1D-33, IGKV1D-37, IGKV1D-39, IGKV2D-28.
        // These are identical to other genes in the reference, except that the reference provides
        // a longer 5'-UTR in one case. We observe that clonotypes having one of these genes often
        // have heterogeneous assignments between the gene and its duplicate, and that's bad, as
        // it implies that assignment of genes to clonotypes in these cases is effectively random.

        deleted_genes.push("IGHV4-30-2");
        deleted_genes.push("IGKV1D-33");
        deleted_genes.push("IGKV1D-37");
        deleted_genes.push("IGKV1D-39");
        deleted_genes.push("IGKV2D-28");

        // Delete more human genes.  We observe that these are very rarely assigned, and when
        // assigned, apparently incorrect.

        deleted_genes.push("IGLJ4");
        deleted_genes.push("IGLJ5");
        deleted_genes.push("IGJL6");

        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
        // Begin human changes for cell ranger 5.0.
        // (see also mouse changes, below)
        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

        /*
        // 1. Replace IGKV2D-40.  It has a leader sequence of length 9 amino acids, which is an
        // extreme low outlier, and we observe in the whole genome reference and in 10x data a
        // left extension of it whose leader is 20 amino acids long, as expected, and which has a
        // leucine-rich stretch, as expected, unlike the short leader.

        deleted_genes.push("IGKV2D-40");
        added_genes2.push((
            "IGKV2D-40",
            "2",
            89851758,
            89851806,
            89852178,
            89852493,
            true,
        ));
        */

        // 2. Delete IGKV2-18.  We previously added this gene to our reference but it is listed
        // in some places as a pseudogene, and the sequence we provided had a leader of length
        // 9 amino acids, which is an extremely short outlier.  The IMGT sequence for IGKV2-18
        // does not begin with a start codon.  We observe in all cases examined (more than 50),
        // that when IGKV2-18 appears in 10x data, it appears with a heavy chain and ANOTHER light
        // chain, which is odd.  We implemented this change by commenting out the previous
        // addition lines, and moved them here.

        // added_genes2.push((
        //     "IGKV2-18", "2", 89128701, 89129017, 89129435, 89129449, false,
        // ));

        // 3. Delete IGLV5-48.  This is truncated on the right.

        deleted_genes.push("IGLV5-48");

        // 4. Our previouse notes for TRBV21-1 said that our version began with a start codon.
        // That's great, but it has multiple frameshifts, we don't see it in 10x data, and it is
        // annotated as a pseudogene.  Therefore we are "unallowing" it here.  Note that there
        // are two versions.

        // allowed_pseudogenes.push("TRBV21-1");

        // 5. Add a gene that is present in the human reference and in our data, but which
        // we missed.

        // 6. Add a gene that is present in the human reference and in our data, but which
        // we missed.

        added_genes_seq.push(("IGKV1-NL1",
            "ATGGACATGAGGGTCCCCGCTCAGCTCCTGGGGCTCCTGCTGCTCTGGCTCCCAGGTACCAGATGTGACATCCAGATGACCCAGTCTCCATCCTCCCTGTCTGCATCTGTAGGAGACAGAGTCACCATCACTTGCCGGGCGAGTCAGGGCATTAGCAATTCTTTAGCCTGGTATCAGCAGAAACCAGGGAAAGCCCCTAAGCTCCTGCTCTATGCTGCATCCAGATTGGAAAGTGGGGTCCCATCCAGGTTCAGTGGCAGTGGATCTGGGACGGATTACACTCTCACCATCAGCAGCCTGCAGCCTGAAGATTTTGCAACTTATTACTGT", false));

        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
        // End human changes for cell ranger 5.0.
        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

        // After forking repo.
        //
        // Add missing 5' UTRs for human V genes.  These were determined empirically by
        // examining VDJ reads.  Note that transcription start sites are stochastic so there
        // is no one correct answer for 5' UTR sequences.  It would also be better to use
        // reference coordinates for these sequences.

        added_genes_seq.push((
            "IGLV1-36",
            "GAGATTTGGGGGAAGCTCAGCTTCAGCTGCGGGTAGAGAAGACAGGACTCAGGACAATCTCCAGC",
            true,
        ));
        added_genes_seq.push(("IGKV1-NL1", "GGGGAGTCAGACCCTGTCAGGACACAGC", true));
        added_genes_seq.push((
            "IGHV1-69-2",
            "GGGGAGCATCACACAACAGCCACATCCCTCCCCTACAGAAGCCCCCAGAGAGCAGCACCTCACC",
            true,
        ));
        added_genes_seq.push((
            "IGHV3-64D",
            "GGGAGCTCTGGGAGAGGAGCCCCAGGCCCGGGATTCCCAGGTGTTTCCATTCAGTGATCAGCACTGAAGACAGAAGACTCATC",
            true,
        ));
    }
    if species == "mouse" {
        // Doesn't start with start codon, is labeled a pseudogene by NCBI,
        // and not clear that we have an example that has a start codon ahead
        // of the official gene start.

        deleted_genes.push("IGHV1-67");

        // The following V segment is observed in BALB/c datasets 76836 and 77990,
        // although the sequence accession AJ851868.3 is 129S1.

        added_genes2_source.push((
            "IGHV12-1",
            361009,
            361054,
            361145,
            361449,
            true,
            "AJ851868.3".to_string(),
        ));

        // The following V segment is observed in BALB/c datasets 76836 and 77990.
        // The accession is an unplaced sequence in the Sanger assembly of BALB/c,
        // which is also on the Sanger ftp site at
        // ftp://ftp-mouse.sanger.ac.uk/current_denovo.
        // The best match to IMGT is to IGHV1-77 as shown below, and it equally well
        // matches IGHV1-66 and IGHV1-85.

        //           *       *                 *              * ***     **   *  *
        // ATGGAATGGAACTGGGTCGTTCTCTTCCTCCTGTCATTAACTGCAGGTGTCTATGCCCAGGGTCAGATGCAGCAGTCTGG
        // ATGGAATGGAGCTGGGTCTTTCTCTTCCTCCTGTCAGTAACTGCAGGTGTCCACTGCCAGGTCCAGCTGAAGCAGTCTGG
        //
        //                                   * *         *         *        *******
        // AGCTGAGCTGGTGAAGCCTGGGGCTTCAGTGAAGCTGTCCTGCAAGACTTCTGGCTTCACCTTCAGCAGTAGCTATATAA
        // AGCTGAGCTGGTGAAGCCTGGGGCTTCAGTGAAGATATCCTGCAAGGCTTCTGGCTACACCTTCACTGACTACTATATAA
        //
        // **   *       * *          * *             * **    ** *      *    *     **   *
        // GTTGGTTGAAGCAAAAGCCTGGACAGAGTCTTGAGTGGATTGCATGGATTTATGCTGGAACTGGTGGTACTAGCTATAAT
        // ACTGGGTGAAGCAGAGGCCTGGACAGGGCCTTGAGTGGATTGGAAAGATTGGTCCTGGAAGTGGTAGTACTTACTACAAT
        //
        // *         **         **        *     *                        **              *
        // CAGAAGTTCACAGGCAAGGCCCAACTGACTGTAGACACATCCTCCAGCACAGCCTACATGCAATTCAGCAGCCTGACAAC
        // GAGAAGTTCAAGGGCAAGGCCACACTGACTGCAGACAAATCCTCCAGCACAGCCTACATGCAGCTCAGCAGCCTGACATC
        //
        //             **      *
        // TGAGGACTCTGCCATCTATTACTGTGCAAGA
        // TGAGGACTCTGCAGTCTATTTCTGTGCAAGA

        // ◼ Correctly name this sequence.

        added_genes2_source.push((
            "IGHV1-unknown1",
            7084,
            7391,
            7475,
            7517,
            false,
            "LVXK01034187.1".to_string(),
        ));

        // Add form of TRAV4-4-DV10 seen in BALB/c.  Includes a 3-base indel and
        // SNPs.

        added_genes_seq.push((
            "TRAV4-4-DV10",
            "ATGCAGAGGAACCTGGGAGCTGTGCTGGGGATTCTGTGGGTGCAGATTTGCTGGGTGAGAGGGGATCAGG\
             TGGAGCAGAGTCCTTCAGCCCTGAGCCTCCACGAGGGAACCGATTCTGCTCTGAGATGCAATTTTACGACC\
             ACCATGAGGAGTGTGCAGTGGTTCCGACAGAATTCCAGGGGCAGCCTCATCAGTTTGTTCTACTTGGCTTC\
             AGGAACAAAGGAGAATGGGAGGCTAAAGTCAGCATTTGATTCTAAGGAGCGGCGCTACAGCACCCTGCACA\
             TCAGGGATGCCCAGCTGGAGGACTCAGGCACTTACTTCTGTGCTGCTGAGG",
            false,
        ));

        // Add form of TRAV13-1 or TRAV13D-1 seen in BALB/c.  Arbitrarily labeled
        // TRAV13-1.  Has 8 SNPs.

        added_genes_seq.push((
            "TRAV13-1",
            "ATGAACAGGCTGCTGTGCTCTCTGCTGGGGCTTCTGTGCACCCAGGTTTGCTGGGTGAAAGGACAGCAAG\
             TGCAGCAGAGCCCCGCGTCCTTGGTTCTGCAGGAGGGGGAGAATGCAGAGCTGCAGTGTAACTTTTCCACA\
             TCTTTGAACAGTATGCAGTGGTTTTACCAACGTCCTGAGGGAAGTCTCGTCAGCCTGTTCTACAATCCTTC\
             TGGGACAAAGCAGAGTGGGAGACTGACATCCACAACAGTCATCAAAGAACGTCGCAGCTCTTTGCACATTT\
             CCTCCTCCCAGATCACAGACTCAGGCACTTATCTCTGTGCTTTGGAAC",
            false,
        ));

        // Alt splicing, first exon of TRBV12-2 plus second exon of TRBV13-2,
        // very common.

        added_genes_seq.push((
            "TRBV12-2+TRBV13-2",
            "ATGTCTAACACTGCCTTCCCTGACCCCGCCTGGA\
             ACACCACCCTGCTATCTTGGGTTGCTCTCTTTCTCCTGGGAACAAAACACATGGAGGCTGCAGTCACCCAA\
             AGCCCAAGAAACAAGGTGGCAGTAACAGGAGGAAAGGTGACATTGAGCTGTAATCAGACTAATAACCACAA\
             CAACATGTACTGGTATCGGCAGGACACGGGGCATGGGCTGAGGCTGATCCATTATTCATATGGTGCTGGCA\
             GCACTGAGAAAGGAGATATCCCTGATGGATACAAGGCCTCCAGACCAAGCCAAGAGAACTTCTCCCTCATT\
             CTGGAGTTGGCTACCCCCTCTCAGACATCAGTGTACTTCTGTGCCAGCGGTGATG",
            false,
        ));

        // Insertion of 3 bases on TRAV16N as indicated below.  Note that we use
        // the same name.

        added_genes_seq.push((
            "TRAV16N",
            "ATGCTGATTCTAAGCCTGTTGGGAGCAGCCTTTGGCTCCATTTGTTTTGCA\
             GCA\
             ACCAGCATGGCCCAGAAGGTAACACAGACTCAGACTTCAATTTCTGTGGTGGAGAAGACAACGGTGACAAT\
             GGACTGTGTGTATGAAACCCGGGACAGTTCTTACTTCTTATTCTGGTACAAGCAAACAGCAAGTGGGGAAA\
             TAGTTTTCCTTATTCGTCAGGACTCTTACAAAAAGGAAAATGCAACAGTGGGTCATTATTCTCTGAACTTT\
             CAGAAGCCAAAAAGTTCCATCGGACTCATCATCACCGCCACACAGATTGAGGACTCAGCAGTATATTTCTG\
             TGCTATGAGAGAGGG",
            false,
        ));

        // Insertion of 3 bases on TRAV6N-5 as indicated below.  Note that we use
        // the same name.

        added_genes_seq.push((
            "TRAV6N-5",
            "ATGAACCTTTGTCCTGAACTGGGTATTCTACTCTTCCTAATGCTTTTTG\
             GAG\
             AAAGCAATGGAGACTCAGTGACTCAGACAGAAGGCCCAGTGACACTGTCTGAAGGGACTTCTCTGACTGTG\
             AACTGTTCCTATGAAACCAAACAGTACCCAACCCTGTTCTGGTATGTGCAGTATCCCGGAGAAGGTCCACA\
             GCTCCTCTTTAAAGTCCCAAAGGCCAACGAGAAGGGAAGCAACAGAGGTTTTGAAGCTACATACAATAAAG\
             AAGCCACCTCCTTCCACTTGCAGAAAGCCTCAGTGCAAGAGTCAGACTCGGCTGTGTACTACTGTGCTCTG\
             GGTGA",
            false,
        ));

        // Insertion of 15 bases on TRAV13N-4 as indicated below.  Actually this
        // appears to be the only form, so we should probably delete the form
        // we have, but not the UTR.  (Can't just push onto deleted_genes.)

        added_genes_seq.push((
            "TRAV13N-4",
            "ATGAAGAGGCTGCTGTGCTCTCTGCTGGGGCTCCTGTGCACCCAGGTTTGCT\
             GTGCTTCTCAATTAG\
             GGCTGAAAGAACAGCAAGTGCAGCAGAGTCCCGCATCCTTGGTTCTGCAGGAGGCGGAGAACGCAGAGCTC\
             CAGTGTAGCTTTTCCATCTTTACAAACCAGGTGCAGTGGTTTTACCAACGTCCTGGGGGAAGACTCGTCAG\
             CCTGTTGTACAATCCTTCTGGGACAAAGCAGAGTGGGAGACTGACATCCACAACAGTCATTAAAGAACGTC\
             GCAGCTCTTTGCACATTTCCTCCTCCCAGATCACAGACTCAGGCACTTATCTCTGTGCTATGGAAC",
            false,
        ));

        // Insertion of 21 bases on TRBV13-2, as indicated below.  Note that we
        // use the same name.

        added_genes_seq.push((
            "TRBV13-2",
            "ATGGGCTCCAGGCTCTTCTTCGTGCTCTCCAGTCTCCTGTGTTCAA\
             GTTTTGTCTTTCTTTTTATAG\
             AACACATGGAGGCTGCAGTCACCCAAAGCCCAAGAAACAAGGTGGCAGTAACAGGAGGAAAGGTGACATTG\
             AGCTGTAATCAGACTAATAACCACAACAACATGTACTGGTATCGGCAGGACACGGGGCATGGGCTGAGGCT\
             GATCCATTATTCATATGGTGCTGGCAGCACTGAGAAAGGAGATATCCCTGATGGATACAAGGCCTCCAGAC\
             CAAGCCAAGAGAACTTCTCCCTCATTCTGGAGTTGGCTACCCCCTCTCAGACATCAGTGTACTTCTGTGCC\
             AGCGGTGATG",
            false,
        ));

        // Fragment of constant region.  This is from GenBank V01526.1, which
        // points to "The structure of the mouse immunoglobulin in gamma 3 membrane
        // gene segment", Nucleic Acids Res. 1983 Oct 11;11(19):6775-85.  From that
        // article, it appears that the sequence is probably from an A/J mouse.
        // Since 10x only supports B6 and BALB/c mice, it's not clear why we should
        // have this sequence in the reference, however we have an enrichment primer
        // that matches this sequence and none of the other constant regions.
        //
        // This sequence is not long enough to be a full constant region sequence.
        //
        // We have another IGHG3 sequence, so this might be regarded as an alternate
        // allele, however these sequences seem to have no homology.
        //
        // Perhaps this sequence is just wrong.

        added_genes_seq.push((
            "IGHG3",
            "AGCTGGAACTGAATGGGACCTGTGCTGAGGCCCAGGATGGGGAGCTGGACGGGCTCTGGACGACCATCACC\
             ATCTTCATCAGCCTCTTCCTGCTCAGCGTGTGCTACAGCGCCTCTGTCACCCTGTTCAAGGTGAAGTGGAT\
             CTTCTCCTCAGTGGTGCAGGTGAAGCAGACGGCCATCCCTGACTACAGGAACATGATTGGACAAGGTGCC",
            false,
        ));

        // Trim TRAJ49.

        right_trims.push(("TRAJ49", 3));

        // Remove extra first base from a constant region.

        left_trims.push(("IGLC2", 1));

        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
        // Begin mouse changes for cell ranger 5.0.
        // (see also human changes, above)
        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

        // 1. The gene TRAV23 is frameshifted.

        deleted_genes.push("TRAV23");

        // 2. The constant region gene IGHG2B has an extra base at its beginning.  We previously
        // added this sequence, and so we've moved that addition here, and deleted its first base.
        // It is a BALB/c constant region, from GenBank V00763.1.

        added_genes_seq.push((
            "IGHG2B",
            "CCAAAACAACACCCCCATCAGTCTATCCACTGGCCCCTGGGTGTGGAGATACAACTGGTTCCTCCGTGAC\
             CTCTGGGTGCCTGGTCAAGGGGTACTTCCCTGAGCCAGTGACTGTGACTTGGAACTCTGGATCCCTGTCCA\
             GCAGTGTGCACACCTTCCCAGCTCTCCTGCAGTCTGGACTCTACACTATGAGCAGCTCAGTGACTGTCCCC\
             TCCAGCACCTGGCCAAGTCAGACCGTCACCTGCAGCGTTGCTCACCCAGCCAGCAGCACCACGGTGGACAA\
             AAAACTTGAGCCCAGCGGGCCCATTTCAACAATCAACCCCTGTCCTCCATGCAAGGAGTGTCACAAATGCC\
             CAGCTCCTAACCTCGAGGGTGGACCATCCGTCTTCATCTTCCCTCCAAATATCAAGGATGTACTCATGATC\
             TCCCTGACACCCAAGGTCACGTGTGTGGTGGTGGATGTGAGCGAGGATGACCCAGACGTCCAGATCAGCTG\
             GTTTGTGAACAACGTGGAAGTACACACAGCTCAGACACAAACCCATAGAGAGGATTACAACAGTACTATCC\
             GGGTGGTCAGCACCCTCCCCATCCAGCACCAGGACTGGATGAGTGGCAAGGAGTTCAAATGCAAGGTGAAC\
             AACAAAGACCTCCCATCACCCATCGAGAGAACCATCTCAAAAATTAAAGGGCTAGTCAGAGCTCCACAAGT\
             ATACACTTTGCCGCCACCAGCAGAGCAGTTGTCCAGGAAAGATGTCAGTCTCACTTGCCTGGTCGTGGGCT\
             TCAACCCTGGAGACATCAGTGTGGAGTGGACCAGCAATGGGCATACAGAGGAGAACTACAAGGACACCGCA\
             CCAGTTCTTGACTCTGACGGTTCTTACTTCATATATAGCAAGCTCAATATGAAAACAAGCAAGTGGGAGAA\
             AACAGATTCCTTCTCATGCAACGTGAGACACGAGGGTCTGAAAAATTACTACCTGAAGAAGACCATCTCCC\
             GGTCTCCGGGTAAA",
            false,
        ));

        // 3. The gene IGKV12-89 shows a six base insertion in all 10x data, so we insert it here.

        deleted_genes.push("IGKV12-89");
        added_genes2.push((
            "IGKV12-89",
            "6",
            68834846,
            68835149,
            68835268,
            68835307,
            false,
        ));

        // 4. Fix a gene for which the canonical C at the end of FWR3 is seen as S.  In all our
        // data, we see C.  This is a single base change, except that we've truncated after the C.
        // Also we switched to the GRCm38 version.
        // The space after the gene name is to work around a crash.

        deleted_genes.push("IGHV8-9");
        added_genes_seq.push((
            "IGHV8-9 ",
            "ATGGACAGGCTTACTTCCTCATTCCTACTGCTGATTGTCCCTGTCTATGTCCTATCCCAGGTTACTCTGAAAGAGTCTGGCCCTGGTATATTGCAGCCCTCCCAGACCCTCAGTCTGACCTGTTCTTTCTCTGTGTTTTCACTGAGCACTTTTGGTATGGGTGTGAGCTGGATTCGTCAGCCTTCAGGGAAGGGTCTGGAGTGGCTGGCACACATTTATTGGGATGAGGACAAGCACTATAAACCATCCTTGAAGAGCCGGCTCACAATCTCCAAGGATACCTCCAACAACCAGGTATTCCTCAAGATCACCACTGTGGACACTGCAGATACTGCCACATACTACTGT",
            false,
        ));

        // 5. Add a missing allele of IGKV2-109.  This is in GRCm38 and in 10x data.

        added_genes_seq.push((
            "IGKV2-109",
            "ATGAGGTTCTCTGCTCAGCTTCTGGGGCTGCTTGTGCTCTGGATCCCTGGATCCACTGCAGATATTGTGATGACGCAGGCTGCCTTCTCCAATCCAGTCACTCTTGGAACATCAGCTTCCATCTCCTGCAGGTCTAGTAAGAATCTCCTACATAGTAATGGCATCACTTATTTGTATTGGTATCTGCAGAGGCCAGGCCAGTCTCCTCAGCTCCTGATATATCGGGTGTCCAATCTGGCCTCAGGAGTCCCAAACAGGTTCAGTGGCAGTGAGTCAGGAACTGATTTCACACTGAGAATCAGCAGAGTGGAGGCTGAGGATGTGGGTGTTTATTACTGT",
            false,
        ));

        // 6. Add missing gene IGKV4-56.  This is in GRCm38 and in 10x data.

        added_genes_seq.push((
            "IGKV4-56",
            "ATGGATTTTCAGGTGCAGATTTTCAGCTTCCTGCTAATCAGCAGAGTCATACTGTCCAGAGGACAAATTGTTCTCACCCAGTCTCCAGCAATCATGTCTGCATCTCCAGGGCAGAAAGTCACCATAACCTGCAGTGCCATCTCAAGTGTAAATTACATGCACTGGTACCAGCAGAAGCCAGGATCCTCCCCCAAACTCTGGATTTATGCAACATCCAAACTGGCTCTTGGAGTCCCTGCTTGCTTCAGTGGCAGTGGGTCTGGGACCTCTTACTCTCTCACAATCAGCAGCATGGTGGCTGAAGATGCCACCTCTTATTTCTGT",
            false,
        ));

        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
        // End mouse changes for cell ranger 5.0.
        // ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
    }
    if species == "balbc" {}

    // Normalize exceptions.

    deleted_genes.sort_unstable();
    allowed_pseudogenes.sort_unstable();
    left_trims.sort_unstable();
    right_trims.sort_unstable();

    // Define a function that returns the ensembl path for a particular dataset.
    // Note that these are for the ungzipped versions.

    fn ensembl_path(
        species: &str, // human or mouse or balbc
        ftype: &str,   // gff3 or gtf or fasta
        release: i32,  // release number
    ) -> String {
        let species_name = match species {
            "human" => "homo_sapiens",
            "mouse" => "mus_musculus",
            "balbc" => "mus_musculus_balbcj",
            _ => "",
        };
        let releasep = format!("release-{}", release);
        let csn = cap1(species_name);
        match (species, ftype) {
            ("mouse", "gff3") => format!(
                "{}/{}/{}/{}.GRCm38.{}.gff3",
                releasep, ftype, species_name, csn, release
            ),
            ("mouse", "gtf") => format!(
                "{}/{}/{}/{}.GRCm38.{}.gtf",
                releasep, ftype, species_name, csn, release
            ),
            ("mouse", "fasta") => format!(
                "{}/{}/{}/dna/{}.GRCm38.dna.toplevel.fa",
                releasep, ftype, species_name, csn
            ),

            ("balbc", "gff3") => format!(
                "{}/{}/{}/{}.BALB_cJ_v1.{}.gff3",
                releasep, ftype, species_name, csn, release
            ),
            ("balbc", "gtf") => format!(
                "{}/{}/{}/{}.BALB_cJ_v1.{}.gtf",
                releasep, ftype, species_name, csn, release
            ),
            ("balbc", "fasta") => format!(
                "{}/{}/{}/dna/{}.BALB_cJ_v1.dna.toplevel.fa",
                releasep, ftype, species_name, csn
            ),

            ("human", "gff3") => format!(
                "{}/{}/{}/{}.GRCh38.{}.chr_patch_hapl_scaff.gff3",
                releasep, ftype, species_name, csn, release
            ),
            ("human", "gtf") => format!(
                "{}/{}/{}/{}.GRCh38.{}.chr_patch_hapl_scaff.gtf",
                releasep, ftype, species_name, csn, release
            ),
            ("human", "fasta") => format!(
                "{}/{}/{}/dna/{}.GRCh38.dna.toplevel.fa",
                releasep, ftype, species_name, csn
            ),
            _ => String::default(),
        }
    }

    // Download files from ensembl site if requested.  Not fully tested, and it
    // would appear that a git command can fail without causing this code to panic.
    // This can't be fully tested since we don't want to run the git commands as
    // an experiment.
    // Note that the human fasta file would be 54 GB if uncompressed (owing to
    // gigantic runs of ends), so we don't uncompress fasta files.

    if download {
        fn fetch(species: &str, ftype: &str, release: i32, internal: &str) {
            println!("fetching {}.{}", species, ftype);
            let path = ensembl_path(species, ftype, release);
            let external = "ftp://ftp.ensembl.org/pub";
            let dir = format!("{}/{}", internal, path.rev_before("/"));
            fs::create_dir_all(&dir).unwrap();
            let full_path = format!("{}/{}", internal, path);
            Command::new("wget")
                .current_dir(&dir)
                .arg(format!("{}/{}.gz", external, path))
                .status()
                .expect("wget failed");
            if ftype != "fasta" {
                Command::new("gunzip")
                    .arg(&full_path)
                    .status()
                    .expect("gunzip failed");
            }
            // Code specific to 10x commented out.
            /*
            Command::new("git")
                .current_dir(&internal)
                .arg("add")
                .arg(&path)
                .status()
                .expect("git add failed");
            Command::new("git")
                .current_dir(&internal)
                .arg("commit")
                .arg(path)
                .status()
                .expect("git commit failed");
            */
        }
        // ◼ Add balbc if we're going ot use it.
        for species in ["human", "mouse"].iter() {
            for ftype in ["gff3", "gtf", "fasta"].iter() {
                fetch(species, ftype, release, internal);
            }
        }
        std::process::exit(0);
    }

    // Define root output directory.

    let root = "vdj_ann_ref/vdj_refs";
    let mut out = open_for_write_new![&format!("{}/{}/fasta/regions.fa", root, species)];

    // Define input filenames.

    let gtf = format!("{}/{}", internal, ensembl_path(species, "gtf", release));
    let fasta = format!(
        "{}/{}.gz",
        internal,
        ensembl_path(species, "fasta", release)
    );

    // Generate reference.json.  Note version number.

    let mut json = open_for_write_new![&format!("{}/{}/reference.json", root, species)];
    fwriteln!(json, "{{");
    let mut sha256 = Sha256::new();
    copy(&mut File::open(&fasta).unwrap(), &mut sha256).unwrap();
    let hash = sha256.finalize();
    fwriteln!(json, r###"    "fasta_hash": "{:x}","###, hash);
    fwriteln!(json, r###"    "genomes": "{}","###, source2);
    let mut sha256 = Sha256::new();
    copy(&mut File::open(&gtf).unwrap(), &mut sha256).unwrap();
    let hash = sha256.finalize();
    fwriteln!(json, r###"    "gtf_hash": "{:x}","###, hash);
    fwriteln!(
        json,
        r###"    "input_fasta_files": "{}","###,
        ensembl_path(species, "fasta", release)
    );
    fwriteln!(
        json,
        r###"    "input_gtf_files": "{}","###,
        ensembl_path(species, "gtf", release)
    );
    fwriteln!(json, r###"    "mkref_version": "","###);
    fwriteln!(json, r###"    "type": "V(D)J Reference","###);
    fwriteln!(json, r###"    "version": "{}""###, version);
    fwrite!(json, "}}");

    // Load the gff3 file and use it to do two things:
    //
    // 1. Remove genes classed as non-functional.
    //    ◼ Except that we don't.  To consider later.
    //
    // 2. Convert gene names into the standard format.  This would not be needed,
    //    except that in the gtf file, for genes present only on alternate loci,
    //    only an accession identifier is given (in some and perhaps all cases).

    let gff3 = format!("{}/{}", internal, ensembl_path(species, "gff3", release));
    let mut demangle = HashMap::<String, String>::new();
    let f = open_for_read![&gff3];
    for line in f.lines() {
        let s = line.unwrap();
        let fields: Vec<&str> = s.split_terminator('\t').collect();
        if fields.len() < 9 {
            continue;
        }
        if fields[2] != "gene" && fields[2] != "pseudogene" {
            continue;
        }
        let fields8: Vec<&str> = fields[8].split_terminator(';').collect();
        let (mut gene, mut gene2) = (String::new(), String::new());
        let mut biotype = String::new();
        for i in 0..fields8.len() {
            if fields8[i].starts_with("Name=") {
                gene = fields8[i].after("Name=").to_string();
            }
            if fields8[i].starts_with("description=") {
                gene2 = fields8[i].after("description=").to_string();
            }
            if fields8[i].starts_with("biotype=") {
                biotype = fields8[i].after("biotype=").to_string();
            }
        }

        // Test for appropriate gene type.
        // Note that we allow V and J pseudogenes, but only by explicit inclusion.

        if biotype != "TR_V_gene"
            && biotype != "TR_D_gene"
            && biotype != "TR_J_gene"
            && biotype != "TR_C_gene"
            && biotype != "TR_V_pseudogene"
            && biotype != "TR_J_pseudogene"
            && biotype != "IG_V_gene"
            && biotype != "IG_D_gene"
            && biotype != "IG_J_gene"
            && biotype != "IG_C_gene"
            && biotype != "IG_V_pseudogene"
            && biotype != "IG_J_pseudogene"
        {
            continue;
        }

        // Sanity check.

        if !gene2.starts_with("T cell receptor ")
            && !gene2.starts_with("T-cell receptor ")
            && !gene2.starts_with("immunoglobulin ")
            && !gene2.starts_with("Immunoglobulin ")
        {
            continue;
            // println!( "problem with gene = '{}', gene2 = '{}'", gene, gene2 );
        }

        // Maybe exclude nonfunctional.

        let exclude_non_functional = false;
        if exclude_non_functional && gene2.contains("(non-functional)") {
            continue;
        }

        // Fix gene.

        gene = gene.to_uppercase();
        gene = gene.replace("TCR", "TR");
        gene = gene.replace("BCR", "BR");
        gene = gene.replace("G-", "G");

        // Fix gene2.

        gene2 = gene2.replace("  ", " ");
        gene2 = gene2.replace("%2C", "");
        gene2 = gene2.replace("T cell receptor ", "TR");
        gene2 = gene2.replace("T-cell receptor ", "TR");
        gene2 = gene2.replace("immunoglobulin ", "IG");
        gene2 = gene2.replace("Immunoglobulin ", "IG");
        gene2 = gene2.replace("variable V", "V");

        // More fixing.  Replace e.g. "alpha " by A.

        for x in [
            "alpha",
            "beta",
            "gamma",
            "delta",
            "epsilon",
            "kappa",
            "lambda",
            "mu",
            "variable",
            "diversity",
            "joining",
            "constant",
            "heavy",
        ]
        .iter()
        {
            gene2 = gene2.replace(&format!("{} ", x), &x[0..1].to_uppercase());
        }

        // More fixing.

        gene2 = gene2.replace("region ", "");
        gene2 = gene2.replace("novel ", "");
        gene2 = gene2.replace("chain ", "");
        if gene2.contains('[') {
            gene2 = gene2.before("[").to_string();
        }
        if gene2.contains('(') {
            gene2 = gene2.before("(").to_string();
        }
        gene2 = gene2.replace(' ', "");
        if gene2.contains("identical") || gene2.contains("identicle") {
            continue;
        }
        gene2 = gene2.to_uppercase();

        // Ignore certain genes.

        if (biotype == "TR_V_pseudogene"
            || biotype == "TR_J_pseudogene"
            || biotype == "IG_V_pseudogene"
            || biotype == "IG_J_pseudogene")
            && !bin_member(&allowed_pseudogenes, &gene2.as_str())
        {
            continue;
        }
        if bin_member(&deleted_genes, &gene2.as_str()) {
            continue;
        }

        // Save result.

        demangle.insert(gene.clone(), gene2.clone());
    }

    // Parse the gtf file.

    let mut exons = Vec::<(String, String, String, i32, i32, String, bool, String)>::new();
    parse_gtf_file(&gtf, &demangle, &mut exons);

    // Find the chromosomes that we're using.

    let mut all_chrs = Vec::<String>::new();
    for k in 0..exons.len() {
        all_chrs.push(exons[k].2.clone());
    }
    unique_sort(&mut all_chrs);

    // Load fasta.  We only load the records that we need.  This is still slow
    // and it might be possible to speed it up.
    // ◼ Put this 'selective fasta loading' into its own function.

    println!("{:.1} seconds used, loading fasta", elapsed(&t));
    let mut refs = Vec::<DnaString>::new();
    let mut rheaders = Vec::<String>::new();
    let gz = MultiGzDecoder::new(std::fs::File::open(&fasta).unwrap());
    let f = BufReader::new(gz);
    let mut last: String = String::new();
    let mut using = false;
    for line in f.lines() {
        let s = line.unwrap();
        if s.starts_with('>') {
            if using {
                refs.push(DnaString::from_dna_string(&last));
                last.clear();
            }
            if rheaders.len() == all_chrs.len() {
                break;
            }
            let mut h = s.get(1..).unwrap().to_string();
            if h.contains(' ') {
                h = h.before(" ").to_string();
            }
            if bin_member(&all_chrs, &h) {
                rheaders.push(h.clone());
                using = true;
            } else {
                using = false;
            }
        } else if using {
            last += &s
        }
    }
    if using {
        refs.push(DnaString::from_dna_string(&last));
    }
    let mut to_chr = HashMap::new();
    for i in 0..rheaders.len() {
        to_chr.insert(rheaders[i].clone(), i);
    }

    // Get the DNA sequences for the exons.

    println!("{:.1} seconds used, getting exon seqs", elapsed(&t));
    let mut dna = Vec::<DnaString>::new();
    for i in 0..exons.len() {
        let chr = &exons[i].2;
        let chrid = to_chr[chr];
        let (start, stop) = (exons[i].3, exons[i].4);
        let seq = refs[chrid].slice(start as usize, stop as usize).to_owned();
        dna.push(seq);
    }

    // Remove transcripts having identical sequences, or which are identical except for trailing
    // bases.  The shorter transcript is deleted.

    println!(
        "{:.1} seconds used, checking for nearly identical transcripts",
        elapsed(&t)
    );
    let mut to_delete = vec![false; exons.len()];
    let mut i = 0;
    let mut dnas = Vec::<(Vec<DnaString>, usize, usize)>::new();
    while i < exons.len() {
        let j = next_diff12_8(&exons, i as i32) as usize;
        let mut x = Vec::<DnaString>::new();
        for k in i..j {
            x.push(dna[k].clone());
        }
        if !exons[i].6 {
            x.reverse();
            for k in 0..x.len() {
                x[k] = x[k].rc().to_owned();
            }
        }
        dnas.push((x, i, j));
        i = j;
    }
    dnas.sort();
    for i in 1..dnas.len() {
        let n = dnas[i].0.len();
        if dnas[i - 1].0.len() == n {
            let mut semi = true;
            for j in 0..n - 1 {
                if dnas[i].0[j] != dnas[i - 1].0[j] {
                    semi = false;
                    break;
                }
            }
            if semi {
                let mut matches = true;
                let x1 = &dnas[i - 1].0[n - 1];
                let x2 = &dnas[i].0[n - 1];
                let k = std::cmp::min(x1.len(), x2.len());
                for p in 0..k {
                    if x1.get(p) != x2.get(p) {
                        matches = false;
                        break;
                    }
                }
                if matches {
                    let (r, s) = (dnas[i - 1].1, dnas[i - 1].2);
                    for k in r..s {
                        to_delete[k] = true;
                    }
                }
            }
        }
    }
    erase_if(&mut exons, &to_delete);

    // Build fasta.

    println!("{:.1} seconds used, building fasta", elapsed(&t));
    let mut i = 0;
    let mut record = 0;
    while i < exons.len() {
        let j = next_diff12_8(&exons, i as i32) as usize;
        let mut fws = Vec::<bool>::new();
        for k in i..j {
            fws.push(exons[k].6);
        }
        unique_sort(&mut fws);
        assert!(fws.len() == 1);
        let fw = fws[0];
        let gene = &exons[i].0;
        if bin_member(&excluded_genes, &gene.as_str()) {
            i = j;
            continue;
        }

        // The gene may appear on more than one record.  We pick the one that
        // is lexicographically minimal.  This should favor numbered chromosomes
        // over alt loci.
        // ◼ NOT SURE WHAT THIS IS DOING NOW.

        let mut chrs = Vec::<String>::new();
        for k in i..j {
            chrs.push(exons[k].2.clone());
        }
        unique_sort(&mut chrs);
        let chr = chrs[0].clone();
        let chrid = to_chr[&chr.to_string()];

        // Build the 5' UTR for V, if there is one.  We allow for the possibility
        // that there is an intron in the UTR, although this is very rare (once in
        // human TCR).

        let mut seq = DnaString::new();
        let trid = &exons[i].7;
        for k in i..j {
            if exons[k].2 != chr {
                continue;
            }
            let (start, stop) = (exons[k].3, exons[k].4);
            let cat = &exons[k].5;
            if cat == "five_prime_utr" {
                let seqx = refs[chrid].slice(start as usize, stop as usize);
                for i in 0..seqx.len() {
                    seq.push(seqx.get(i));
                }
            }
        }
        if !seq.is_empty() {
            let header = header_from_gene(gene, true, false, &mut record, trid);
            print_oriented_fasta(&mut out, &header, &seq.slice(0, seq.len()), fw, none);
        }

        // Build the 3' UTR for constant region gene, if there is one.  We allow for the
        // possibility that there is an intron in the UTR.

        let mut seq = DnaString::new();
        let trid = &exons[i].7;
        for k in i..j {
            if exons[k].2 != chr {
                continue;
            }
            let (start, stop) = (exons[k].3, exons[k].4);
            let cat = &exons[k].5;
            if cat == "three_prime_utr" {
                let seqx = refs[chrid].slice(start as usize, stop as usize);
                for i in 0..seqx.len() {
                    seq.push(seqx.get(i));
                }
            }
        }
        if !seq.is_empty() {
            let header = header_from_gene(gene, false, true, &mut record, trid);
            print_oriented_fasta(&mut out, &header, &seq.slice(0, seq.len()), fw, none);
        }

        // Build L+V segment.
        // ◼ To do: separately track L.  Do not require that transcripts include L.

        if gene.starts_with("TRAV")
            || gene.starts_with("TRBV")
            || gene.starts_with("TRDV")
            || gene.starts_with("TRGV")
            || gene.starts_with("IGHV")
            || gene.starts_with("IGKV")
            || gene.starts_with("IGLV")
        {
            let mut seq = DnaString::new();
            let mut ncodons = 0;
            for k in i..j {
                if exons[k].2 != chr {
                    continue;
                }
                let (start, stop) = (exons[k].3, exons[k].4);
                let cat = &exons[k].5;
                if cat == "CDS" {
                    ncodons += 1;
                    let seqx = refs[chrid].slice(start as usize, stop as usize);
                    for i in 0..seqx.len() {
                        seq.push(seqx.get(i));
                    }
                }
            }
            if !seq.is_empty() {
                let header = header_from_gene(gene, false, false, &mut record, trid);
                let mut seqx = seq.clone();
                if !fw {
                    seqx = seqx.rc();
                }
                let p = bin_position1_2(&right_trims, &gene.as_str());
                // negative right_trims incorrectly handled, to fix make code
                // same as for J
                let mut n = seq.len() as i32;
                if p >= 0 {
                    n -= right_trims[p as usize].1;
                }
                let mut m = 0;
                let p = bin_position1_2(&left_trims, &gene.as_str());
                if p >= 0 {
                    m = left_trims[p as usize].1;
                }

                // Save.  Mostly we require two exons.

                let standard = gene.starts_with("TRAV")
                    || gene.starts_with("TRBV")
                    || gene.starts_with("IGHV")
                    || gene.starts_with("IGKV")
                    || gene.starts_with("IGLV");
                if ncodons == 2 || !standard {
                    print_fasta(&mut out, &header, &seqx.slice(m, n as usize), none);
                } else {
                    record -= 1;
                }
            }
        }

        // Build J and D segments.

        if (gene.starts_with("TRAJ")
            || gene.starts_with("TRBJ")
            || gene.starts_with("TRDJ")
            || gene.starts_with("TRGJ")
            || gene.starts_with("IGHJ")
            || gene.starts_with("IGKJ")
            || gene.starts_with("IGLJ")
            || gene.starts_with("TRBD")
            || gene.starts_with("TRDD")
            || gene.starts_with("IGHD"))
            && gene != "IGHD"
        {
            let mut using = Vec::<usize>::new();
            for k in i..j {
                if exons[k].2 == chr && exons[k].5 != "five_prime_utr" {
                    using.push(k);
                }
            }
            if using.len() != 1 {
                eprintln!("\nProblem with {}, have {} exons.", gene, using.len());
                eprintln!("This needs to be fixed, failing.\n");
                std::process::exit(1);
            }
            // assert_eq!( using.len(), 1 );
            let k = using[0];
            let start = exons[k].3;
            let mut stop = exons[k].4;
            let p = bin_position1_2(&right_trims, &gene.as_str());
            if p >= 0 && right_trims[p as usize].1 < 0 {
                stop -= right_trims[p as usize].1;
            }
            let seq = refs[chrid].slice(start as usize, stop as usize);
            let mut n = seq.len() as i32;
            if p >= 0 && right_trims[p as usize].1 > 0 {
                n -= right_trims[p as usize].1;
            }
            let mut m = 0;
            let p = bin_position1_2(&left_trims, &gene.as_str());
            if p >= 0 {
                m = left_trims[p as usize].1;
            }
            let header = header_from_gene(gene, false, false, &mut record, trid);
            let seqx = seq.clone();
            print_oriented_fasta(&mut out, &header, &seqx.slice(m, n as usize), fw, none);
        }

        // Build C segments.  Extend by three bases if that adds a TAG or TGA stop codon.

        if gene.starts_with("TRAC")
            || gene.starts_with("TRBC")
            || gene.starts_with("TRDC")
            || gene.starts_with("TRGC")
            || gene.starts_with("IGKC")
            || gene.starts_with("IGLC")
            || gene.starts_with("IGHG")
            || gene == "IGHD"
            || gene == "IGHE"
            || gene == "IGHM"
            || gene.starts_with("IGHA")
        {
            let mut gene = gene.to_string();
            if gene.starts_with("TRGCC") {
                gene = format!("TRG{}", gene.after("TRGC"));
            }
            let mut seq = DnaString::new();
            let mut exons_keep = Vec::<usize>::new();
            for k in i..j {
                if exons[k].2 != chr {
                    continue;
                }
                if exons[k].5 == "three_prime_utr" {
                    continue;
                }
                exons_keep.push(k);
            }
            for m in 0..exons_keep.len() {
                let k = exons_keep[m];
                let (mut start, mut stop) = (exons[k].3, exons[k].4);
                if fw && m == exons_keep.len() - 1 {
                    if refs[chrid]
                        .slice(stop as usize, (stop + 3) as usize)
                        .ascii()
                        == b"TAG"
                    {
                        stop += 3;
                    }
                    if refs[chrid]
                        .slice(stop as usize, (stop + 3) as usize)
                        .ascii()
                        == b"TGA"
                    {
                        stop += 3;
                    }
                }
                if !fw && m == 0 {
                    if refs[chrid]
                        .slice((start - 3) as usize, start as usize)
                        .ascii()
                        == b"CTA"
                    {
                        start -= 3;
                    }
                    if refs[chrid]
                        .slice((start - 3) as usize, start as usize)
                        .ascii()
                        == b"TCA"
                    {
                        start -= 3;
                    }
                }
                let seqx = refs[chrid].slice(start as usize, stop as usize);
                for i in 0..seqx.len() {
                    seq.push(seqx.get(i));
                }
            }
            let mut m = 0;
            let p = bin_position1_2(&left_trims, &gene.as_str());
            if p >= 0 {
                m = left_trims[p as usize].1;
            }
            let header = header_from_gene(&gene, false, false, &mut record, trid);
            if fw {
                print_oriented_fasta(&mut out, &header, &seq.slice(m, seq.len()), fw, none);
            } else {
                print_oriented_fasta(&mut out, &header, &seq.slice(0, seq.len() - m), fw, none);
            }
        }

        // Advance.

        i = j;
    }

    // Add genes.

    println!("{:.1} seconds used, adding genes", elapsed(&t));
    for i in 0..added_genes.len() {
        add_gene(
            &mut out,
            added_genes[i].0,
            &mut record,
            added_genes[i].1,
            added_genes[i].2,
            added_genes[i].3,
            &to_chr,
            &refs,
            none,
            added_genes[i].4,
            false,
            &source,
        );
    }
    for i in 0..added_genes2.len() {
        add_gene2(
            &mut out,
            added_genes2[i].0,
            &mut record,
            added_genes2[i].1,
            added_genes2[i].2,
            added_genes2[i].3,
            added_genes2[i].4,
            added_genes2[i].5,
            &to_chr,
            &refs,
            none,
            added_genes2[i].6,
            &source,
        );
    }
    for i in 0..added_genes2_source.len() {
        let gene = &added_genes2_source[i].0;
        let start1 = added_genes2_source[i].1;
        let stop1 = added_genes2_source[i].2;
        let start2 = added_genes2_source[i].3;
        let stop2 = added_genes2_source[i].4;
        let fw = added_genes2_source[i].5;
        let source = &added_genes2_source[i].6;
        let mut seq = DnaString::new();
        load_genbank_accession(source, &mut seq);
        let seq1 = seq.slice(start1 - 1, stop1);
        let seq2 = seq.slice(start2 - 1, stop2);
        let mut seq = seq1.to_owned();
        for i in 0..seq2.len() {
            seq.push(seq2.get(i));
        }
        if !fw {
            seq = seq.rc();
        }
        let header = header_from_gene(gene, false, false, &mut record, source);
        print_fasta(&mut out, &header, &seq.slice(0, seq.len()), none);
    }
    for i in 0..added_genes_seq.len() {
        let gene = &added_genes_seq[i].0;
        let seq = DnaString::from_dna_string(added_genes_seq[i].1);
        let is_5utr = added_genes_seq[i].2;
        let header = header_from_gene(gene, is_5utr, false, &mut record, &source);
        print_fasta(&mut out, &header, &seq.slice(0, seq.len()), none);
    }
    for i in 0..added_genes_seq3.len() {
        let gene = &added_genes_seq3[i].0;
        let seq = DnaString::from_dna_string(added_genes_seq3[i].1);
        let is_3utr = added_genes_seq3[i].2;
        let header = header_from_gene(gene, false, is_3utr, &mut record, &source);
        print_fasta(&mut out, &header, &seq.slice(0, seq.len()), none);
    }
}
