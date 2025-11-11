// Copyright (c) 2019 10X Genomics, Inc. All rights reserved.

// Write and read functions to which one passes a File, a ref to a number type
// defining the start of a 'vector' of entries, and the number of entries.
//
// See also crate memmap.

use itertools::Itertools;
use std::convert::TryInto;
use std::io::Write;

#[cfg(not(target_os = "windows"))]
use std::os::unix::fs::MetadataExt;

pub trait BinaryInputOutputSafe {}
impl BinaryInputOutputSafe for i8 {}
impl BinaryInputOutputSafe for i16 {}
impl BinaryInputOutputSafe for i32 {}
impl BinaryInputOutputSafe for i64 {}
impl BinaryInputOutputSafe for u8 {}
impl BinaryInputOutputSafe for u16 {}
impl BinaryInputOutputSafe for u32 {}
impl BinaryInputOutputSafe for u64 {}
impl BinaryInputOutputSafe for f32 {}
impl BinaryInputOutputSafe for f64 {}
impl BinaryInputOutputSafe for ([u8; 5], [u8; 3]) {}
impl BinaryInputOutputSafe for ([u8; 20], u32, u32) {}
impl BinaryInputOutputSafe for [u8; 12] {}
// i128, u128?

use std::io::Error;

pub fn binary_write_from_ref<T>(f: &mut std::fs::File, p: &T, n: usize) -> Result<(), Error> {
    let raw = p as *const T as *const u8;
    unsafe {
        let sli: &[u8] = std::slice::from_raw_parts(raw, n * (std::mem::size_of::<T>()));
        f.write_all(sli)?;
        Ok(())
    }
}

pub fn binary_read_to_ref<T>(f: &mut std::fs::File, p: &mut T, n: usize) -> Result<(), Error> {
    let mut raw = p as *mut T as *mut u8;
    unsafe {
        use std::io::Read;
        let bytes_to_read = n * std::mem::size_of::<T>();
        let mut bytes_read = 0;
        // Rarely, one must read twice (maybe, not necessarily proven).  Conceivably one needs
        // to read more than twice on occasion.
        const MAX_TRIES: usize = 10;
        let mut reads = Vec::<usize>::new();
        for _ in 0..MAX_TRIES {
            if bytes_read == bytes_to_read {
                break;
            }
            raw = raw.add(bytes_read);
            let sli: &mut [u8] = std::slice::from_raw_parts_mut(raw, bytes_to_read - bytes_read);
            let n = f.read(sli).unwrap();
            reads.push(n);
            bytes_read += n;
        }
        if bytes_read != bytes_to_read {
            let mut msg = format!(
                "Failure in binary_read_to_ref, bytes_read = {}, but \
                bytes_to_read = {}.  Bytes read on successive\nattempts = {}.\n",
                bytes_read,
                bytes_to_read,
                reads.iter().format(","),
            );
            #[cfg(not(target_os = "windows"))]
            {
                let metadata = f.metadata()?;
                msg += &mut format!(
                    "File has length {} and inode {}.\n",
                    metadata.len(),
                    metadata.ino(),
                );
            }
            panic!("{}", msg);
        }
    }
    Ok(())
}

// The functions binary_write_vec and binary_read_vec append, either to a file,
// in the first case, or to a vector, in the second case.

pub fn binary_write_vec<T>(f: &mut std::fs::File, x: &[T]) -> Result<(), Error>
where
    T: BinaryInputOutputSafe,
{
    let n = x.len();
    binary_write_from_ref::<usize>(f, &n, 1)?;
    if n > 0 {
        return binary_write_from_ref::<T>(f, &x[0], x.len());
    }
    Ok(())
}

pub fn binary_read_vec<T>(f: &mut std::fs::File, x: &mut Vec<T>) -> Result<(), Error>
where
    T: BinaryInputOutputSafe,
{
    // Read the vector size.

    let mut n: usize = 0;
    binary_read_to_ref::<usize>(f, &mut n, 1)?;

    // Resize the vector without setting any of its entries.
    // (could use resize_without_setting)

    let len = x.len();
    if len + n > x.capacity() {
        let extra: usize = len + n - x.capacity();
        x.reserve(extra);
    }
    unsafe {
        x.set_len(len + n);
    }

    // Read the vector entries.

    if n > 0 {
        return binary_read_to_ref::<T>(f, &mut x[len], n);
    }
    Ok(())
}

pub fn binary_write_vec_vec<T>(f: &mut std::fs::File, x: &[Vec<T>]) -> Result<(), Error>
where
    T: BinaryInputOutputSafe,
{
    let n = x.len();
    binary_write_from_ref::<usize>(f, &n, 1)?;
    for i in 0..n {
        binary_write_vec::<T>(f, &x[i])?;
    }
    Ok(())
}

pub fn binary_read_vec_vec<T>(f: &mut std::fs::File, x: &mut Vec<Vec<T>>) -> Result<(), Error>
where
    T: BinaryInputOutputSafe + Clone,
{
    let mut n: usize = 0;
    binary_read_to_ref::<usize>(f, &mut n, 1)?;
    let len = x.len();
    if len + n > x.capacity() {
        let extra: usize = len + n - x.capacity();
        x.reserve(extra);
    }
    x.resize(len + n, Vec::<T>::new());
    for i in 0..n {
        binary_read_vec::<T>(f, &mut x[i])?;
    }
    Ok(())
}


pub trait FromLeBytes: Sized {
    const BYTE_LEN: usize;
    fn from_le_bytes_slice(bytes: &[u8]) -> Self;
}

impl FromLeBytes for f32 {
    const BYTE_LEN: usize = 4;
    fn from_le_bytes_slice(bytes: &[u8]) -> Self {
        let arr: [u8; 4] = bytes.try_into().unwrap();
        Self::from_le_bytes(arr)
    }
}

pub fn binary_read_vec_from_memory<T>(bytes: &[u8], x: &mut Vec<T>) -> usize
where
    T: BinaryInputOutputSafe + Clone + Default + FromLeBytes,
{
    let mut pos = 0;
    let n = usize::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
    pos += 8;
    x.clear();
    x.reserve(n);
    for _ in 0..n {
        let end = pos + T::BYTE_LEN;
        let val = T::from_le_bytes_slice(&bytes[pos..end]);
        x.push(val);
        pos = end;
    }
    pos
}

pub fn binary_read_vec_vec_from_memory(bytes: &[u8], x: &mut Vec<Vec<f64>>) -> usize {
    let t = std::mem::size_of::<f64>();
    let mut pos = 0;
    let n = usize::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
    pos += 8;
    x.resize(n, Vec::new());
    for i in 0..n {
        let k = usize::from_le_bytes(bytes[pos..pos + 8].try_into().unwrap());
        pos += 8;
        x[i].resize(k, 0.0);
        for j in 0..k {
            x[i][j] = f64::from_le_bytes(bytes[pos..pos + t].try_into().unwrap());
            pos += t;
        }
    }
    pos
}

pub fn binary_write_vec_to_memory(x: &Vec<f32>) -> Vec<u8> {
    let mut bytes = x.len().to_le_bytes().to_vec();
    for i in 0..x.len() {
        bytes.append(&mut x[i].to_le_bytes().to_vec());
    }
    bytes
}

pub fn binary_write_vec_vec_to_memory(x: &Vec<Vec<f32>>) -> Vec<u8> {
    let mut bytes = x.len().to_le_bytes().to_vec();
    for i in 0..x.len() {
        bytes.append(&mut binary_write_vec_to_memory(&x[i]));
    }
    bytes
}
