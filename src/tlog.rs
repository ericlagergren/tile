use sha2::{Digest, Sha256};

const fn maxpow2(n: usize) -> (usize, isize) {
    let mut l = 0;
    while 1 << (l + 1) < n {
        l += 1;
    }
    (1 << l, l)
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Hash([u8; 32]);

impl AsRef<[u8]> for Hash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

/// Returns the content hash for record data.
pub fn record_hash(data: &[u8]) -> Hash {
    // SHA256(0x00 || data)
    // https://tools.ietf.org/html/rfc6962#section-2.1
    let mut h = Sha256::new();
    h.update(&[0x00]);
    h.update(data);
    Hash(h.finalize().into())
}

/// Returns the hash for an interior tree node.
pub fn node_hash(left: &Hash, right: &Hash) -> Hash {
    // SHA256(0x01 || left || right)
    // https://tools.ietf.org/html/rfc6962#section-2.1
    let mut h = Sha256::new();
    h.update(&[0x01]);
    h.update(left);
    h.update(right);
    Hash(h.finalize().into())
}

/// The coordinate of a tile.
#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Coordinate {
    /// The tile's level.
    pub level: isize,
    /// The tile's number within the level.
    pub n: usize,
}

impl Coordinate {
    /// The inverse of [`stored_hash_index`].
    ///
    /// ```rust
    /// use tile::tlog::{self, Coordinate};
    ///
    /// let want = Coordinate { level: 12, n: 34 };
    /// let got = Coordinate::split_stored_hash_index(
    ///     want.stored_hash_index()
    /// );
    /// assert_eq!(got, want);
    /// ```
    pub fn split_stored_hash_index(index: usize) -> Self {
        // Determine level 0 record before index.
        //
        // Given `stored_hash_index(0, n)` < 2*n, the `n` we want
        // is in `index/2..index/2+log2(index)`.
        let mut n = index / 2;
        let mut index_n = stored_hash_index(0, n);
        debug_assert!(index_n <= index);

        loop {
            // Each new record n adds 1 + trailingZeros(n)
            // hashes.
            let x = index_n + 1 + (n + 1).trailing_zeros() as usize;
            if x > index {
                break;
            }
            n += 1;
            index_n = x;
        }
        // The hash we want was committed with record n, meaning
        // it is one of (0, n), (1, n/2), (2, n/4), ...
        let level = (index - index_n) as isize;
        Self {
            level,
            n: n >> level,
        }
    }

    /// Maps the tree coordinates to a dense linear ordering that
    /// can be used for hash storage.
    ///
    /// Hash storage implementations that store hashes in
    /// sequential storage can use this function to compute where
    /// to read or write a given hash.
    pub fn stored_hash_index(&self) -> usize {
        stored_hash_index(self.level, self.n)
    }
}

/// Returns the number of stored hashes that are expected for
/// a tree with `n` records.
pub fn stored_hash_count(n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    // The tree will have the hashes up to the last leaf hash.
    let mut num_hash = stored_hash_index(0, n) + 1;
    // And it will have any hashes for subtrees completed by that
    // leaf.
    let mut i = n - 1;
    while i & 1 != 0 {
        num_hash += 1;
        i >>= 1;
    }
    num_hash
}

fn stored_hash_index(level: isize, mut n: usize) -> usize {
    debug_assert!(level >= -1);
    debug_assert!(level <= 63);

    // The nth hash for level L is written right after the
    // 2n+1 hash for level L+1.
    //
    // Work our way down to the level 0 ordering. We'll add
    // back the original level count at the end.
    for _ in (1..=level).rev() {
        // TODO(eric): overflow
        n = 2 * n + 1;
    }

    // The nth hash for level 0 is written at n+n/2+n/4+...
    // (n/2^i eventually hits zero.)
    let mut i = 0;
    while n > 0 {
        i += n;
        n >>= 1;
    }
    ((i as isize) + level) as usize
}

pub trait ReadHash {
    type Error;
    fn read_hashes(indices: &[usize]) -> Result<impl Iterator<Item = Hash>, Self::Error>;
}

pub fn tree_hash<R: ReadHash>(n: usize, reader: R) -> Result<Hash, R::Error> {
    if n == 0 {
        return Ok(Default::default());
    }
    let indices = sub_tree_index();
}

fn sub_tree_index(mut lo: usize, hi: usize, need: &[usize]) -> impl Iterator<Item = usize> {
    core::iter::from_fn(move || {
        if lo < hi {
            let (k, level) = maxpow2(hi - lo + 1);
            debug_assert_eq!(lo & (k - 1), 0);
            let index = stored_hash_index(level, lo >> level);
            lo += k;
            Some(index)
        } else {
            None
        }
        // while lo < hi {
        // }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_stored_hash_index() {
        for level in 0..10 {
            for n in 1..100 {
                let want = Coordinate { level, n };
                let got = Coordinate::split_stored_hash_index(want.stored_hash_index());
                assert_eq!(got, want, "{level},{n}");
            }
        }
    }
}
