// Copyright (c) 2018 10x Genomics, Inc. All rights reserved.

// Functions print_tabular and print_tabular_vbox for making pretty tables.  And related utilities.

use io_utils::eprintme;
use itertools::Itertools;
use std::cmp::{max, min};
use string_utils::strme;

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

// Package characters with ANSI escape codes that come before them.

pub fn package_characters_with_escapes(c: &[u8]) -> Vec<Vec<u8>> {
    let mut x = Vec::<Vec<u8>>::new();
    let mut escaped = false;
    let mut package = Vec::<u8>::new();
    for b in c.iter() {
        if escaped && *b != b'm' {
            package.push(*b);
        } else if *b == b'' {
            escaped = true;
            package.push(*b);
        } else if escaped && *b == b'm' {
            escaped = false;
            package.push(*b);
        } else {
            package.push(*b);
            x.push(package.clone());
            package.clear();
        }
    }
    x
}

pub fn package_characters_with_escapes_char(c: &[char]) -> Vec<Vec<char>> {
    let mut x = Vec::<Vec<char>>::new();
    let mut escaped = false;
    let mut package = Vec::<char>::new();
    for b in c.iter() {
        if escaped && *b != 'm' {
            package.push(*b);
        } else if *b == '' {
            escaped = true;
            package.push(*b);
        } else if escaped && *b == 'm' {
            escaped = false;
            package.push(*b);
        } else {
            package.push(*b);
            x.push(package.clone());
            package.clear();
        }
    }
    x
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

// Print out a matrix, with left-justified entries, and given separation between
// columns.  (Justification may be changed by supplying an optional argument
// consisting of a string of l's and r's.)

pub fn print_tabular(
    log: &mut Vec<u8>,
    rows: &[Vec<String>],
    sep: usize,
    justify: Option<Vec<u8>>,
) {
    let just = match justify {
        Some(x) => x,
        None => Vec::<u8>::new(),
    };
    let nrows = rows.len();
    let mut ncols = 0;
    for i in 0..nrows {
        ncols = max(ncols, rows[i].len());
    }
    let mut maxcol = vec![0; ncols];
    for i in 0..rows.len() {
        for j in 0..rows[i].len() {
            maxcol[j] = max(maxcol[j], rows[i][j].chars().count());
        }
    }
    for i in 0..rows.len() {
        for j in 0..rows[i].len() {
            let x = rows[i][j].clone();
            if j < just.len() && just[j] == b'r' {
                log.append(&mut vec![b' '; maxcol[j] - x.chars().count()]);
                log.append(&mut x.as_bytes().to_vec());
                if j < rows[i].len() - 1 {
                    log.append(&mut vec![b' '; sep]);
                }
            } else {
                log.append(&mut x.as_bytes().to_vec());
                if j < rows[i].len() - 1 {
                    log.append(&mut vec![b' '; maxcol[j] - x.chars().count() + sep]);
                }
            }
        }
        log.push(b'\n');
    }
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

// Compute the visible length of a string, counting unicode characters as width one and
// ignoring some ASCII escape sequences.

pub fn visible_width(s: &str) -> usize {
    if s == "\\ext" || s == "\\hline" {
        return 0;
    }
    let mut n = 0;
    let mut escaped = false;
    for c in s.chars() {
        if escaped && c != 'm' {
        } else if c == '' {
            escaped = true;
        } else if escaped && c == 'm' {
            escaped = false;
        } else {
            n += 1;
        }
    }
    n
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

// Print out a matrix, with given separation between columns.  Rows of the matrix
// may contain arbitrary UTF-8 and some escape sequences.  Put the entire thing in a box, with
// extra vertical bars.  The argument justify consists of symbols l and r, denoting
// left and right justification for given columns, respectively, and the symbol | to
// denote a vertical bar.
//
// There is no separation printed on the far left or far right.
//
// By a "matrix entry", we mean one of the Strings in "rows".
//
// Entries that begin with a backslash are reserved for future features.
// Symbols other than l or r or | in "justify" are reserved for future features.
//
// An entry may be followed on the right by one more entries whose contents are
// exactly "\ext".  In that case the entries are treated as multi-column.  Padding
// is inserted as needed on the "right of the multicolumn".
//
// An entry may be "\hline", which gets you a horizontal line.  The normal use case is to
// use one or more of these in succession horizontally to connect two vertical lines.  Cannot
// be combined with \ext.
//
// bold_box: use bold box characters
//
// Really only guaranteed to work for the tested cases.

pub fn print_tabular_vbox(
    log: &mut String,
    rows: &[Vec<String>],
    sep: usize,
    justify: &[u8],
    debug_print: bool,
    bold_box: bool,
) {
    // If you've added a test that fails and are trying to get it work, temporarily change
    // the next to the last entry in the print_tabular_vbox line for the test to true.

    // Define box characters.

    let dash = if !bold_box { '─' } else { '━' };
    let verty = if !bold_box { '│' } else { '┃' };
    let topleft = if !bold_box { '┌' } else { '┏' };
    let topright = if !bold_box { '┐' } else { '┓' };
    let botleft = if !bold_box { '└' } else { '┗' };
    let botright = if !bold_box { '┘' } else { '┛' };
    let tee = if !bold_box { '┬' } else { '┳' };
    let uptee = if !bold_box { '┴' } else { '┻' };
    let cross = if !bold_box { '┼' } else { '╋' };
    let lefty = if !bold_box { '├' } else { '┣' };
    let righty = if !bold_box { '┤' } else { '┫' };

    // Proceed.

    let mut rrr = rows.to_owned();
    let nrows = rrr.len();
    let mut ncols = 0;
    for i in 0..nrows {
        ncols = max(ncols, rrr[i].len());
    }
    let mut vert = vec![false; ncols];
    let mut just = Vec::<u8>::new();
    let mut count = 0_isize;
    for i in 0..justify.len() {
        if justify[i] == b'|' {
            assert!(count > 0);
            if count >= ncols as isize {
                eprintln!("\nposition of | in justify string is illegal");
                eprintme!(count, ncols);
            }
            assert!(count < ncols as isize);
            vert[(count - 1) as usize] = true;
        } else {
            just.push(justify[i]);
            count += 1;
        }
    }
    if just.len() != ncols {
        eprintln!(
            "\nError.  Your table has {} columns but the number of \
             l or r symbols in justify is {}.\nThese numbers should be equal.",
            ncols,
            just.len()
        );
        eprintln!("justify = {}", strme(justify));
        for i in 0..rows.len() {
            eprintln!(
                "row {} = {} = {}",
                i + 1,
                rows[i].len(),
                rows[i].iter().format(",")
            );
        }
        assert_eq!(just.len(), ncols);
    }
    let mut maxcol = vec![0; ncols];
    let mut ext = vec![0; ncols];
    for i in 0..rrr.len() {
        for j in 0..rrr[i].len() {
            if j < rrr[i].len() - 1 && rrr[i][j + 1] == *"\\ext" {
                continue;
            }
            if rrr[i][j] == *"\\ext" || rrr[i][j] == *"\\hline" {
                continue;
            }
            maxcol[j] = max(maxcol[j], visible_width(&rrr[i][j]));
        }
    }
    let mut orig_vis_widths = vec![Vec::<usize>::new(); rrr.len()];
    for i in 0..rrr.len() {
        for j in 0..rrr[i].len() {
            orig_vis_widths[i].push(visible_width(&rrr[i][j]));
        }
    }
    if debug_print {
        println!("maxcol = {}", maxcol.iter().format(","));
        println!("\nvisible widths");
        let mut vis = rrr.clone();
        for i in 0..vis.len() {
            for j in 0..vis[i].len() {
                vis[i][j] = visible_width(&rrr[i][j]).to_string();
            }
        }
        let mut log = String::new();
        let mut justify = Vec::<u8>::new();
        for i in 0..vis[0].len() {
            if i > 0 {
                justify.push(b'|');
            }
            justify.push(b'r');
        }
        print_tabular_vbox(&mut log, &vis, 0, &justify, false, false);
        print!("{log}");
    }

    // Add space according to ext entries.

    for i in 0..rrr.len() {
        for j in 0..rrr[i].len() {
            // Test if matrix entry is not \\ext and the following entry is also not.

            if j < rrr[i].len() - 1 && rrr[i][j + 1] == *"\\ext" && rrr[i][j] != *"\\ext" {
                // Find the largest block j..k that does not include an ext column.

                let mut k = j + 1;
                while k < rrr[i].len() {
                    if rrr[i][k] != *"\\ext" {
                        break;
                    }
                    k += 1;
                }

                // Figure out how much space to add.  Defined *need* to be the width of the jth
                // entry in column i.  Define *have* to be the sum across l in j..k of the
                // maximum of column width entries, with addition for separation.

                let need = visible_width(&rrr[i][j]);
                // let need = orig_vis_widths[i][j];
                let mut have = 0;
                for l in j..k {
                    have += maxcol[l];
                    if l < k - 1 {
                        have += sep;
                        if vert[l] {
                            have += sep + 1;
                        }
                    }
                }
                if debug_print {
                    println!(
                        "\nrow {i} columns {j}-{k} = {}",
                        rrr[i][j..k].iter().format(",")
                    );
                    println!("row {i} columns {j}-{k}, have = {have}, need = {need}");
                }

                // If have exceeds need, add have - need spaces to the right of the (i,j)th entry.

                if have > need {
                    if debug_print {
                        println!("adding {} spaces to right of row {i} col {j}", have - need,);
                    }
                    for _ in need..have {
                        rrr[i][j].push(' ');
                    }

                // If instead need exceeds have, add to ext[k-1].  This is used later.
                } else if need > have {
                    maxcol[k - 1] += need - have;
                    if debug_print {
                        println!("increasing maxcol[{}] to {}", k - 1, maxcol[k - 1]);
                    }
                    ext[k - 1] += need - have;
                }

                // Look at the widths of column j entries and see if that means we need more space.

                let mut m = 0;
                for u in 0..rrr.len() {
                    if j >= rrr[u].len() {
                        eprintln!("\nProblem with line {}, not enough fields.\n", u);
                    }
                    if rrr[u][j] != *"\\ext" && rrr[u][j] != *"\\hline" {
                        m = max(m, visible_width(&rrr[u][j]));
                        // m = max(m, orig_vis_widths[u][j]);
                    }
                }
                if m > visible_width(&rrr[i][j]) {
                    // if m > orig_vis_widths[i][j] {
                    if debug_print {
                        eprintln!(
                            "adding {} spaces to right of row {i} column {j} because \
                            visible width = {}",
                            m - visible_width(&rrr[i][j]),
                            // m - orig_vis_widths[i][j],
                            visible_width(&rrr[i][j]),
                            // orig_vis_widths[i][j],
                        );
                    }
                    for _ in visible_width(&rrr[i][j])..m {
                        // for _ in orig_vis_widths[i][j]..m {
                        rrr[i][j].push(' ');
                    }
                }
            }
        }
    }

    // Create top boundary of table.

    log.push(topleft);
    for i in 0..ncols {
        let mut n = maxcol[i];
        if i < ncols - 1 {
            n += sep;
        }
        for _ in 0..n {
            log.push(dash);
        }
        if vert[i] {
            log.push(tee);
            for _ in 0..sep {
                log.push(dash);
            }
        }
    }
    log.push(topright);
    log.push('\n');

    // Go through the rows.

    for i in 0..nrows {
        if debug_print {
            println!("now row {} = {}", i, rrr[i].iter().format(","));
            println!("0 - pushing │ onto row {}", i);
        }
        log.push(verty);
        for j in 0..min(ncols, rrr[i].len()) {
            // Pad entries according to justification.

            let mut x = String::new();
            if j >= rrr[i].len() {
                for _ in 0..maxcol[j] {
                    x.push(' ');
                }
            } else if rrr[i][j] == *"\\hline" {
                for _ in 0..maxcol[j] {
                    x.push(dash);
                }
            } else {
                let r = rrr[i][j].clone();
                let rlen = visible_width(&r);
                let mut xlen = 0;
                if r != *"\\ext" {
                    if just[j] == b'r' {
                        for _ in rlen..(maxcol[j] - ext[j]) {
                            x.push(' ');
                            xlen += 1;
                        }
                    }
                    if j < rrr[i].len() {
                        x += &r;
                        xlen += visible_width(&r);
                    }
                    if just[j] == b'r' {
                        for _ in (maxcol[j] - ext[j])..maxcol[j] {
                            x.push(' ');
                            xlen += 1;
                        }
                    }
                    if just[j] == b'l' {
                        for _ in xlen..maxcol[j] {
                            x.push(' ');
                        }
                    }
                }
            }
            for c in x.chars() {
                log.push(c);
            }

            // Add separations and separators.

            let mut add_sep = true;
            if j + 1 < rrr[i].len() && rrr[i][j + 1] == *"\\ext" {
                add_sep = false;
            }
            let mut jp = j;
            while jp + 1 < rrr[i].len() {
                if rrr[i][jp + 1] != *"\\ext" {
                    break;
                }
                jp += 1;
            }
            if add_sep && jp < ncols - 1 {
                if rrr[i][j] == *"\\hline" {
                    for _ in 0..sep {
                        log.push(dash);
                    }
                } else {
                    for _ in 0..sep {
                        log.push(' ');
                    }
                }
            }
            if vert[j] && j + 1 >= rrr[i].len() {
                if debug_print {
                    println!("1 - pushing {} onto row {}, j = {}", verty, i, j);
                }
                log.push(verty);
                for _ in 0..sep {
                    log.push(' ');
                }
            } else {
                if vert[j] && rrr[i][j + 1] != "\\ext" {
                    if debug_print {
                        println!("1 - pushing {} onto row {}, j = {}", verty, i, j);
                    }
                    log.push(verty);
                    if rrr[i][j + 1] == *"\\hline" {
                        for _ in 0..sep {
                            log.push(dash);
                        }
                    } else {
                        for _ in 0..sep {
                            log.push(' ');
                        }
                    }
                }
            }
        }
        if debug_print {
            println!("2 - pushing {} onto row {}", verty, i);
        }
        log.push(verty);
        log.push('\n');
    }
    log.push(botleft);
    for i in 0..ncols {
        let mut n = maxcol[i];
        if i < ncols - 1 {
            n += sep;
        }
        for _ in 0..n {
            log.push(dash);
        }
        if vert[i] {
            if i + 1 >= rrr[rrr.len() - 1].len() {
                log.push(dash);
            } else if rrr[rrr.len() - 1][i + 1] != "\\ext" {
                log.push(uptee);
            } else {
                log.push(dash);
            }
            for _ in 0..sep {
                log.push(dash);
            }
        }
    }
    log.push(botright);
    log.push('\n');

    // Convert into a super-character vec of matrices.  There is one vector entry per line.
    // In each matrix, an entry is a super_character: a rust character, together with the escape
    // code characters that came before it.

    let mut mat = Vec::<Vec<Vec<char>>>::new();
    {
        let mut all = Vec::<Vec<char>>::new();
        let mut z = Vec::<char>::new();
        for c in log.chars() {
            if c != '\n' {
                z.push(c);
            } else {
                if !z.is_empty() {
                    all.push(z.clone());
                }
                z.clear();
            }
        }
        if !z.is_empty() {
            all.push(z);
        }
        for i in 0..all.len() {
            mat.push(package_characters_with_escapes_char(&all[i]));
        }
    }

    /*
    // FOR DEBUGGING
    println!("\ninitial:");
    let mut out = String::new();
    for i in 0..mat.len() {
        for j in 0..mat[i].len() {
            for k in 0..mat[i][j].len() {
                out.push(mat[i][j][k]);
            }
        }
        out.push('\n');
    }
    println!("{out}");
    */

    // "Smooth" edges of hlines.

    let verbose = debug_print;
    for i in 0..mat.len() {
        for j in 0..mat[i].len() {
            if j > 0
                && mat[i][j - 1] == vec![dash]
                && mat[i][j] == vec![verty]
                && j + 1 < mat[i].len()
                && mat[i][j + 1] == vec![dash]
                && i + 1 < mat.len()
                && j < mat[i + 1].len()
                && mat[i + 1][j].ends_with(&[verty])
                && i > 0
                && !mat[i - 1][j].ends_with(&[verty])
                && mat[i - 1][j] != vec![tee]
            {
                if verbose {
                    println!(
                        "(verty to tee) i = {i}, j = {j}, from {} to {tee}",
                        mat[i][j][0]
                    );
                }
                mat[i][j] = vec![tee];
            } else if j > 0
                && mat[i][j - 1] == vec![dash]
                && mat[i][j] == vec![verty]
                && j + 1 < mat[i].len()
                && mat[i][j + 1] == vec![dash]
                && i + 1 < mat.len()
                && j < mat[i + 1].len()
                && !mat[i + 1][j].ends_with(&[verty])
            {
                if verbose {
                    println!(
                        "(verty to uptee maybe) i = {i}, j = {j}, from {} to {uptee}",
                        mat[i][j][0]
                    );
                }
                if i == 0 || !mat[i - 1][j].ends_with(&[verty]) {
                    mat[i][j] = vec![dash];
                } else {
                    mat[i][j] = vec![uptee];
                }
            } else if j > 0
                && mat[i][j - 1] == vec![dash]
                && mat[i][j] == vec![verty]
                && j + 1 < mat[i].len()
                && mat[i][j + 1] == vec![dash]
                && i > 0
                && (mat[i - 1][j].ends_with(&[verty]) || mat[i - 1][j] == vec![tee])
            {
                if verbose {
                    println!(
                        "(verty to cross) i = {i}, j = {j}, from {} to {cross}",
                        mat[i][j][0]
                    );
                }
                mat[i][j] = vec![cross];
            } else if mat[i][j] == vec![verty]
                && j + 1 < mat[i].len()
                && mat[i][j + 1] == vec![dash]
                && (j == 0 || !mat[i][j - 1].ends_with(&[dash]))
            {
                if verbose {
                    println!(
                        "(verty to lefty) i = {i}, j = {j}, from {} to {lefty}",
                        mat[i][j][0]
                    );
                }
                mat[i][j] = vec![lefty];
            } else if j > 0
                && mat[i][j - 1] == vec![dash]
                && mat[i][j] == vec![verty]
                && (j + 1 == mat[i].len() || mat[i][j + 1] != vec![dash])
            {
                if verbose {
                    println!(
                        "(verty to righty) i = {i}, j = {j}, from {} to {righty}",
                        mat[i][j][0]
                    );
                }
                mat[i][j] = vec![righty];
            } else if j > 0
                && i + 1 < mat.len()
                && mat[i][j] == vec![tee]
                && !mat[i + 1][j].ends_with(&[verty])
            {
                if verbose {
                    println!("i = {i}, j = {j}, from {} to {dash}", mat[i][j][0]);
                }
                mat[i][j] = vec![dash];
            }
        }
    }

    // Output matrix.

    log.clear();
    for i in 0..mat.len() {
        for j in 0..mat[i].len() {
            for k in 0..mat[i][j].len() {
                log.push(mat[i][j][k]);
            }
        }
        log.push('\n');
    }

    // Finish.

    if debug_print {
        println!();
    }
}

// ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓

#[cfg(test)]
mod tests {

    // run this test using:
    // cargo test -p tables test_print_tabular_vbox -- --nocapture

    use crate::print_tabular_vbox;
    use ansi_escape::{emit_bold_escape, emit_end_escape};
    use string_utils::stringme;

    #[test]
    fn test_print_tabular_vbox() {
        // test 1

        println!("running test 1");
        let mut rows = Vec::<Vec<String>>::new();
        let row = vec![
            "omega".to_string(),
            "superduperfineexcellent".to_string(),
            "\\ext".to_string(),
        ];
        rows.push(row);
        let row = vec![
            "woof".to_string(),
            "snarl".to_string(),
            "octopus".to_string(),
        ];
        rows.push(row);
        let row = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        rows.push(row);
        let row = vec![
            "hiccup".to_string(),
            "tomatillo".to_string(),
            "ddd".to_string(),
        ];
        rows.push(row);
        let mut log = String::new();
        let justify = &[b'r', b'|', b'l', b'l'];
        print_tabular_vbox(&mut log, &rows, 2, justify, false, false);
        let answer = "┌────────┬─────────────────────────┐\n\
                      │ omega  │  superduperfineexcellent│\n\
                      │  woof  │  snarl      octopus     │\n\
                      │     a  │  b          c           │\n\
                      │hiccup  │  tomatillo  ddd         │\n\
                      └────────┴─────────────────────────┘\n";
        if log != answer {
            println!("\ntest 1 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 2

        println!("running test 2");
        let mut rows = Vec::<Vec<String>>::new();
        let row = vec!["pencil".to_string(), "pusher".to_string()];
        rows.push(row);
        let row = vec!["\\hline".to_string(), "\\hline".to_string()];
        rows.push(row);
        let row = vec!["fabulous pumpkins".to_string(), "\\ext".to_string()];
        rows.push(row);
        let mut log = String::new();
        let justify = &[b'l', b'|', b'l'];
        print_tabular_vbox(&mut log, &rows, 2, justify, false, false);
        let answer = "┌────────┬────────┐\n\
                      │pencil  │  pusher│\n\
                      ├────────┴────────┤\n\
                      │fabulous pumpkins│\n\
                      └─────────────────┘\n";
        if log != answer {
            println!("\ntest 2 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 3

        println!("running test 3");
        let mut rows = Vec::<Vec<String>>::new();
        let row = vec!["fabulous pumpkins".to_string(), "\\ext".to_string()];
        rows.push(row);
        let row = vec!["\\hline".to_string(), "\\hline".to_string()];
        rows.push(row);
        let row = vec!["pencil".to_string(), "pusher".to_string()];
        rows.push(row);
        let mut log = String::new();
        let justify = &[b'l', b'|', b'l'];
        print_tabular_vbox(&mut log, &rows, 2, justify, false, false);
        let answer = "┌─────────────────┐\n\
                      │fabulous pumpkins│\n\
                      ├────────┬────────┤\n\
                      │pencil  │  pusher│\n\
                      └────────┴────────┘\n";
        if log != answer {
            println!("\ntest 3 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 4

        println!("running test 4");
        let mut rows = Vec::<Vec<String>>::new();
        let row = vec!["\\hline".to_string(), "\\hline".to_string()];
        rows.push(row);
        let row = vec!["hunky".to_string(), "dory".to_string()];
        rows.push(row);
        let mut log = String::new();
        let justify = &[b'l', b'|', b'l'];
        print_tabular_vbox(&mut log, &rows, 2, justify, false, false);
        let answer = "┌───────┬──────┐\n\
                      ├───────┼──────┤\n\
                      │hunky  │  dory│\n\
                      └───────┴──────┘\n";
        if log != answer {
            println!("\ntest 4 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 5

        println!("running test 5");
        let mut escape = Vec::<u8>::new();
        emit_end_escape(&mut escape);
        let escape = stringme(&escape);
        let mut rows = Vec::<Vec<String>>::new();
        let mut row = Vec::<String>::new();
        row.push(format!("piglet"));
        row.push("\\ext".to_string());
        row.push(format!("kitten"));
        row.push("\\ext".to_string());
        row.push(format!("woof{escape}"));
        row.push(format!("p"));
        rows.push(row);
        rows.push(vec!["\\hline".to_string(); rows[0].len()]);
        let row = vec!["x".to_string(); 6];
        rows.push(row);
        let mut log = String::new();
        print_tabular_vbox(&mut log, &rows, 0, &b"l|l|l|l|l|l".to_vec(), false, false);
        let answer = "┌──────┬──────┬────┬─┐\n\
                      │piglet│kitten│woof[0m│p│\n\
                      ├─┬────┼─┬────┼────┼─┤\n\
                      │x│x   │x│x   │x   │x│\n\
                      └─┴────┴─┴────┴────┴─┘\n";
        if log != answer {
            println!("\ntest 5 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 6

        println!("running test 6");
        let mut e = Vec::<u8>::new();
        emit_bold_escape(&mut e);
        let start_bold = stringme(&e);
        let mut e = Vec::<u8>::new();
        emit_end_escape(&mut e);
        let stop_bold = stringme(&e);
        const TOPS: usize = 2;
        let mut rows = Vec::<Vec<String>>::new();
        let mut row = vec!["".to_string()];
        row.append(&mut vec!["\\ext".to_string(); 2]);
        for j in 0..TOPS {
            row.push(format!("    {start_bold}gumbo {}{stop_bold}", j + 1));
            row.append(&mut vec!["\\ext".to_string(); 2]);
        }
        row.push("".to_string());
        rows.push(row);
        rows.push(vec!["\\hline".to_string(); rows[0].len()]);
        let mut row = vec![
            "gerbil".to_string(),
            "pumpkins".to_string(),
            "top".to_string(),
        ];
        for _ in 0..TOPS {
            row.append(&mut vec![
                "dist".to_string(),
                "gumbo".to_string(),
                "len".to_string(),
            ]);
        }
        row.push("x".to_string());
        for j in 0..row.len() {
            row[j] = format!("{start_bold}{}{stop_bold}", row[j]);
        }
        rows.push(row);
        rows.push(vec!["\\hline".to_string(); rows[0].len()]);
        rows.push(vec!["0".to_string(); rows[0].len()]);
        let mut log = String::new();
        let mut just = b"l".to_vec();
        for _ in 0..rows[0].len() - 1 {
            just.append(&mut b"|l".to_vec());
        }
        print_tabular_vbox(&mut log, &rows, 0, &just, false, false);
        let answer = "┌───────────────────┬──────────────┬──────────────┬─┐\n\
                      │                   │    [01mgumbo 1[0m   │    [01mgumbo 2[0m   │ │\n\
                      ├──────┬────────┬───┼────┬─────┬───┼────┬─────┬───┼─┤\n\
                      │[01mgerbil[0m│[01mpumpkins[0m│[01mtop[0m│[01mdist[0m│[01mgumbo[0m│[01mlen[0m│[01mdist[0m│[01mgumbo[0m│[01mlen[0m│[01mx[0m│\n\
                      ├──────┼────────┼───┼────┼─────┼───┼────┼─────┼───┼─┤\n\
                      │0     │0       │0  │0   │0    │0  │0   │0    │0  │0│\n\
                      └──────┴────────┴───┴────┴─────┴───┴────┴─────┴───┴─┘\n";
        if log != answer {
            println!("\ntest 6 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 7

        println!("running test 7");
        let mut rows = vec![vec![String::new(); 7]; 5];
        rows[0][0] = "".to_string();
        rows[0][1] = "\\ext".to_string();
        rows[0][2] = " read".to_string();
        rows[0][3] = "\\ext".to_string();
        rows[0][4] = " edge".to_string();
        rows[0][5] = "\\ext".to_string();
        rows[0][6] = "".to_string();
        rows[1][0] = "\\hline".to_string();
        rows[1][1] = "\\hline".to_string();
        rows[1][2] = "\\hline".to_string();
        rows[1][3] = "\\hline".to_string();
        rows[1][4] = "\\hline".to_string();
        rows[1][5] = "\\hline".to_string();
        rows[1][6] = "\\hline".to_string();
        rows[2][0] = "woof".to_string();
        rows[2][1] = "p".to_string();
        rows[2][2] = "L".to_string();
        rows[2][3] = "R".to_string();
        rows[2][4] = "L".to_string();
        rows[2][5] = "R".to_string();
        rows[2][6] = "read".to_string();
        rows[3][0] = "\\hline".to_string();
        rows[3][1] = "\\hline".to_string();
        rows[3][2] = "\\hline".to_string();
        rows[3][3] = "\\hline".to_string();
        rows[3][4] = "\\hline".to_string();
        rows[3][5] = "\\hline".to_string();
        rows[3][6] = "\\hline".to_string();
        rows[4][0] = "3".to_string();
        rows[4][1] = "6".to_string();
        rows[4][2] = "0".to_string();
        rows[4][3] = "150".to_string();
        rows[4][4] = "132".to_string();
        rows[4][5] = "282".to_string();
        rows[4][6] = "AGGGATGGTAAGGATGTTTTCATTTGGTGATCAGTTGGGCTGAGCTGGGTTTTCCTT".to_string();
        let mut log = String::new();
        print_tabular_vbox(&mut log, &rows, 0, b"l|l|r|r|r|r|l", false, true);
        let answer =
            "┏━━━━━━┳━━━━━┳━━━━━━━┳━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┓\n\
┃      ┃ read┃ edge  ┃                                                         ┃\n\
┣━━━━┳━╋━┳━━━╋━━━┳━━━╋━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫\n\
┃woof┃p┃L┃  R┃  L┃  R┃read                                                     ┃\n\
┣━━━━╋━╋━╋━━━╋━━━╋━━━╋━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┫
┃3   ┃6┃0┃150┃132┃282┃AGGGATGGTAAGGATGTTTTCATTTGGTGATCAGTTGGGCTGAGCTGGGTTTTCCTT┃\n\
┗━━━━┻━┻━┻━━━┻━━━┻━━━┻━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━┛\n";
        if log != answer {
            println!("\ntest 7 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 8

        println!("running test 8");
        let mut rows = Vec::<Vec<String>>::new();
        rows.push(vec![
            "mangos".to_string(),
            "   1".to_string(),
            "\\ext".to_string(),
            "   2".to_string(),
            "\\ext".to_string(),
            "   3".to_string(),
            "\\ext".to_string(),
            "   4".to_string(),
            "\\ext".to_string(),
            "   5".to_string(),
            "\\ext".to_string(),
            "   6".to_string(),
            "\\ext".to_string(),
            " total".to_string(),
            "\\ext".to_string(),
        ]);
        rows.push(vec!["\\hline".to_string(); rows[0].len()]);
        let mut row = vec!["mooom".to_string()];
        for _ in 0..6 {
            row.push("   0".to_string());
            row.push("\\ext".to_string());
        }
        row.push(" 100.0".to_string());
        row.push("\\ext".to_string());
        rows.push(row);
        rows.push(vec!["\\hline".to_string(); rows[0].len()]);
        let mut row = vec!["amplifiers".to_string()];
        for _ in 0..7 {
            row.push("n".to_string());
            row.push("woofy".to_string());
        }
        rows.push(row);
        let mut log = String::new();
        print_tabular_vbox(
            &mut log,
            &rows,
            0,
            b"l|r|r|r|r|r|r|r|r|r|r|r|r|r|r",
            false,
            false,
        );
        let answer = "┌──────────┬───────┬───────┬───────┬───────┬───────┬───────┬───────┐\n\
│mangos    │   1   │   2   │   3   │   4   │   5   │   6   │ total │\n\
├──────────┼───────┼───────┼───────┼───────┼───────┼───────┼───────┤\n\
│mooom     │   0   │   0   │   0   │   0   │   0   │   0   │ 100.0 │\n\
├──────────┼─┬─────┼─┬─────┼─┬─────┼─┬─────┼─┬─────┼─┬─────┼─┬─────┤\n\
│amplifiers│n│woofy│n│woofy│n│woofy│n│woofy│n│woofy│n│woofy│n│woofy│\n\
└──────────┴─┴─────┴─┴─────┴─┴─────┴─┴─────┴─┴─────┴─┴─────┴─┴─────┘\n";
        if log != answer {
            println!("\ntest 8 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }

        // test 9

        /*
        println!("running test 9");
        let rows0 = vec![
            vec!["WOOFITY", "\\ext", "\\ext", "\\ext", "\\ext", "\\ext"],
            vec!["\\hline"; 6],
            vec!["gerbil", "\\ext", "\\ext", "hippo", "\\ext", "\\ext"],
            vec!["\\hline"; 6],
            vec!["A", "B", "C", "D", "E", "F"],
            vec!["\\hline"; 6],
            vec!["5", "0", "13", "18", "102", "5"],
        ];
        let mut rows = Vec::<Vec<String>>::new();
        for x in rows0.iter() {
            let mut r = Vec::<String>::new();
            for i in 0..x.len() {
                r.push(x[i].to_string());
            }
            rows.push(r);
        }
        let mut log = String::new();
        let justify = b"r|r|r|r|r|r";
        print_tabular_vbox(&mut log, &rows, 0, justify, true, false);
        let answer = "┌───────────────┐\n\
                      │        WOOFITY│\n\
                      ├──────┬────────┤\n\
                      │gerbil│   hippo│\n\
                      ├─┬─┬──┼──┬───┬─┤\n\
                      │A│B│ C│ D│  E│F│\n\
                      ├─┼─┼──┼──┼───┼─┤\n\
                      │5│0│13│18│102│5│\n\
                      └─┴─┴──┴──┴───┴─┘\n";
        if log != answer {
            println!("\ntest 9 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }
        */
    }
}
