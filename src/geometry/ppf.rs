// Point Pair Features (PPF):

use std::fmt;
use crate::geometry::core::HashType;
use crate::utils::convert::discretize_f32_value_into_u32 as discretize_value;
use crate::utils::convert::continuize_u32_value_into_f32 as continuize_value;
use crate::utils::convert::map_u8_to_aa;
use crate::utils::convert::*;

// 5 bit for AA, 4 bit for distance, 3 bit for sin & cos

#[derive(Ord, PartialOrd, Eq, PartialEq, Clone, Copy, Hash)]
pub struct HashValue(pub u32);

impl HashValue {
    #[inline]
    pub fn perfect_hash(ppf: &Vec<f32>, nbin_dist: usize, nbin_sincos: usize) -> u32 {
        let nbin_dist = if nbin_dist > 16 { 
            16.0
        } else if nbin_dist == 0 {
            NBIN_DIST
        } else {
            nbin_dist as f32
        };
        let nbin_sincos = if nbin_sincos > 8 {
            8.0
        } else if nbin_sincos == 0 {
            NBIN_SIN_COS
        } else {
            nbin_sincos as f32
        };
        let aa1 = ppf[0] as u32;
        let aa2 = ppf[1] as u32;
        let h_dist = discretize_value(ppf[2], MIN_DIST, MAX_DIST, nbin_dist); 
        let sin_arr = [ppf[3].sin(), ppf[4].sin(), ppf[5].sin()];
        let cos_arr = [ppf[3].cos(), ppf[4].cos(), ppf[5].cos()];
        let h_sin_arr = [
            discretize_value(sin_arr[0], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
            discretize_value(sin_arr[1], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
            discretize_value(sin_arr[2], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
        ];
        let h_cos_arr = [
            discretize_value(cos_arr[0], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
            discretize_value(cos_arr[1], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
            discretize_value(cos_arr[2], MIN_SIN_COS, MAX_SIN_COS, nbin_sincos),
        ];
        let hashvalue = (aa1 << 27) | (aa2 << 22) | (h_dist << 18)
            | (h_sin_arr[0] << 15) | (h_cos_arr[0] << 12) | (h_sin_arr[1] << 9)
            | (h_cos_arr[1] << 6) | (h_sin_arr[2] << 3) | (h_cos_arr[2]);
        hashvalue
    }
    
    pub fn perfect_hash_default(ppf: &Vec<f32>) -> u32 {
        HashValue::perfect_hash(ppf, NBIN_DIST as usize, NBIN_SIN_COS as usize)
    }
    
    pub fn reverse_hash(&self, nbin_dist: usize, nbin_sincos: usize) -> [f32; 6] {
        let aa1 = (self.0 >> 27) & BITMASK32_5BIT as u32;
        let aa2 = (self.0 >> 22) & BITMASK32_5BIT as u32;
        let dist = continuize_value(
            (self.0 >> 18) & BITMASK32_4BIT as u32, 
            MIN_DIST, MAX_DIST, nbin_dist as f32
        );
        let sin_n1_d = continuize_value(
            (self.0 >> 15) & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        let cos_n1_d = continuize_value(
            (self.0 >> 12) & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        let sin_n2_d = continuize_value(
            (self.0 >> 9) & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        let cos_n2_d = continuize_value(
            (self.0 >> 6) & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        let sin_n3_d = continuize_value(
            (self.0 >> 3) & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        let cos_n3_d = continuize_value(
            self.0 & BITMASK32_3BIT as u32, 
            MIN_SIN_COS, MAX_SIN_COS, nbin_sincos as f32
        );
        [
            aa1 as f32, aa2 as f32, dist, 
            sin_n1_d.atan2(cos_n1_d).to_degrees(),
            sin_n2_d.atan2(cos_n2_d). to_degrees(),
            sin_n3_d.atan2(cos_n3_d).to_degrees()
        ]
    }
    
    pub fn reverse_hash_default(&self) -> [f32; 6] {
        self.reverse_hash(NBIN_DIST as usize, NBIN_SIN_COS as usize)
    }
    
    pub fn hash(&self) -> u32 {
        self.0
    }
    
    pub fn from_u32(hashvalue: u32) -> Self {
        HashValue(hashvalue)
    }
    
    pub fn from_u64(hashvalue: u64) -> Self {
        HashValue(hashvalue as u32)
    }
    
    pub fn as_u32(&self) -> u32 {
        self.0
    }
    
    pub fn as_u64(&self) -> u64 {
        self.0 as u64
    }
    
    pub fn hash_type(&self) -> HashType {
        HashType::PointPairFeature
    }

    pub fn is_symmetric(&self) -> bool {
        let values = self.reverse_hash_default();
        (values[0] == values[1]) && (values[3] == values[4])
    }
    
}

impl fmt::Debug for HashValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ppf = self.reverse_hash_default();
        write!(
            f,
            "HashValue({}), ({},{}) d={}, ({},{},{})",
            self.0, map_u8_to_aa(ppf[0] as u8),  map_u8_to_aa(ppf[1] as u8),
            ppf[2], ppf[3], ppf[4], ppf[5]
        )
    }
}

impl fmt::Display for HashValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let ppf = self.reverse_hash_default();
        write!(
            f,
            "{}\t{}\t{}\t{}\t{}\t{}\t{}",
            self.0, ppf[0], ppf[1], ppf[2], ppf[3], ppf[4], ppf[5]
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::convert::map_aa_to_u8;
    use super::*;

    #[test]
    fn test_hashvalue() {
        let aa1 = b"ALA";
        let aa2 = b"GLY";
        let mut ppf = vec![
            7.5, 120.0_f32.to_radians(), 45.0_f32.to_radians(), -60.0_f32.to_radians()
        ];
        ppf.insert(0, map_aa_to_u8(aa1) as f32);
        ppf.insert(1, map_aa_to_u8(aa2) as f32);
        let hashvalue = HashValue::perfect_hash_default(&ppf);
        println!("{:?}", hashvalue);
    }
 
    #[test]
    fn test_multiple_bins() {
        let mut ppf = vec![
            7.5, 120.0_f32.to_radians(), 45.0_f32.to_radians(), -60.0_f32.to_radians()
        ];
        ppf.insert(0, map_aa_to_u8(b"ALA") as f32);
        ppf.insert(1, map_aa_to_u8(b"GLY") as f32);
        let hash = HashValue(HashValue::perfect_hash(&ppf, 8, 3));
        let rev = hash.reverse_hash(8, 3);
        assert_eq!(rev[0], 0.0);
        assert_eq!(rev[1], 7.0);
    }
    
}