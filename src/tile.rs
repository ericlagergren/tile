use core::{fmt, ops::Deref};

use super::tlog::Coordinate;

/// A description of a transparency log tile.
///
/// A tile of height `H` at level `L` offset `N` lists `W`
/// consecutive hashes at level `H*L` of the tree starting at
/// offset `N*(2**H)`. A complete tile lists `2**H` hashes;
/// a partial tile lists fewer. Note that a tile represents the
/// entire subtree of height H with those hashes as the leaves.
/// The levels above `H*L` can be reconstructed by hashing the
/// leaves.
///
/// Each Tile can be encoded as a “tile coordinate path”
/// of the form `tile/H/L/NNN[.p/W]`.
/// The `.p/W` suffix is present only for partial tiles, meaning `W < 2**H`.
/// The `NNN` element is an encoding of `N` into 3-digit path elements.
/// All but the last path element begins with an "x".
/// For example,
/// Tile{H: 3, L: 4, N: 1234067, W: 1}'s path
/// is tile/3/4/x001/x234/067.p/1, and
/// Tile{H: 3, L: 4, N: 1234067, W: 8}'s path
/// is tile/3/4/x001/x234/067.
/// See the [Tile.Path] method and the [ParseTilePath] function.
///
/// The special level L=-1 holds raw record data instead of hashes.
/// In this case, the level encodes into a tile path as the path element
/// "data" instead of "-1".
///
/// See also https://golang.org/design/25530-sumdb#checksum-database
/// and https://research.swtch.com/tlog#tiling_a_log.
#[derive(Copy, Clone, Debug, Default, Hash, Eq, PartialEq, Ord, PartialOrd)]
pub struct Tile {
    /// Height of the tile in `1..=30`.
    height: usize,
    /// Level of the tile in `-1..=63`.
    level: isize,
    /// Number within the level in `0..`.
    n: usize,
    /// Width of the tile in `1..=2^height`.
    width: usize,
}

impl Tile {
    /// Creates a `Tile`, returning `None` if any of the
    /// invariants are violated.
    pub const fn new(height: usize, level: isize, n: usize, width: usize) -> Option<Self> {
        if height > 30 {
            return None;
        }
        if level < -1 || level > 63 {
            return None;
        }
        if width < 1 || width > 1 << height {
            return None;
        }
        Some(Self {
            height,
            level,
            n,
            width,
        })
    }

    /// The height of the tile.
    ///
    /// Invariant: the result is in `1..=30`.
    pub const fn height(&self) -> usize {
        self.height
    }

    /// The level of the tile.
    ///
    /// Invariant: the result is in `-1..=63`.
    pub const fn level(&self) -> isize {
        self.level
    }

    /// The number within the level.
    ///
    /// Invariant: the result is in `0..`.
    pub const fn number(&self) -> usize {
        self.n
    }

    /// The width of the tile.
    ///
    /// Invariant: the result is in `1..=2^height`.
    pub const fn width(&self) -> usize {
        self.width
    }

    /// Does this tile hold data instead of a hash?
    pub const fn is_data(&self) -> bool {
        self.level() == -1
    }

    /// Returns the tile of a fixed non-zero `height` and at
    /// least width storing the given hash storage index.
    ///
    /// Returns `None` if `height` is out of range.
    pub fn for_index(height: usize, index: usize) -> Option<Self> {
        if height == 0 || height > 30 {
            return None;
        };
        let Coordinate { mut level, mut n } = Coordinate::split_stored_hash_index(index);

        let mut tile = Tile {
            height,
            // This cannot wrap since `height` is in 0..=30.
            #[allow(clippy::cast_possible_wrap)]
            level: level / (height as isize),
            ..Default::default()
        };
        level -= tile.level * (height as isize);
        tile.n = n << level >> tile.height;
        n -= tile.n << tile.height >> level;
        tile.width = (n + 1) << level;

        Some(tile)
    }

    ///// Returns the tile's `k`th parent for a tree size of `n`.
    // fn parent(mut self, k: usize, n: usize) -> Self {
    //     self.level += k as isize;
    //     self.n >>= k * self.height;
    //     self.width = 1 << self.height;
    //     let max = n >> ((self.level as usize) * self.height);
    //     if self.n << (self.height + self.width) >= max {
    //         if self.n << self.height >= max {
    //             return Self::default();
    //         }
    //         self.width = max - (self.n << self.height);
    //     }
    //     self
    // }
}

impl fmt::Display for Tile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            height,
            level,
            n,
            width,
        } = *self;

        write!(f, "tile/{height}/")?;
        if level == -1 {
            write!(f, "data/")?;
        } else {
            write!(f, "{level}/")?;
        }

        if let Some((first, last)) = format(&mut [0usize; 7], n).split_last() {
            for v in last {
                write!(f, "x{v:03}/")?;
            }
            write!(f, "{first:03}")?;
        }

        if width != 1 << height {
            write!(f, ".p/{width}")?;
        }
        Ok(())
    }
}

fn format(buf: &mut [usize; 7], mut n: usize) -> &[usize] {
    const PATH_BASE: usize = 1000;

    for (i, v) in buf.iter_mut().enumerate().rev() {
        *v = n % PATH_BASE;
        if n < PATH_BASE {
            #[allow(clippy::indexing_slicing)] // clearly okay
            return &buf[i..];
        }
        n /= PATH_BASE;
    }
    &buf[..]
}

#[derive(Clone, Debug, Default)]
pub struct Tiles {
    tiles: Box<[Tile]>,
}

impl Tiles {
    pub fn new(height: usize, old_tree_size: usize, new_tree_size: usize) -> Option<Self> {
        if height == 0 || height > 30 {
            return None;
        };
        let mut tiles = Vec::new();
        for level in 0..=63 {
            if new_tree_size >> (height * level) == 0 {
                break;
            }
            let old_n = old_tree_size >> (height * level);
            let new_n = new_tree_size >> (height * level);
            if old_n == new_n {
                continue;
            }
            let mut n = old_n >> height;
            while n < new_n >> height {
                tiles.push(Tile::new(height, level as isize, n, 1 << height)?);
                n += 1;
            }
            let n = new_n >> height;
            let width = new_n - (n << height);
            if width > 0 {
                tiles.push(Tile::new(height, level as isize, n, width)?);
            }
        }
        Some(Self {
            tiles: tiles.into_boxed_slice(),
        })
    }
}

impl Deref for Tiles {
    type Target = [Tile];

    fn deref(&self) -> &Self::Target {
        &self.tiles
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! tile {
        ($height:expr, $level:expr, $n:expr, $width:expr) => {{
            Tile {
                height: $height,
                level: $level,
                n: $n,
                width: $width,
            }
        }};
    }

    #[test]
    fn test_tiles_new() {
        let cases = [
            (1, 1, 0),
            (100, 101, 1),
            (1023, 1025, 3),
            (1024, 1030, 1),
            (1030, 2000, 1),
            (1030, 10000, 10),
            (49516517, 49516586, 3),
        ];
        for (old, new, want) in cases {
            let tiles = Tiles::new(10, old, new).unwrap();
            assert_eq!(tiles.len(), want, "({old}, {new}, {want})");
        }
    }

    #[test]
    fn test_tile_paths() {
        let cases = [
            ("tile/4/0/001", tile!(4, 0, 1, 16)),
            ("tile/4/0/001.p/5", tile!(4, 0, 1, 5)),
            ("tile/3/5/x123/x456/078", tile!(3, 5, 123456078, 8)),
            ("tile/3/5/x123/x456/078.p/2", tile!(3, 5, 123456078, 2)),
            ("tile/1/0/x003/x057/500", tile!(1, 0, 3057500, 2)),
            ("tile/3/5/123/456/078", tile!(0, 0, 0, 0)),
            ("tile/3/-1/123/456/078", tile!(0, 0, 0, 0)),
            ("tile/1/data/x003/x057/500", tile!(1, -1, 3057500, 2)),
        ];
        for (path, tile) in cases {
            if tile.height > 0 {
                let got = tile.to_string();
                assert_eq!(got, path, "{tile:?}");
            }
            // TODO: parse
        }
    }
}
