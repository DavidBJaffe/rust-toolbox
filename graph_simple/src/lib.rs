// Copyright (c) 2018 10X Genomics, Inc. All rights reserved.
// Some code here is based on the BroadCRD codebase.

// Define generic digraph functions.
//
// Some of the functions here are unnecessarily quadratic in the vertex degree for
// petgraph as a result of calling v_from and related functions below.  The
// quadratic behavior might be avoided but might make the code a bit less readable.
//
// These functions seem unnecessarily specialized to u32.

use petgraph::{prelude::*, EdgeType};
use std::collections::HashSet;
use vector_utils::{bin_member, meet};

pub trait GraphSimple<T> {
    // =============================================================================
    // Return the object associated to an edge id.
    // =============================================================================

    fn edge_obj(&self, e: u32) -> &T;

    // =============================================================================
    // Return the source or target of an edge.
    // =============================================================================

    fn to_left(&self, e: u32) -> u32;
    fn to_right(&self, e: u32) -> u32;

    // =============================================================================
    // Return the number of edges exiting or entering a given vertex.
    // =============================================================================

    fn n_from(&self, v: usize) -> usize;
    fn n_to(&self, v: usize) -> usize;

    // =============================================================================
    // Return id of the nth vertex exiting or entering a given vertex id.
    // Note that this is O(n).
    // =============================================================================

    fn v_from(&self, v: usize, n: usize) -> usize;
    fn v_to(&self, v: usize, n: usize) -> usize;

    // =============================================================================
    // Return id of the nth edge exiting or entering a given vertex id.
    // Note that this is O(n).
    // =============================================================================

    fn e_from(&self, v: usize, n: usize) -> usize;
    fn e_to(&self, v: usize, n: usize) -> usize;

    // =============================================================================
    // Return the nth edge exiting or entering a given vertex id.
    // Note that this is O(n).
    // =============================================================================

    fn o_from(&self, v: usize, n: usize) -> &T;
    fn o_to(&self, v: usize, n: usize) -> &T;

    // =============================================================================
    // source: return if a vertex is a source
    // sink: return if a vertex is a sink
    // =============================================================================

    fn source(&self, v: i32) -> bool;
    fn sink(&self, v: i32) -> bool;

    // =============================================================================
    // sources: return the ordered list of source vertices
    // sinks: return the ordered list of sink vertices
    // =============================================================================

    fn sources(&self) -> Vec<i32>;
    fn sinks(&self) -> Vec<i32>;

    // =============================================================================
    // cyclic_core: return the ordered list of vertices that define a subgraph having no
    // sources and sinks, and which is empty iff the graph is acyclic.
    // The (vertex) cyclic core is the union of all vertices that appear in cycles.
    // =============================================================================

    fn cyclic_core(&self) -> Vec<i32>;

    // =============================================================================
    // cyclic_core_edges: return the ordered list of edges that define a subgraph having no
    // sources and sinks, and which is empty iff the graph is acyclic.
    // The (edge) cyclic core is the union of all edges that appear in cycles.
    // =============================================================================

    fn cyclic_core_edges(&self) -> Vec<u32>;

    // =============================================================================
    // acyclic: return true if graph is acyclic
    // =============================================================================

    fn acyclic(&self) -> bool;

    // =============================================================================
    // get_predecessors: find all vertices which have a directed path to a vertex
    // in v.  This includes the vertices in v by definition.  Return a sorted list
    // x.  get_successors: go the other way.
    // get_predecessors1 and get_successors1: start from one vertex
    // =============================================================================

    fn get_predecessors(&self, v: &[i32], x: &mut Vec<u32>);
    fn get_predecessors1(&self, v: i32, x: &mut Vec<u32>);
    fn get_successors(&self, v: &[i32], x: &mut Vec<u32>);
    fn get_successors1(&self, v: i32, x: &mut Vec<u32>);

    // =============================================================================
    // Determine if there is a path from one vertex to another, allowing for the
    // case of a zero length path, where the vertices are equal.
    // =============================================================================

    fn have_path(&self, v: i32, w: i32) -> bool;

    // =============================================================================
    // Find the connected components.  Each component is a sorted list of vertices.
    // =============================================================================

    fn components(&self, comp: &mut Vec<Vec<u32>>);

    // =============================================================================
    // Find the connected components as lists of edges.  Each component is an
    // UNSORTED list of edges.
    // =============================================================================

    fn components_e(&self, comp: &mut Vec<Vec<u32>>);

    // =============================================================================
    // Find the connected components as lists of edges, sorted within each component
    // to try to follow the order of the graph.  This is slow and suboptimal.
    // =============================================================================

    fn components_e_pos_sorted(&self, comp: &mut Vec<Vec<u32>>);
}

impl<S, T, U, V> GraphSimple<T> for Graph<S, T, U, V>
where
    U: EdgeType,
    V: petgraph::csr::IndexType,
{
    fn edge_obj(&self, e: u32) -> &T {
        &self[EdgeIndex::<V>::new(e as usize)]
    }

    fn to_left(&self, e: u32) -> u32 {
        self.edge_endpoints(EdgeIndex::<V>::new(e as usize))
            .unwrap()
            .0
            .index() as u32
    }

    fn to_right(&self, e: u32) -> u32 {
        self.edge_endpoints(EdgeIndex::<V>::new(e as usize))
            .unwrap()
            .1
            .index() as u32
    }

    fn n_from(&self, v: usize) -> usize {
        self.neighbors(NodeIndex::<V>::new(v)).count()
    }

    fn n_to(&self, v: usize) -> usize {
        self.neighbors_directed(NodeIndex::<V>::new(v), Incoming)
            .count()
    }

    fn v_from(&self, v: usize, n: usize) -> usize {
        self.edges_directed(NodeIndex::<V>::new(v), Outgoing)
            .nth(n)
            .unwrap()
            .target()
            .index()
    }

    fn v_to(&self, v: usize, n: usize) -> usize {
        self.edges_directed(NodeIndex::<V>::new(v), Incoming)
            .nth(n)
            .unwrap()
            .source()
            .index()
    }

    fn e_from(&self, v: usize, n: usize) -> usize {
        let mut e: EdgeIndex<V> = self.first_edge(NodeIndex::<V>::new(v), Outgoing).unwrap();
        for _j in 0..n {
            let f = self.next_edge(e, Outgoing).unwrap();
            e = f;
        }
        e.index()
    }

    fn e_to(&self, v: usize, n: usize) -> usize {
        let mut e: EdgeIndex<V> = self.first_edge(NodeIndex::<V>::new(v), Incoming).unwrap();
        for _j in 0..n {
            let f = self.next_edge(e, Incoming).unwrap();
            e = f;
        }
        e.index()
    }

    fn o_from(&self, v: usize, n: usize) -> &T {
        self.edge_obj(self.e_from(v, n) as u32)
    }

    fn o_to(&self, v: usize, n: usize) -> &T {
        self.edge_obj(self.e_to(v, n) as u32)
    }

    fn source(&self, v: i32) -> bool {
        self.n_to(v as usize) == 0
    }

    fn sink(&self, v: i32) -> bool {
        self.n_from(v as usize) == 0
    }

    fn sources(&self) -> Vec<i32> {
        let mut s = Vec::<i32>::new();
        for v in 0..self.node_count() as i32 {
            if self.source(v) {
                s.push(v);
            }
        }
        s
    }

    fn sinks(&self) -> Vec<i32> {
        let mut s = Vec::<i32>::new();
        for v in 0..self.node_count() as i32 {
            if self.sink(v) {
                s.push(v);
            }
        }
        s
    }

    // cyclic_core successively deletes vertices and edges from the graph, without actually
    // deleting them, but tracking instead the number of edges entering and exiting each vertex.

    fn cyclic_core(&self) -> Vec<i32> {
        let n = self.node_count();
        let (mut sources, mut sinks) = (self.sources(), self.sinks());
        let (mut ins, mut outs) = (vec![0; n], vec![0; n]);
        for v in 0..n {
            ins[v] = self.n_to(v);
            outs[v] = self.n_from(v);
        }
        let mut i = 0;
        while i < sources.len() {
            let v = sources[i] as usize;
            outs[v] = 0;
            for j in 0..self.n_from(v) {
                let w = self.v_from(v, j);
                ins[w] -= 1;
                if ins[w] == 0 {
                    sources.push(w as i32);
                }
            }
            i += 1;
        }
        let mut i = 0;
        while i < sinks.len() {
            let v = sinks[i] as usize;
            if ins[v] == 0 {
                i += 1;
                continue;
            }
            for j in 0..self.n_to(v) {
                let w = self.v_to(v, j);
                if ins[w] == 0 {
                    continue;
                }
                outs[w] -= 1;
                if outs[w] == 0 {
                    sinks.push(w as i32);
                }
            }
            i += 1;
        }
        let mut core = Vec::<i32>::new();
        for v in 0..n {
            if ins[v] > 0 && outs[v] > 0 {
                core.push(v as i32);
            }
        }
        core
    }

    fn cyclic_core_edges(&self) -> Vec<u32> {
        let vert_core = self.cyclic_core();
        let mut edge_core = Vec::<u32>::new();
        for v in vert_core.iter() {
            for i in 0..self.n_from(*v as usize) {
                let e = self.e_from(*v as usize, i);
                let w = self.to_right(e as u32);
                if bin_member(&vert_core, &(w as i32)) {
                    edge_core.push(e as u32);
                }
            }
        }
        edge_core.sort();
        edge_core
    }

    fn acyclic(&self) -> bool {
        self.cyclic_core().is_empty()
    }

    fn get_predecessors(&self, v: &[i32], x: &mut Vec<u32>) {
        let mut check: Vec<u32> = Vec::new();
        let mut tov: HashSet<u32> = HashSet::new();
        for j in 0..v.len() {
            let s: u32 = v[j] as u32;
            check.push(s);
            tov.insert(s);
        }
        while !check.is_empty() {
            let x = check.pop().unwrap();
            let n = self.n_to(x as usize);
            for i in 0..n {
                let y = self.v_to(x as usize, i);
                if tov.contains(&(y as u32)) {
                    continue;
                }
                check.push(y as u32);
                tov.insert(y as u32);
            }
        }
        x.clear();
        for v in tov {
            x.push(v);
        }
        x.sort_unstable();
    }

    fn get_predecessors1(&self, v: i32, x: &mut Vec<u32>) {
        let vs = vec![v];
        self.get_predecessors(&vs, x);
    }

    fn get_successors(&self, v: &[i32], x: &mut Vec<u32>) {
        let mut check: Vec<u32> = Vec::new();
        let mut fromv: HashSet<u32> = HashSet::new();
        for j in 0..v.len() {
            let s: u32 = v[j] as u32;
            check.push(s);
            fromv.insert(s);
        }
        while !check.is_empty() {
            let x = check.pop().unwrap();
            let n = self.n_from(x as usize);
            for i in 0..n {
                let y = self.v_from(x as usize, i);
                if fromv.contains(&(y as u32)) {
                    continue;
                }
                check.push(y as u32);
                fromv.insert(y as u32);
            }
        }
        x.clear();
        for v in fromv {
            x.push(v);
        }
        x.sort_unstable();
    }

    fn get_successors1(&self, v: i32, x: &mut Vec<u32>) {
        let vs = vec![v];
        self.get_successors(&vs, x);
    }

    fn have_path(&self, v: i32, w: i32) -> bool {
        let mut vsuc: Vec<u32> = Vec::new();
        self.get_successors1(v, &mut vsuc);
        let mut wpre: Vec<u32> = Vec::new();
        self.get_predecessors1(w, &mut wpre);
        meet(&vsuc, &wpre)
    }

    fn components(&self, comp: &mut Vec<Vec<u32>>) {
        comp.clear();
        let mut used: Vec<bool> = vec![false; self.node_count()];
        let mut c: Vec<u32> = Vec::new();
        let mut cnext: Vec<u32> = Vec::new();
        for v in 0..self.node_count() {
            if used[v] {
                continue;
            }
            c.clear();
            cnext.clear();
            cnext.push(v as u32);
            while !cnext.is_empty() {
                let w = cnext.pop().unwrap();
                if used[w as usize] {
                    continue;
                }
                used[w as usize] = true;
                c.push(w);
                let n = self.n_from(w as usize);
                for j in 0..n {
                    cnext.push(self.v_from(w as usize, j) as u32);
                }
                let n = self.n_to(w as usize);
                for j in 0..n {
                    cnext.push(self.v_to(w as usize, j) as u32);
                }
            }
            c.sort_unstable();
            comp.push(c.clone());
        }
    }

    fn components_e(&self, comp: &mut Vec<Vec<u32>>) {
        self.components(comp);
        for j in 0..comp.len() {
            let mut c = Vec::<u32>::new();
            for i in 0..comp[j].len() {
                let v = comp[j][i];
                let n = self.n_from(v as usize);
                for l in 0..n {
                    c.push(self.e_from(v as usize, l) as u32);
                }
            }
            comp[j] = c;
        }
    }

    fn components_e_pos_sorted(&self, comp: &mut Vec<Vec<u32>>) {
        self.components_e(comp);
        for u in 0..comp.len() {
            comp[u].sort_by(|a, b| {
                if a == b {
                    return std::cmp::Ordering::Equal;
                }
                let v = self.to_right(*a);
                let w = self.to_left(*b);
                if self.have_path(v as i32, w as i32) {
                    return std::cmp::Ordering::Less;
                }
                let v = self.to_right(*b);
                let w = self.to_left(*a);
                if self.have_path(v as i32, w as i32) {
                    return std::cmp::Ordering::Greater;
                }
                std::cmp::Ordering::Equal
            });
        }
    }
}

// tests can be run with
// cargo test -p graph_simple -- --nocapture

#[cfg(test)]
mod tests {
    #[test]
    fn test_cyclic_core() {
        use crate::GraphSimple;
        use petgraph::graph::DiGraph;
        let g = DiGraph::<i32, ()>::from_edges(&[(0, 1), (1, 2), (2, 3), (3, 0)]);
        let core = g.cyclic_core();
        assert_eq!(core.len(), 4);
        let g = DiGraph::<i32, ()>::from_edges(&[
            (0, 1),
            (1, 2),
            (1, 3),
            (2, 3),
            (2, 4),
            (3, 4),
            (4, 5),
        ]);
        let core = g.cyclic_core();
        assert_eq!(core.len(), 0);
    }
}
