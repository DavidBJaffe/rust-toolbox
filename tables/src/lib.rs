// Copyright (c) 2018 10x Genomics, Inc. All rights reserved.

// Functions print_tabular and print_tabular_vbox for making pretty tables.  And related utilities.

use io_utils::{eprintme, fail};
use itertools::Itertools;
use std::cmp::{max, min};
use string_utils::*;

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
    if s == "\\ext" || s == "\\hline" || s == "\\hline_bold" {
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
        } else if c == '✅' {
            n += 2;
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
// denote a vertical bar.  The symbol ! denotes a bold vertical bar.
//
// There is no separation printed on the far left or far right.
//
// By a "matrix entry", we mean one of the Strings in "rows".
//
// Entries that begin with a backslash are reserved for future features.
// Symbols other than l or r or | or ! in "justify" are reserved for future features.
//
// An entry may be followed on the right by one more entries whose contents are
// exactly "\ext".  In that case the entries are treated as multi-column.  Padding
// is inserted as needed on the "right of the multicolumn".
//
// An entry may be "\hline", which gets you a horizontal line.  The normal use case is to
// use one or more of these in succession horizontally to connect two vertical lines.  Cannot
// be combined with \ext.
//
// \hline_bold: like \hline but bold
//
// bold_box: use bold box characters
//
// Really only guaranteed to work for the tested cases.

#[derive(Default)]
pub struct VboxOptions {
    pub bold_box: bool,
    pub bold_outer: bool,
}

pub fn print_tabular_vbox(
    log: &mut String,
    rows: &[Vec<String>],
    sep: usize,
    justify: &[u8],
    debug_print: bool,
    opt: &VboxOptions,
) {

    // Test that rows all have same length.

    for i in 1..rows.len() {
        if rows[i].len() != rows[0].len() {
            fail!("print_tabular_vbox: row {i} has length {} but row 0 has length {}",
                rows[i].len(), rows[0].len(),
            );
        }
    }

    // If you've added a test that fails and are trying to get it work, temporarily change
    // the next to the last entry in the print_tabular_vbox line for the test to true.

    // Define box characters.

    let dash = if !opt.bold_box { '─' } else { '━' };
    let dash_bold = '━';
    let verty = if !opt.bold_box { '│' } else { '┃' };
    let verty_bold = '┃';
    let topleft = if !opt.bold_box { '┌' } else { '┏' };
    let topleft_bold = '┏';
    let topright = if !opt.bold_box { '┐' } else { '┓' };
    let topright_bold = '┓';
    let botleft = if !opt.bold_box { '└' } else { '┗' };
    let botleft_bold = '┗';
    let botright = if !opt.bold_box { '┘' } else { '┛' };
    let botright_bold = '┛';
    let tee = if !opt.bold_box { '┬' } else { '┳' };
    let tee_bold = '┳';
    let uptee = if !opt.bold_box { '┴' } else { '┻' };
    let uptee_bold = '┻';
    let cross = if !opt.bold_box { '┼' } else { '╋' };
    let cross_bold_bold = '╋';
    let cross_bold1 = '╂';
    let cross_bold2 = '┿';
    let lefty = if !opt.bold_box { '├' } else { '┣' };
    let lefty_bold_bold = '┣';
    let lefty_bold1 = '┠';
    let righty = if !opt.bold_box { '┤' } else { '┫' };
    let righty_bold_bold = '┫';
    let righty_bold1 = '┨';

    // Proceed.

    let mut rrr = rows.to_owned();
    let nrows = rrr.len();
    let mut ncols = 0;
    for i in 0..nrows {
        ncols = max(ncols, rrr[i].len());
    }
    let mut vert = vec![false; ncols];
    let mut vert_bold = vec![false; ncols];
    let mut just = Vec::<u8>::new();
    let mut count = 0_isize;
    for i in 0..justify.len() {
        if justify[i] == b'|' {
            if count == 0 {
                fail!("print_tabular_vbox: justify may not start with |");
            }
            if count >= ncols as isize {
                eprintln!("\nposition of | in justify string is illegal");
                eprintme!(count, ncols);
            }
            assert!(count < ncols as isize);
            vert[(count - 1) as usize] = true;
        } else if justify[i] == b'!' {
            if count == 0 {
                fail!("print_tabular_vbox: justify may not start with !");
            }
            if count >= ncols as isize {
                eprintln!("\nposition of ! in justify string is illegal");
                eprintme!(count, ncols);
            }
            assert!(count < ncols as isize);
            vert[(count - 1) as usize] = true;
            vert_bold[(count - 1) as usize] = true;
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
    for i in 0..rrr.len() {
        for j in 0..rrr[i].len() {
            if j < rrr[i].len() - 1 && rrr[i][j + 1] == *"\\ext" {
                continue;
            }
            if rrr[i][j] == *"\\ext" || rrr[i][j].starts_with(&*"\\hline") {
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
        print_tabular_vbox(&mut log, &vis, 0, &justify, false, &VboxOptions::default());
        print!("{log}");
    }

    // Define a linear programming problem, which when solved yields adjusted widths for each
    // column, with the property that ext entries are properly accommodated.  The constraints are
    // given by sums of variables, defined by entry,ext,...,ext (maximal) being at least equal
    // to a number, which is the sum of actual widths for those columns, minus separation.

    let mut lhs = Vec::<Vec<bool>>::new();
    let mut rhs = Vec::<usize>::new();
    for i in 0..rrr.len() {
        let mut j = 0;
        while j < ncols {
            if rrr[i][j].starts_with(&"\\hline") {
                j += 1;
                continue;
            }
            let mut con = vec![false; ncols];
            con[j] = true;
            let mut k = j + 1;
            con[j] = true;
            while k < ncols {
                if rrr[i][k] != "\\ext" {
                    break;
                }
                con[k] = true;
                k += 1;
            }
            lhs.push(con);
            let mut r = 0 as isize;
            for l in j..k {
                r += visible_width(&rrr[i][l]) as isize;
                if l < k - 1 {
                    r -= sep as isize;
                    if vert[l] {
                        r -= sep as isize + 1;
                    }
                }
            }
            let r = max(r, 0) as usize;
            rhs.push(r);
            j = k;
        }
    }

    // Now, in a truly moronic fashion, find a solution to the linear programming problem of
    // minimizing x1 + ... + xn, subject to these constraints.
    // The solution here progressively increments the variables until all the constraints are
    // satisfied.  We could use an actual linear programming solver if we could find one that is
    // pure rust, appropriately licensed, and suitably general.

    let mut xw = vec![0; ncols];
    let mut deficit = 0;
    for i in 0..rhs.len() {
        deficit += rhs[i];
    }

    let mut sum = vec![vec![0; lhs.len()]; ncols];
    for i in 0..ncols {
        let mut xw_new = xw.clone();
        xw_new[i] += 1;
        for j in 0..lhs.len() {
            for k in 0..ncols {
                if lhs[j][k] {
                    sum[i][j] += xw_new[k];
                }
            }
        }
    }

    loop {
        let mut best_improvement = 0;
        let mut best_i = 0;
        for i in 0..ncols {
            let mut xw_new = xw.clone();
            xw_new[i] += 1;
            let mut current_deficit = 0;
            for j in 0..lhs.len() {
                let sum = sum[i][j];
                if sum < rhs[j] {
                    current_deficit += rhs[j] - sum;
                }
            }
            if current_deficit < deficit {
                let improvement = deficit - current_deficit;
                if improvement > best_improvement {
                    best_improvement = improvement;
                    best_i = i;
                }
            }
        }
        xw[best_i] += 1;
        for i in 0..ncols {
            for j in 0..lhs.len() {
                if lhs[j][best_i] {
                    sum[i][j] += 1;
                }
            }
        }
        deficit -= best_improvement;
        if deficit == 0 {
            break;
        }
    }

    if debug_print {
        println!("\nlinear constraints and solution");
        let mut rows = Vec::<Vec<String>>::new();
        for i in 0..lhs.len() {
            let mut row = Vec::<String>::new();
            for j in 0..ncols {
                row.push((if lhs[i][j] { "x" } else { " " }).to_string());
            }
            rows.push(row);
        }
        let mut log = String::new();
        let mut justify = Vec::<u8>::new();
        for i in 0..ncols {
            if i > 0 {
                justify.push(b'|');
            }
            justify.push(b'r');
        }
        rows.push(vec!["\\hline".to_string(); ncols]);
        let mut row = Vec::<String>::new();
        for i in 0..xw.len() {
            row.push(xw[i].to_string());
        }
        rows.push(row);
        print_tabular_vbox(&mut log, &rows, 0, &justify, false, &VboxOptions::default());
        let mut log2 = String::new();
        for (z, line) in log.lines().enumerate() {
            if z >= 1 && z <= lhs.len() {
                log2 += &mut format!("{line} ≥ {}\n", rhs[z - 1]);
            } else {
                log2 += &mut format!("{line}\n");
            }
        }
        print!("{log2}");
    }

    // Add space according to ext entries.

    for i in 0..rrr.len() {
        for j in 0..ncols {
            if rrr[i][j] == "\\ext" || rrr[i][j].starts_with(&"\\hline") {
                continue;
            }
            let w = visible_width(&rrr[i][j]);
            let mut target = xw[j];
            let mut k = j + 1;
            let mut exting = false;
            while k < ncols && rrr[i][k] == "\\ext" {
                exting = true;
                target += xw[k];
                target += sep;
                if vert[k - 1] {
                    target += sep + 1;
                }
                k += 1;
            }
            if target > w {
                let add = target - w;
                // Use left justification in the exting case, for backward compatibility,
                // and maybe because it usually makes a nicer table.  But not consistent with doc.
                if just[j] == b'l' || exting {
                    if debug_print {
                        println!("({}, {}) adding {add} on right", i + 1, j + 1);
                    }
                    for _ in 0..add {
                        rrr[i][j].push(' ');
                    }
                } else {
                    if debug_print {
                        println!("({}, {}) adding {add} on left", i + 1, j + 1);
                    }
                    for _ in 0..add {
                        rrr[i][j] = " ".to_string() + &mut rrr[i][j].clone();
                    }
                }
            }
        }
    }

    // Update maxcol.

    let mut maxcol = vec![0; ncols];
    for i in 0..rrr.len() {
        for j in 0..rrr[i].len() {
            if j < rrr[i].len() - 1 && rrr[i][j + 1] == *"\\ext" {
                continue;
            }
            if rrr[i][j] == *"\\ext" || rrr[i][j].starts_with(&*"\\hline") {
                continue;
            }
            maxcol[j] = max(maxcol[j], visible_width(&rrr[i][j]));
        }
    }
    if debug_print {
        println!("now maxcol = {}", maxcol.iter().format(","));
    }

    // Replace leading ext.

    for j in 0..rrr.len() {
        if rrr[j][0] == "\\ext" {
            rrr[j][0] = stringme(&vec![b' '; maxcol[0]]);
        }
    }

    // Create top boundary of table.

    if !opt.bold_outer {
        log.push(topleft);
    } else {
        log.push(topleft_bold);
    }
    for i in 0..ncols {
        let mut n = maxcol[i];
        if i < ncols - 1 {
            n += sep;
        }
        for _ in 0..n {
            if !opt.bold_outer {
                log.push(dash);
            } else {
                log.push(dash_bold);
            }
        }
        if vert[i] {
            if !vert_bold[i] {
                if !opt.bold_outer {
                    log.push(tee);
                } else {
                    log.push(tee_bold);
                }
            } else {
                log.push(tee_bold);
            }
            for _ in 0..sep {
                if !opt.bold_outer {
                    log.push(dash);
                } else {
                    log.push(dash_bold);
                }
            }
        }
    }
    if !opt.bold_outer {
        log.push(topright);
    } else {
        log.push(topright_bold);
    }
    log.push('\n');

    // Go through the rows.

    for i in 0..nrows {
        if debug_print {
            println!("now row {} = {}", i, rrr[i].iter().format(","));
            println!("0 - pushing │ onto row {}", i);
        }
        if !opt.bold_outer {
            log.push(verty);
        } else {
            log.push(verty_bold);
        }
        for j in 0..min(ncols, rrr[i].len()) {
            // Pad entries according to justification.

            let mut x = String::new();
            if j >= rrr[i].len() {
                for _ in 0..maxcol[j] {
                    x.push(' ');
                }
            } else if rrr[i][j].starts_with(&*"\\hline") {
                for _ in 0..maxcol[j] {
                    if rrr[i][j] == *"\\hline" {
                        x.push(dash);
                    } else {
                        x.push(dash_bold);
                    }
                }
            } else {
                let r = rrr[i][j].clone();
                let rlen = visible_width(&r);
                let mut xlen = 0;
                if r != *"\\ext" {
                    if just[j] == b'r' {
                        for _ in rlen..maxcol[j] {
                            x.push(' ');
                            xlen += 1;
                        }
                    }
                    if j < rrr[i].len() {
                        x += &r;
                        xlen += visible_width(&r);
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
                if rrr[i][j].starts_with(&*"\\hline") {
                    for _ in 0..sep {
                        if rrr[i][j] == *"\\hline" {
                            log.push(dash);
                        } else {
                            log.push(dash_bold);
                        }
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
                if !vert_bold[j] {
                    log.push(verty);
                } else {
                    log.push(verty_bold);
                }
                for _ in 0..sep {
                    log.push(' ');
                }
            } else {
                if vert[j] && rrr[i][j + 1] != "\\ext" {
                    if debug_print {
                        println!("1 - pushing {} onto row {}, j = {}", verty, i, j);
                    }
                    if !vert_bold[j] {
                        log.push(verty);
                    } else {
                        log.push(verty_bold);
                    }
                    if rrr[i][j + 1].starts_with(&*"\\hline") {
                        for _ in 0..sep {
                            if rrr[i][j + 1] == *"\\hline" {
                                log.push(dash);
                            } else {
                                log.push(dash_bold);
                            }
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
        if !opt.bold_outer {
            log.push(verty);
        } else {
            log.push(verty_bold);
        }
        log.push('\n');
    }
    if !opt.bold_outer {
        log.push(botleft)
    } else {
        log.push(botleft_bold)
    }
    for i in 0..ncols {
        let mut n = maxcol[i];
        if i < ncols - 1 {
            n += sep;
        }
        for _ in 0..n {
            if !opt.bold_outer {
                log.push(dash);
            } else {
                log.push(dash_bold);
            }
        }
        if vert[i] {
            if i + 1 >= rrr[rrr.len() - 1].len() {
                if !vert_bold[i] {
                    log.push(dash);
                } else {
                    log.push(dash_bold);
                }
            } else if rrr[rrr.len() - 1][i + 1] != "\\ext" {
                if !vert_bold[i] {
                    log.push(uptee);
                } else {
                    log.push(uptee_bold);
                }
            } else {
                if !vert_bold[i] {
                    log.push(dash);
                } else {
                    log.push(dash_bold);
                }
            }
            for _ in 0..sep {
                if !vert_bold[i] {
                    log.push(dash);
                } else {
                    log.push(dash_bold);
                }
            }
        }
    }
    if !opt.bold_outer {
        log.push(botright)
    } else {
        log.push(botright_bold)
    }
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
                && (mat[i][j - 1] == vec![dash] || mat[i][j - 1] == vec![dash_bold])
                && (mat[i][j] == vec![verty] || mat[i][j] == vec![verty_bold])
                && j + 1 < mat[i].len()
                && (mat[i][j + 1] == vec![dash] || mat[i][j + 1] == vec![dash_bold])
                && i + 1 < mat.len()
                && j < mat[i + 1].len()
                && (mat[i + 1][j].ends_with(&[verty]) || mat[i + 1][j].ends_with(&[verty_bold]))
                && i > 0
                && (j >= mat[i - 1].len() || (!mat[i - 1][j].ends_with(&[verty]) && !mat[i - 1][j].ends_with(&[verty_bold])))
                && (j >= mat[i - 1].len() || (mat[i - 1][j] != vec![tee] && mat[i - 1][j] != vec![tee_bold]))
            {
                if verbose {
                    println!(
                        "(verty to tee) i = {i}, j = {j}, from {} to {tee}",
                        mat[i][j][0]
                    );
                }
                if !opt.bold_outer {
                    mat[i][j] = vec![tee];
                } else {
                    mat[i][j] = vec![tee_bold];
                }
            } else if j > 0
                && (mat[i][j - 1] == vec![dash] || mat[i][j - 1] == vec![dash_bold])
                && (mat[i][j] == vec![verty] || mat[i][j] == vec![verty_bold])
                && j + 1 < mat[i].len()
                && (mat[i][j + 1] == vec![dash] || mat[i][j + 1] == vec![dash_bold])
                && i + 1 < mat.len()
                && j < mat[i + 1].len()
                && (!mat[i + 1][j].ends_with(&[verty]) && !mat[i + 1][j].ends_with(&[verty_bold]))
            {
                if verbose {
                    println!(
                        "(verty to uptee maybe) i = {i}, j = {j}, from {} to {uptee}",
                        mat[i][j][0]
                    );
                }
                if i == 0 || (!mat[i - 1][j].ends_with(&[verty]) && !mat[i - 1][j].ends_with(&[verty_bold])) {
                    if mat[i - 1][j].ends_with(&[verty_bold]) {
                        mat[i][j] = vec![dash_bold];
                    } else {
                        mat[i][j] = vec![dash];
                    }
                } else {
                    if !opt.bold_outer {
                        mat[i][j] = vec![uptee];
                    } else {
                        mat[i][j] = vec![uptee_bold];
                    }
                }
            } else if j > 0
                && (mat[i][j - 1] == vec![dash] || mat[i][j - 1] == vec![dash_bold])
                && (mat[i][j] == vec![verty] || mat[i][j] == vec![verty_bold])
                && j + 1 < mat[i].len()
                && (mat[i][j + 1] == vec![dash] || mat[i][j + 1] == vec![dash_bold])
                && i > 0
                && ( (mat[i - 1][j].ends_with(&[verty]) || mat[i - 1][j].ends_with(&[verty_bold])) || mat[i - 1][j] == vec![tee] || mat[i - 1][j] == vec![tee_bold])
            {
                if verbose {
                    println!(
                        "(verty to cross) i = {i}, j = {j}, from {} to {cross}",
                        mat[i][j][0]
                    );
                }
                let mut this_cross = cross;
                if mat[i][j - 1] == vec![dash_bold] && mat[i - 1][j] == vec![verty_bold] {
                    this_cross = cross_bold_bold;
                } else if mat[i][j - 1] == vec![dash_bold] {
                    this_cross = cross_bold2;
                } else if mat[i - 1][j] == vec![verty_bold] {
                    this_cross = cross_bold1;
                }
                mat[i][j] = vec![this_cross];
            } else if (mat[i][j] == vec![verty] || mat[i][j] == vec![verty_bold])
                && j + 1 < mat[i].len()
                && (mat[i][j + 1] == vec![dash] || mat[i][j + 1] == vec![dash_bold])
                && (j == 0 || (!mat[i][j - 1].ends_with(&[dash])) && !mat[i][j - 1].ends_with(&[dash_bold]))
            {
                if verbose {
                    println!(
                        "(verty to lefty) i = {i}, j = {j}, from {} to {lefty}",
                        mat[i][j][0]
                    );
                }
                if !opt.bold_outer {
                    mat[i][j] = vec![lefty];
                } else if mat[i][j + 1] == vec![dash_bold] {
                    mat[i][j] = vec![lefty_bold_bold];
                } else {
                    mat[i][j] = vec![lefty_bold1];
                }
            } else if j > 0
                && (mat[i][j - 1] == vec![dash] || mat[i][j - 1] == vec![dash_bold])
                && (mat[i][j] == vec![verty] || mat[i][j] == vec![verty_bold])
                && (j + 1 == mat[i].len() || (mat[i][j + 1] != vec![dash] && mat[i][j + 1] != vec![dash_bold]))
            {
                if verbose {
                    println!(
                        "(verty to righty) i = {i}, j = {j}, from {} to {righty}",
                        mat[i][j][0]
                    );
                }
                if !opt.bold_outer {
                    mat[i][j] = vec![righty];
                } else if mat[i][j - 1] == vec![dash_bold] {
                    mat[i][j] = vec![righty_bold_bold];
                } else {
                    mat[i][j] = vec![righty_bold1];
                }
            } else if j > 0
                && i + 1 < mat.len()
                && (mat[i][j] == vec![tee] || mat[i][j] == vec![tee_bold])
                && (i + 1 >= mat.len()
                    || j >= mat[i + 1].len()
                    || (!mat[i + 1][j].ends_with(&[verty])
                    && !mat[i + 1][j].ends_with(&[verty_bold])))
            {
                if verbose {
                    println!("i = {i}, j = {j}, from {} to {dash}", mat[i][j][0]);
                }
                if opt.bold_outer {
                    mat[i][j] = vec![dash_bold];
                } else {
                    mat[i][j] = vec![dash];
                }
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
                      │  woof  │  snarl           octopus│\n\
                      │     a  │  b               c      │\n\
                      │hiccup  │  tomatillo       ddd    │\n\
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
                      ├────┬─┼────┬─┼────┼─┤\n\
                      │x   │x│x   │x│x   │x│\n\
                      └────┴─┴────┴─┴────┴─┘\n";
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
        print_tabular_vbox(&mut log, &rows, 0, justify, false, false);
        let answer = "┌───────────────┐\n\
                      │WOOFITY        │\n\
                      ├──────┬────────┤\n\
                      │gerbil│hippo   │\n\
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

        // test 10

        println!("running test 10");
        let rows0 = vec![
            vec!["\\ext", "HELLO", "\\ext", "\\ext"],
            vec!["\\hline"; 4],
            vec!["bloop", "meep", "toes", "dust"],
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
        let justify = b"l|l|l|l";
        print_tabular_vbox(&mut log, &rows, 0, justify, false, false);
        let answer = "┌─────┬──────────────┐\n\
                      │     │HELLO         │\n\
                      ├─────┼────┬────┬────┤\n\
                      │bloop│meep│toes│dust│\n\
                      └─────┴────┴────┴────┘\n";
        if log != answer {
            println!("\ntest 10 failed");
            println!("\nyour answer:\n{}", log);
            println!("correct answer:\n{}", answer);
        }
        if log != answer {
            panic!();
        }
    }
}
