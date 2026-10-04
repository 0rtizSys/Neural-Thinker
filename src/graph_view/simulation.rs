//! The force simulation: one step per frame until the layout settles.

use crate::graph::Graph;
use crate::graph_layout;

use super::{
    ALPHA_DECAY, ALPHA_MIN, GRAVITY, GraphView, LINK_DISTANCE, LINK_STRENGTH, REPULSION,
    VELOCITY_KEEP,
};

impl GraphView {
    pub(super) fn reheat(&mut self, alpha: f32) {
        self.alpha = self.alpha.max(alpha);
    }

    /// One step of the force simulation.
    // Index loops read best for the per-axis vector math here.
    #[allow(clippy::needless_range_loop)]
    pub(super) fn step(&mut self, three_d: bool) {
        let n = self.pos.len();
        if n == 0 {
            self.alpha = 0.0;
            return;
        }
        let dims = if three_d { 3 } else { 2 };
        let alpha = self.alpha;
        let mut force = vec![[0.0f32; 3]; n];
        graph_layout::repel(&self.pos, dims, REPULSION, &mut force);

        for &(a, b) in &self.graph.edges {
            let mut d = [0.0; 3];
            let mut dist2 = 0.0;
            for k in 0..dims {
                d[k] = self.pos[b][k] - self.pos[a][k];
                dist2 += d[k] * d[k];
            }
            let dist = dist2.sqrt().max(0.01);
            let pull = (dist - LINK_DISTANCE) * LINK_STRENGTH / dist;
            for k in 0..dims {
                force[a][k] += d[k] * pull;
                force[b][k] -= d[k] * pull;
            }
        }

        for i in 0..n {
            if Some(i) == self.dragged_node {
                self.vel[i] = [0.0; 3];
                continue;
            }
            for k in 0..dims {
                let f = force[i][k] - self.pos[i][k] * GRAVITY;
                // Clamp so a bad start cannot fling nodes far away.
                let v = ((self.vel[i][k] + f * alpha) * VELOCITY_KEEP).clamp(-40.0, 40.0);
                self.vel[i][k] = v;
                self.pos[i][k] += v;
            }
            if !three_d {
                self.pos[i][2] = 0.0;
            }
        }
        self.alpha -= self.alpha * ALPHA_DECAY;
        if self.alpha < ALPHA_MIN {
            self.alpha = 0.0;
        }
    }
}

/// Neighbour lists of every node, packed (CSR).
pub(super) fn adjacency(graph: &Graph) -> (Vec<u32>, Vec<u32>) {
    let n = graph.nodes.len();
    let mut start = vec![0u32; n + 1];
    for &(a, b) in &graph.edges {
        start[a + 1] += 1;
        start[b + 1] += 1;
    }
    for i in 0..n {
        start[i + 1] += start[i];
    }
    let mut fill = start.clone();
    let mut adjacent = vec![0u32; graph.edges.len() * 2];
    for &(a, b) in &graph.edges {
        adjacent[fill[a] as usize] = b as u32;
        fill[a] += 1;
        adjacent[fill[b] as usize] = a as u32;
        fill[b] += 1;
    }
    (start, adjacent)
}

/// Deterministic starting point: a sunflower spiral, with depth in 3D.
pub(super) fn seed_position(i: usize, three_d: bool) -> [f32; 3] {
    let golden = std::f32::consts::PI * (3.0 - 5f32.sqrt());
    let r = 12.0 * (0.5 + i as f32).sqrt();
    let a = i as f32 * golden;
    let z = if three_d {
        // A cheap hash keeps the spread stable between runs.
        let h = (i as u32).wrapping_mul(2_654_435_761) >> 16;
        (h % 200) as f32 - 100.0
    } else {
        0.0
    };
    [r * a.cos(), r * a.sin(), z]
}
