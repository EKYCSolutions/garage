use std::sync::Arc;

use garage_rpc::layout::*;
use garage_rpc::system::System;
use garage_util::data::*;

use crate::replication::*;

/// Sharded replication schema:
/// - based on the ring of nodes, a certain set of neighbors
///   store entries, given as a function of the position of the
///   entry's hash in the ring
/// - reads are done on all of the nodes that replicate the data
/// - writes as well
#[derive(Clone)]
pub struct TableShardedReplication {
	/// The membership manager of this node
	pub system: Arc<System>,
	/// How many time each data should be replicated
	pub replication_factor: usize,
	/// How many nodes to contact for a read, should be at most `replication_factor`
	pub read_quorum: usize,
	/// How many nodes to contact for a write, should be at most `replication_factor`
	pub write_quorum: usize,
}

impl TableReplication for TableShardedReplication {
	fn read_nodes(&self, hash: &Hash) -> Vec<Uuid> {
		self.system
			.cluster_layout()
			.current()
			.nodes_of(hash, self.replication_factor)
	}
	fn read_quorum(&self) -> usize {
		self.read_quorum
	}

	fn write_nodes(&self, hash: &Hash) -> Vec<Uuid> {
		self.system
			.cluster_layout()
			.current()
			.nodes_of(hash, self.replication_factor)
	}
	fn write_quorum(&self) -> usize {
		self.write_quorum
	}
	fn max_write_errors(&self) -> usize {
		self.replication_factor - self.write_quorum
	}

	fn partition_of(&self, hash: &Hash) -> Partition {
		self.system.cluster_layout().current().partition_of(hash)
	}

	fn sync_partitions(&self) -> SyncPartitions {
		let layout = self.system.cluster_layout();
		let layout_version = layout.all_ack();

		let mut partitions = layout
			.current()
			.partitions()
			.map(|(partition, first_hash)| {
				let mut storage_nodes = layout
					.write_sets_of(&first_hash)
					.map(|x| x.into_iter())
					.flatten()
					.collect::<Vec<_>>();
				storage_nodes.sort();
				storage_nodes.dedup();
				SyncPartition {
					partition,
					first_hash,
					last_hash: [0u8; 32].into(), // filled in just after
					storage_nodes,
				}
			})
			.collect::<Vec<_>>();

		for i in 0..partitions.len() {
			partitions[i].last_hash = if i + 1 < partitions.len() {
				partitions[i + 1].first_hash
			} else {
				[0xFFu8; 32].into()
			};
		}

		SyncPartitions {
			layout_version,
			partitions,
		}
	}
}
