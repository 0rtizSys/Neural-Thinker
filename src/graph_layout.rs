//! Node repulsion for the graph layout.
//!
//! Small graphs use the exact O(n²) sum. Larger ones use Barnes-Hut: nodes
//! are sorted into a quadtree (2D) or octree (3D), and a distant cell pushes
//! as one body of its total mass placed at its centre of mass. That is
//! O(n log n) per step and keeps vaults of thousands of notes interactive.

/// Below this many nodes the exact sum is both cheaper and exact.
pub const EXACT_MAX_NODES: usize = 400;
/// Barnes-Hut accuracy: a cell is treated as one body when
/// `cell size / distance < THETA`. Lower is more exact and slower.
const THETA: f32 = 0.8;
/// Cells with this many nodes or fewer are not split further.
const LEAF_SIZE: usize = 8;
/// Bounds recursion when many nodes share one spot.
const MAX_DEPTH: u32 = 20;
/// From this many nodes the forces are computed on several threads...
const PARALLEL_MIN_NODES: usize = 2000;
/// ...but no more than this many, to stay light on the rest of the machine.
const MAX_THREADS: usize = 4;

/// Adds the repulsion between every pair of nodes to `force`.
/// `strength` is the push between two nodes at distance 1; it falls off with
/// the square of the distance. Only the first `dims` axes are used.
pub fn repel(pos: &[[f32; 3]], dims: usize, strength: f32, force: &mut [[f32; 3]]) {
    if pos.len() <= EXACT_MAX_NODES {
        repel_exact(pos, dims, strength, force);
    } else {
        Tree::build(pos, dims).repel(pos, strength, THETA, force);
    }
}

/// Push of `j` on `i`, given `d = pos[i] - pos[j]`.
#[inline]
fn pair(mut d: [f32; 3], i: usize, j: usize, dims: usize, strength: f32) -> [f32; 3] {
    let mut dist2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
    if dist2 < 1.0 {
        // Coincident nodes: nudge apart deterministically, in opposite
        // directions for the two nodes of the pair.
        let s = if i < j { 1.0 } else { -1.0 };
        d = [-0.4 * s, 0.3 * s, 0.0];
        dist2 = 1.0;
    }
    let s = strength / (dist2 * dist2.sqrt());
    let mut out = [d[0] * s, d[1] * s, d[2] * s];
    if dims < 3 {
        out[2] = 0.0;
    }
    out
}

#[inline]
fn diff(a: [f32; 3], b: [f32; 3], dims: usize) -> [f32; 3] {
    let mut d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    if dims < 3 {
        d[2] = 0.0;
    }
    d
}

pub fn repel_exact(pos: &[[f32; 3]], dims: usize, strength: f32, force: &mut [[f32; 3]]) {
    let n = pos.len();
    for i in 0..n {
        for j in i + 1..n {
            let push = pair(diff(pos[i], pos[j], dims), i, j, dims, strength);
            for k in 0..3 {
                force[i][k] += push[k];
                force[j][k] -= push[k];
            }
        }
    }
}

struct Cell {
    /// Centre of mass.
    com: [f32; 3],
    mass: f32,
    /// Squared edge length of the cell's cube.
    size2: f32,
    /// Leaf: range of `Tree::order` holding its nodes. Inner: range of
    /// `Tree::children` holding its child cells.
    start: u32,
    end: u32,
    leaf: bool,
}

struct Tree {
    cells: Vec<Cell>,
    children: Vec<u32>,
    order: Vec<u32>,
    dims: usize,
}

impl Tree {
    fn build(pos: &[[f32; 3]], dims: usize) -> Self {
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        for p in pos {
            for k in 0..dims {
                min[k] = min[k].min(p[k]);
                max[k] = max[k].max(p[k]);
            }
        }
        let size = (0..dims).map(|k| max[k] - min[k]).fold(1.0f32, f32::max);
        let mut tree = Tree {
            cells: Vec::with_capacity(pos.len() / 2),
            children: Vec::with_capacity(pos.len()),
            order: (0..pos.len() as u32).collect(),
            dims,
        };
        let mut scratch = vec![0u32; pos.len()];
        tree.split(pos, &mut scratch, 0, pos.len(), min, size, 0);
        tree
    }

    /// Creates the cell for `order[start..end]`, a cube at `min` of edge
    /// `size`, and its subtree. Returns the cell index.
    #[allow(clippy::too_many_arguments)]
    fn split(
        &mut self,
        pos: &[[f32; 3]],
        scratch: &mut [u32],
        start: usize,
        end: usize,
        min: [f32; 3],
        size: f32,
        depth: u32,
    ) -> u32 {
        let id = self.cells.len() as u32;
        let mut com = [0.0f32; 3];
        for &i in &self.order[start..end] {
            for k in 0..3 {
                com[k] += pos[i as usize][k];
            }
        }
        let mass = (end - start) as f32;
        for c in &mut com {
            *c /= mass;
        }
        self.cells.push(Cell {
            com,
            mass,
            size2: size * size,
            start: start as u32,
            end: end as u32,
            leaf: true,
        });
        if end - start <= LEAF_SIZE || depth >= MAX_DEPTH {
            return id;
        }

        // Counting sort of the nodes into the 2^dims octants.
        let half = size / 2.0;
        let mid = [min[0] + half, min[1] + half, min[2] + half];
        let octant = |p: &[f32; 3]| {
            (0..self.dims).fold(0usize, |o, k| o | (((p[k] >= mid[k]) as usize) << k))
        };
        let mut count = [0usize; 8];
        for &i in &self.order[start..end] {
            count[octant(&pos[i as usize])] += 1;
        }
        let mut offset = [0usize; 8];
        let mut acc = start;
        for o in 0..8 {
            offset[o] = acc;
            acc += count[o];
        }
        let mut fill = offset;
        for &i in &self.order[start..end] {
            let o = octant(&pos[i as usize]);
            scratch[fill[o]] = i;
            fill[o] += 1;
        }
        self.order[start..end].copy_from_slice(&scratch[start..end]);

        let mut kids = [0u32; 8];
        let mut nkids = 0;
        for o in 0..1usize << self.dims {
            if count[o] == 0 {
                continue;
            }
            let mut cmin = min;
            for (k, c) in cmin.iter_mut().enumerate().take(self.dims) {
                if o >> k & 1 == 1 {
                    *c += half;
                }
            }
            let s = offset[o];
            kids[nkids] = self.split(pos, scratch, s, s + count[o], cmin, half, depth + 1);
            nkids += 1;
        }
        let first = self.children.len() as u32;
        self.children.extend_from_slice(&kids[..nkids]);
        let cell = &mut self.cells[id as usize];
        cell.leaf = false;
        cell.start = first;
        cell.end = first + nkids as u32;
        id
    }

    fn repel(&self, pos: &[[f32; 3]], strength: f32, theta: f32, force: &mut [[f32; 3]]) {
        let threads = if pos.len() >= PARALLEL_MIN_NODES {
            std::thread::available_parallelism()
                .map_or(1, |n| n.get())
                .min(MAX_THREADS)
        } else {
            1
        };
        if threads <= 1 {
            self.repel_range(pos, 0, strength, theta, force);
            return;
        }
        // Each node's force only reads the tree, so ranges of nodes run on
        // separate threads, each writing its own slice of `force`.
        let chunk = pos.len().div_ceil(threads);
        std::thread::scope(|scope| {
            for (n, out) in force.chunks_mut(chunk).enumerate() {
                scope.spawn(move || self.repel_range(pos, n * chunk, strength, theta, out));
            }
        });
    }

    /// Adds the push on nodes `first..first + force.len()` to `force`.
    fn repel_range(
        &self,
        pos: &[[f32; 3]],
        first: usize,
        strength: f32,
        theta: f32,
        force: &mut [[f32; 3]],
    ) {
        let theta2 = theta * theta;
        let mut stack = Vec::with_capacity(64);
        for (out, i) in force.iter_mut().zip(first..) {
            let p = pos[i];
            let mut f = [0.0f32; 3];
            stack.clear();
            stack.push(0u32);
            while let Some(c) = stack.pop() {
                let cell = &self.cells[c as usize];
                if cell.leaf {
                    for &j in &self.order[cell.start as usize..cell.end as usize] {
                        let j = j as usize;
                        if j != i {
                            let push = pair(diff(p, pos[j], self.dims), i, j, self.dims, strength);
                            for k in 0..3 {
                                f[k] += push[k];
                            }
                        }
                    }
                    continue;
                }
                let d = diff(p, cell.com, self.dims);
                let dist2 = d[0] * d[0] + d[1] * d[1] + d[2] * d[2];
                if cell.size2 < theta2 * dist2 {
                    // Far enough: the whole cell pushes as one body.
                    let s = strength * cell.mass / (dist2 * dist2.sqrt());
                    for k in 0..3 {
                        f[k] += d[k] * s;
                    }
                } else {
                    stack.extend_from_slice(&self.children[cell.start as usize..cell.end as usize]);
                }
            }
            for k in 0..3 {
                out[k] += f[k];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cloud(n: usize, dims: usize) -> Vec<[f32; 3]> {
        let mut s = 12345u32;
        let mut r = move || {
            s = s.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (s >> 8) as f32 / (1u32 << 24) as f32 * 800.0 - 400.0
        };
        (0..n)
            .map(|_| {
                let p = [r(), r(), r()];
                [p[0], p[1], if dims == 3 { p[2] } else { 0.0 }]
            })
            .collect()
    }

    #[test]
    fn barnes_hut_matches_exact_sum() {
        for dims in [2, 3] {
            let pos = cloud(1500, dims);
            let mut exact = vec![[0.0; 3]; pos.len()];
            repel_exact(&pos, dims, 1600.0, &mut exact);
            let mut approx = vec![[0.0; 3]; pos.len()];
            Tree::build(&pos, dims).repel(&pos, 1600.0, THETA, &mut approx);

            // Relative error of the total force field stays small.
            let (mut err, mut norm) = (0.0f64, 0.0f64);
            for (e, a) in exact.iter().zip(&approx) {
                for k in 0..3 {
                    err += ((e[k] - a[k]) as f64).powi(2);
                    norm += (e[k] as f64).powi(2);
                }
                if dims == 2 {
                    assert_eq!(a[2], 0.0);
                }
            }
            let rel = (err / norm).sqrt();
            assert!(rel < 0.05, "{dims}D relative error {rel}");
        }
    }

    #[test]
    fn coincident_nodes_stay_finite() {
        let pos = vec![[5.0, 5.0, 0.0]; 600];
        let mut force = vec![[0.0; 3]; pos.len()];
        repel(&pos, 2, 1600.0, &mut force);
        assert!(force.iter().flatten().all(|v| v.is_finite()));
        assert!(force.iter().any(|f| f[0] != 0.0));
    }
}
