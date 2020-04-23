use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::background::BackgroundRunner;
use crate::data::*;
use crate::error::Error;

use crate::table::table_sharded::*;
use crate::table::*;

use crate::store::block_ref_table::*;

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct Version {
	// Primary key
	pub uuid: UUID,

	// Actual data: the blocks for this version
	pub deleted: bool,
	blocks: Vec<VersionBlock>,

	// Back link to bucket+key so that we can figure if
	// this was deleted later on
	pub bucket: String,
	pub key: String,
}

impl Version {
	pub fn new(
		uuid: UUID,
		bucket: String,
		key: String,
		deleted: bool,
		blocks: Vec<VersionBlock>,
	) -> Self {
		let mut ret = Self {
			uuid,
			deleted,
			blocks: vec![],
			bucket,
			key,
		};
		for b in blocks {
			ret.add_block(b)
				.expect("Twice the same VersionBlock in Version constructor");
		}
		ret
	}
	/// Adds a block if it wasn't already present
	pub fn add_block(&mut self, new: VersionBlock) -> Result<(), ()> {
		match self.blocks.binary_search_by(|b| b.offset.cmp(&new.offset)) {
			Err(i) => {
				self.blocks.insert(i, new);
				Ok(())
			}
			Ok(_) => Err(()),
		}
	}
	pub fn blocks(&self) -> &[VersionBlock] {
		&self.blocks[..]
	}
}

#[derive(PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct VersionBlock {
	pub offset: u64,
	pub hash: Hash,
}

impl Entry<Hash, EmptyKey> for Version {
	fn partition_key(&self) -> &Hash {
		&self.uuid
	}
	fn sort_key(&self) -> &EmptyKey {
		&EmptyKey
	}

	fn merge(&mut self, other: &Self) {
		if other.deleted {
			self.deleted = true;
			self.blocks.clear();
		} else if !self.deleted {
			for bi in other.blocks.iter() {
				match self.blocks.binary_search_by(|x| x.offset.cmp(&bi.offset)) {
					Ok(_) => (),
					Err(pos) => {
						self.blocks.insert(pos, bi.clone());
					}
				}
			}
		}
	}
}

pub struct VersionTable {
	pub background: Arc<BackgroundRunner>,
	pub block_ref_table: Arc<Table<BlockRefTable, TableShardedReplication>>,
}

#[async_trait]
impl TableSchema for VersionTable {
	type P = Hash;
	type S = EmptyKey;
	type E = Version;
	type Filter = ();

	async fn updated(&self, old: Option<Self::E>, new: Option<Self::E>) -> Result<(), Error> {
		let block_ref_table = self.block_ref_table.clone();
		if let (Some(old_v), Some(new_v)) = (old, new) {
			// Propagate deletion of version blocks
			if new_v.deleted && !old_v.deleted {
				let deleted_block_refs = old_v
					.blocks
					.iter()
					.map(|vb| BlockRef {
						block: vb.hash,
						version: old_v.uuid,
						deleted: true,
					})
					.collect::<Vec<_>>();
				block_ref_table.insert_many(&deleted_block_refs[..]).await?;
			}
		}
		Ok(())
	}

	fn matches_filter(entry: &Self::E, _filter: &Self::Filter) -> bool {
		!entry.deleted
	}
}
