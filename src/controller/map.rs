use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::mem::{self, ManuallyDrop};
use std::path::Path;
use std::slice;
use memmap2::Mmap;

use crate::index::alloc::estimate_hash_size;
use crate::prelude::GeometricHash;

// const INITIAL_CAPACITY: usize = 16;

#[derive(Debug)]
struct BitVec {
    bits: ManuallyDrop<Vec<u8>>,
    #[allow(dead_code)] // Suppress warning for now
    len: usize,
}

impl BitVec {
    fn new(size: usize) -> Self {
        let byte_size = (size + 7) / 8; // Round up to the nearest byte
        BitVec {
            bits: ManuallyDrop::new(vec![0; byte_size]),
            len: size,
        }
    }

    fn set(&mut self, index: usize, value: bool) {
        let byte_index = index / 8;
        let bit_index = index % 8;
        if value {
            self.bits[byte_index] |= 1 << bit_index;
        } else {
            self.bits[byte_index] &= !(1 << bit_index);
        }
    }

    fn get(&self, index: usize) -> bool {
        let byte_index = index / 8;
        let bit_index = index % 8;
        self.bits[byte_index] & (1 << bit_index) != 0
    }
}

#[derive(Debug)]
pub struct SimpleHashMap {
    buckets: ManuallyDrop<Vec<u32>>,           // Stores the hash indices with occupancy information
    occupancy: BitVec,                         // Stores the occupancy information
    keys: ManuallyDrop<Vec<u32>>,              // Stores keys separately
    values: ManuallyDrop<Vec<(usize, usize)>>, // Stores values separately
    size: usize,
    capacity: usize,
}

impl SimpleHashMap {
    pub fn new(capacity: usize) -> Self {
        SimpleHashMap {
            buckets: ManuallyDrop::new(vec![0; capacity]),
            occupancy: BitVec::new(capacity),
            keys: ManuallyDrop::new(Vec::with_capacity(capacity)),
            values: ManuallyDrop::new(Vec::with_capacity(capacity)),
            size: 0,
            capacity,
        }
    }

    fn _new_from_std_hashmap(mut map: std::collections::HashMap<GeometricHash, (usize, usize)>, capacity: usize) -> Self {
        let mut simple_hash_map = SimpleHashMap::new(capacity);

        for (key, value) in map.drain() {
            simple_hash_map.insert(key, value);
        }

        simple_hash_map
    }

    pub fn new_from_vec(vec: Vec<(GeometricHash, usize, usize)>) -> Self {
        let capacity = vec.len();
        let mut simple_hash_map = SimpleHashMap::new(capacity);
        for (key, value1, value2) in vec {
            simple_hash_map.insert(key, (value1, value2));
        }
        simple_hash_map
    }
    
    fn _new_from_dashmap(map: dashmap::DashMap<GeometricHash, (usize, usize)>, capacity: usize) -> Self {
        let mut simple_hash_map = SimpleHashMap::new(capacity);

        map.iter().for_each(|entry| {
            simple_hash_map.insert(entry.key().clone(), entry.value().clone());
        });
        
        simple_hash_map
    }

    fn hash(&self, hash: u32) -> usize {
        hash as usize % self.capacity
    }

    fn insert(&mut self, key: GeometricHash, value: (usize, usize)) {
        let key_u32 = key.as_u32();
        let hash = self.hash(key_u32);
        let mut index = hash;

        loop {
            if !self.occupancy.get(index) {
                let key_index = self.keys.len();
                self.keys.push(key_u32);
                self.values.push(value);
                self.buckets[index] = key_index as u32;
                self.occupancy.set(index, true);
                self.size += 1;
                return;
            } else if self.keys[self.buckets[index] as usize] == key_u32 {
                let key_index = self.buckets[index] as usize;
                self.values[key_index] = value;
                return;
            } else {
                index = (index + 1) % self.capacity;
            }
        }
    }
    
    fn insert_u32(&mut self, key: u32, value: (usize, usize)) {
        let hash = self.hash(key);
        let mut index = hash;

        loop {
            if !self.occupancy.get(index) {
                let key_index = self.keys.len();
                self.keys.push(key);
                self.values.push(value);
                self.buckets[index] = key_index as u32;
                self.occupancy.set(index, true);
                self.size += 1;
                return;
            } else if self.keys[self.buckets[index] as usize] == key {
                let key_index = self.buckets[index] as usize;
                self.values[key_index] = value;
                return;
            } else {
                index = (index + 1) % self.capacity;
            }
        }
    }
    

    pub fn get(&self, key: &GeometricHash) -> Option<&(usize, usize)> {
        let key_u32 = key.as_u32();
        let hash = self.hash(key_u32); // Assuming `hash_u32` is your perfect hash function
        let mut index = hash;
        let mut count = 0;
        loop {
            if !self.occupancy.get(index) {
                return None;
            } else if self.keys[self.buckets[index] as usize] == key_u32 {
                let key_index = self.buckets[index] as usize;
                return Some(&self.values[key_index]);
            } else {
                index = (index + 1) % self.capacity;
            }
            count += 1;
            if count == self.capacity {
                return None;
            }
        }
    }
    
    fn estimate_file_size(&self) -> usize {
        let mut size = 0;
        size += mem::size_of::<usize>() * 2; // size and capacity
        size += self.buckets.len() * mem::size_of::<u32>();
        size += (self.capacity + 7) / 8; // occupancy
        size += self.keys.len() * mem::size_of::<u32>();
        size += self.values.len() * mem::size_of::<(usize, usize)>();
        size
    }
    
    pub fn dump_to_disk(&self, path: &Path) -> std::io::Result<()> {
        // If file exists, make a new file
        let file = OpenOptions::new().read(true).write(true).create(true).open(path)?;
        file.set_len(self.estimate_file_size() as u64)?;
        let mut writer = BufWriter::new(file);
        // Serialize and write metadata
        writer.write_all(&self.size.to_le_bytes())?;
        writer.write_all(&self.capacity.to_le_bytes())?;

        // Serialize and write values
        let values_size = self.values.len() * mem::size_of::<(usize, usize)>();
        let values_bytes = unsafe {
            slice::from_raw_parts(self.values.as_ptr() as *const u8, values_size)
        };
        writer.write_all(values_bytes)?;
        // Serialize and write keys
        let keys_size = self.keys.len() * mem::size_of::<u32>();
        let keys_bytes = unsafe {
            slice::from_raw_parts(self.keys.as_ptr() as *const u8, keys_size)
        };
        // Append keys to the file
        writer.write_all(keys_bytes)?;

        // Serialize and write buckets. 
        let buckets_size = self.buckets.len() * mem::size_of::<u32>();
        // Byte ordering should be preserved. little endian
        let buckets_bytes = unsafe {
            slice::from_raw_parts(self.buckets.as_ptr() as *const u8, buckets_size)
        };
        writer.write_all(buckets_bytes)?;

        // Serialize and write occupancy
        let occupancy_size = self.occupancy.bits.len();
        let occupancy_bytes = unsafe {
            slice::from_raw_parts(self.occupancy.bits.as_ptr() as *const u8, occupancy_size)
        };
        writer.write_all(occupancy_bytes)?;
        
        Ok(())
    }

    pub fn load_from_disk(path: &Path) -> (std::io::Result<Self>, Mmap) {
        // Open as read-only
        let file = OpenOptions::new().read(true).write(true).open(path).expect("Failed to open file");
        let mmap = unsafe { Mmap::map(&file).expect("Failed to map file") };
        let mut offset = 0usize;
        // Deserialize and read metadata
        let mut size_bytes = [0u8; mem::size_of::<usize>()];
        size_bytes.copy_from_slice(&mmap[offset..offset + mem::size_of::<usize>()]);
        let size = usize::from_le_bytes(size_bytes);
        offset += mem::size_of::<usize>();

        let mut capacity_bytes = [0u8; mem::size_of::<usize>()];
        capacity_bytes.copy_from_slice(&mmap[offset..offset + mem::size_of::<usize>()]);
        let capacity = usize::from_le_bytes(capacity_bytes);
        offset += mem::size_of::<usize>();

        let keys_count = size;
        let values_size = keys_count * mem::size_of::<(usize, usize)>();
        let values = unsafe {
            let values_ptr = mmap.as_ptr().add(offset) as *mut (usize, usize);
            assert_ptr_for_raw_parts(values_ptr, keys_count);
            // slice::from_raw_parts(mmap.as_ptr().add(offset) as *const (usize, usize), keys_count).to_vec()
            ManuallyDrop::new(Vec::from_raw_parts(values_ptr, keys_count, keys_count))
        };
        offset += values_size;
        
        let keys_size = keys_count * mem::size_of::<u32>();
        let keys = unsafe {
            let keys_ptr = mmap.as_ptr().add(offset) as *mut u32;
            assert_ptr_for_raw_parts(keys_ptr, keys_count);
            // slice::from_raw_parts(mmap.as_ptr().add(offset) as *const u32, keys_count).to_vec()
            ManuallyDrop::new(Vec::from_raw_parts(keys_ptr, keys_count, keys_count))
        };
        offset += keys_size;
        
        let buckets_size = capacity * mem::size_of::<u32>();
        let buckets = unsafe {
            let bucket_ptr = mmap.as_ptr().add(offset) as *mut u32;
            assert_ptr_for_raw_parts(bucket_ptr, capacity);
            // Direct conversion from raw slice to Vec
            ManuallyDrop::new(Vec::from_raw_parts(bucket_ptr, capacity, capacity))
        };
        offset += buckets_size;

        // Deserialize and read occupancy
        let occupancy_size = (capacity + 7) / 8;
        let bits = unsafe {
            let bits_ptr = mmap.as_ptr().add(offset) as *mut u8;
            assert_ptr_for_raw_parts(bits_ptr, occupancy_size);
            ManuallyDrop::new(Vec::from_raw_parts(bits_ptr, occupancy_size, occupancy_size))
        };
        let occupancy = BitVec { bits: bits, len: capacity };
        
        (Ok(SimpleHashMap {
            buckets: buckets,
            occupancy,
            keys: keys,
            values: values,
            size,
            capacity,
        }), mmap)
    }
}

fn assert_ptr_for_raw_parts<T>(ptr: *const T, len: usize) {
    assert!(!ptr.is_null());
    assert_eq!(ptr as usize % mem::align_of::<T>(), 0);
    assert!(len > 0);
    assert!(len <= isize::MAX as usize / mem::size_of::<T>());
}

impl Drop for SimpleHashMap {
    fn drop(&mut self) {
        unsafe {
            // The vector should not be dropped before mmap dropped. Don't drop. leak here
            std::mem::forget(ManuallyDrop::take(&mut self.buckets));
            std::mem::forget(ManuallyDrop::take(&mut self.keys));
            std::mem::forget(ManuallyDrop::take(&mut self.values));
            std::mem::forget(ManuallyDrop::take(&mut self.occupancy.bits));
        }
    }
}

pub fn convert_sorted_hash_pairs_to_simplemap(
    sorted_pairs: Vec<(GeometricHash, usize)>
) -> (SimpleHashMap, Vec<usize>) {
    // OffsetMap - key: hash, value: (offset, length)
    let (total_hashes, total_values) = estimate_hash_size(&sorted_pairs);
    let mut offset_map = SimpleHashMap::new(total_hashes * 3);
    let mut vec: Vec<usize> = Vec::with_capacity(total_values);

    if let Some((first_hash, _)) = sorted_pairs.first() {
        let mut current_hash = first_hash;
        let mut current_offset = 0;
        let mut current_count = 0;

        for (index, pair) in sorted_pairs.iter().enumerate() {
            if pair.0 == *current_hash {
                current_count += 1;
            } else {
                offset_map.insert(*current_hash, (current_offset, current_count));
                current_hash = &pair.0;
                current_offset = index;
                current_count = 1;
            }
            vec.push(pair.1);
        }
        offset_map.insert(*current_hash, (current_offset, current_count));
    }
    (offset_map, vec)
}


pub fn convert_sorted_hash_vec_to_simplemap(
    sorted_vec: Vec<(u32, usize)>
) -> (SimpleHashMap, Vec<usize>) {
    // OffsetMap - key: hash, value: (offset, length)
    let (total_hashes, total_values) = estimate_hash_size(&sorted_vec);
    let mut offset_map = SimpleHashMap::new(total_hashes * 3);
    let mut vec: Vec<usize> = Vec::with_capacity(total_values);

    if let Some((first_hash, _)) = sorted_vec.first() {
        let mut current_hash = first_hash;
        let mut current_offset = 0;
        let mut current_count = 0;

        for (index, pair) in sorted_vec.iter().enumerate() {
            if pair.0 == *current_hash {
                current_count += 1;
            } else {
                offset_map.insert_u32(*current_hash, (current_offset, current_count));
                current_hash = &pair.0;
                current_offset = index;
                current_count = 1;
            }
            vec.push(pair.1);
        }
        offset_map.insert_u32(*current_hash, (current_offset, current_count));
    }
    (offset_map, vec)
}



#[cfg(test)]
mod tests {
    use dashmap::DashMap;

    use crate::measure_time;
    use crate::prelude::{read_offset_map, save_offset_map, GeometricHash};

    use super::*;
    use std::collections::HashMap as StdHashMap;
    use std::path::PathBuf;

    #[test]
    fn test_dump_and_load() {
        let mut std_map = StdHashMap::new();
        std_map.insert(GeometricHash::from_u32(2u32, crate::prelude::HashType::PDBTrRosetta), (200usize, 200usize));
        std_map.insert(GeometricHash::from_u32(1u32, crate::prelude::HashType::PDBTrRosetta), (100usize, 100usize));
        std_map.insert(GeometricHash::from_u32(13u32, crate::prelude::HashType::PDBTrRosetta), (1000usize, 100usize));
        let map = SimpleHashMap::_new_from_std_hashmap(std_map, 16usize);
        println!("MAP: {:?}", map);
        let path = PathBuf::from("hashmap.dat");

        map.dump_to_disk(&path).expect("Failed to dump to disk");
        // Change the permissions of the file to allow read and write access

        let (loaded_map, mmap) = SimpleHashMap::load_from_disk(&path);
        let loaded_map = loaded_map.expect("Failed to load from disk");
        println!("LOADED: {:?}", loaded_map);
        assert_eq!(loaded_map.get(&GeometricHash::from_u32(1u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(100usize, 100usize)));
        assert_eq!(loaded_map.get(&GeometricHash::from_u32(2u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(200usize, 200usize)));
        assert_eq!(loaded_map.get(&GeometricHash::from_u32(13u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(1000usize, 100usize)));
        assert_eq!(loaded_map.get(&GeometricHash::from_u32(3u32, crate::prelude::HashType::PDBTrRosetta)), None);
        drop(loaded_map);
        drop(mmap);
        std::fs::remove_file(path).expect("Failed to remove test file");
    }
    
    #[test]
    #[cfg(not(feature = "foldcomp"))]
    fn test_bigger() {
        let test_size = 200000usize;
        let std_map = DashMap::new();
        for i in 0..test_size {
            std_map.insert(GeometricHash::from_u32(i as u32, crate::prelude::HashType::PDBTrRosetta), (i, i));
        }
        let dash_map = std_map.clone();
        let map = SimpleHashMap::_new_from_dashmap(std_map, test_size);
        let path = PathBuf::from("hashmap.dat");

        map.dump_to_disk(&path).expect("Failed to dump to disk");
        let (loaded_map, mmap) = measure_time!(SimpleHashMap::load_from_disk(&path));
        if let Ok(loaded_map) = loaded_map {
            for i in 0..test_size {
                assert_eq!(loaded_map.get(&GeometricHash::from_u32(i as u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(i, i)));
            }
        }

        save_offset_map("hashmap.offset", &dash_map).unwrap();
        measure_time!({
            let _ = read_offset_map("hashmap.offset", crate::prelude::HashType::PDBTrRosetta).unwrap();
        });
        drop(mmap);
        // Delete the file
        std::fs::remove_file(path).expect("Failed to remove test file");
        std::fs::remove_file("hashmap.offset").expect("Failed to remove test file");
    }

    #[test]
    fn test_conversion_from_vector() {
        println!("Test conversion from vector");
        let vec = vec![
            (GeometricHash::from_u32(1999u32, crate::prelude::HashType::PDBTrRosetta), 100usize, 100usize),
            (GeometricHash::from_u32(22345234u32, crate::prelude::HashType::PDBTrRosetta), 200usize, 200usize),
        ];
        let map = SimpleHashMap::new_from_vec(vec);
        println!("{:?}", map);
        assert_eq!(map.get(&GeometricHash::from_u32(1999u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(100usize, 100usize)));
        assert_eq!(map.get(&GeometricHash::from_u32(22345234u32, crate::prelude::HashType::PDBTrRosetta)), Some(&(200usize, 200usize)));
        assert_eq!(map.get(&GeometricHash::from_u32(3u32, crate::prelude::HashType::PDBTrRosetta)), None);
    }
}
