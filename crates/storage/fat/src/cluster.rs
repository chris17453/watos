//! Cluster chain operations

use alloc::vec::Vec;
use watos_vfs::VfsResult;
use watos_driver_traits::block::BlockDevice;

use crate::bpb::{BiosParameterBlock, FatType};
use crate::table;

/// Represents a chain of clusters for a file
#[derive(Debug)]
pub struct ClusterChain {
    /// Starting cluster
    pub start: u32,
    /// All clusters in the chain
    pub clusters: Vec<u32>,
    /// Cluster size in bytes
    pub cluster_size: u32,
}

impl ClusterChain {
    /// Build cluster chain starting from a cluster
    pub fn build<D: BlockDevice>(
        device: &mut D,
        bpb: &BiosParameterBlock,
        fat_type: FatType,
        start_cluster: u32,
    ) -> VfsResult<Self> {
        let cluster_size = bpb.bytes_per_sector as u32 * bpb.sectors_per_cluster as u32;
        let mut clusters = Vec::new();

        if start_cluster < 2 {
            // Invalid start cluster
            return Ok(ClusterChain {
                start: start_cluster,
                clusters,
                cluster_size,
            });
        }

        let mut current = start_cluster;
        clusters.push(current);

        // Follow the chain
        while let Some(next) = table::read_fat_entry(device, bpb, fat_type, current)? {
            if next < 2 || table::is_end_of_chain(fat_type, next) {
                break;
            }
            clusters.push(next);
            current = next;

            // Safety limit to prevent infinite loops
            if clusters.len() > 1_000_000 {
                break;
            }
        }

        Ok(ClusterChain {
            start: start_cluster,
            clusters,
            cluster_size,
        })
    }

    /// Get total size of the cluster chain in bytes
    pub fn size(&self) -> u64 {
        self.clusters.len() as u64 * self.cluster_size as u64
    }

    /// Get the cluster number for a given byte offset
    pub fn cluster_at_offset(&self, offset: u64) -> Option<u32> {
        let cluster_index = (offset / self.cluster_size as u64) as usize;
        self.clusters.get(cluster_index).copied()
    }

    /// Get the offset within a cluster for a given byte offset
    pub fn offset_in_cluster(&self, offset: u64) -> u32 {
        (offset % self.cluster_size as u64) as u32
    }

    /// Check if the chain is empty
    pub fn is_empty(&self) -> bool {
        self.clusters.is_empty()
    }

    /// Get the number of clusters in the chain
    pub fn len(&self) -> usize {
        self.clusters.len()
    }
}
