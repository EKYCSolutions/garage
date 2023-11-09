use std::sync::Arc;

use hyper::{Body, Response};
use serde::Serialize;

use garage_util::data::*;

use garage_table::util::*;

use garage_model::garage::Garage;
use garage_model::k2v::item_table::{BYTES, CONFLICTS, ENTRIES, VALUES};

use crate::helpers::*;
use crate::k2v::error::*;
use crate::k2v::range::read_range;

pub async fn handle_read_index(
	garage: Arc<Garage>,
	bucket_id: Uuid,
	prefix: Option<String>,
	start: Option<String>,
	end: Option<String>,
	limit: Option<u64>,
	reverse: Option<bool>,
) -> Result<Response<Body>, Error> {
	let reverse = reverse.unwrap_or(false);

	// TODO: not only current
	let node_id_vec = garage.system.cluster_layout().current().node_ids().to_vec();

	let (partition_keys, more, next_start) = read_range(
		&garage.k2v.counter_table.table,
		&bucket_id,
		&prefix,
		&start,
		&end,
		limit,
		Some((DeletedFilter::NotDeleted, node_id_vec)),
		EnumerationOrder::from_reverse(reverse),
	)
	.await?;

	let s_entries = ENTRIES.to_string();
	let s_conflicts = CONFLICTS.to_string();
	let s_values = VALUES.to_string();
	let s_bytes = BYTES.to_string();

	let resp = ReadIndexResponse {
		prefix,
		start,
		end,
		limit,
		reverse,
		partition_keys: partition_keys
			.into_iter()
			.map(|part| {
				let vals = part.filtered_values(&garage.system.cluster_layout());
				ReadIndexResponseEntry {
					pk: part.sk,
					entries: *vals.get(&s_entries).unwrap_or(&0),
					conflicts: *vals.get(&s_conflicts).unwrap_or(&0),
					values: *vals.get(&s_values).unwrap_or(&0),
					bytes: *vals.get(&s_bytes).unwrap_or(&0),
				}
			})
			.collect::<Vec<_>>(),
		more,
		next_start,
	};

	Ok(json_ok_response(&resp)?)
}

#[derive(Serialize)]
struct ReadIndexResponse {
	prefix: Option<String>,
	start: Option<String>,
	end: Option<String>,
	limit: Option<u64>,
	reverse: bool,

	#[serde(rename = "partitionKeys")]
	partition_keys: Vec<ReadIndexResponseEntry>,

	more: bool,
	#[serde(rename = "nextStart")]
	next_start: Option<String>,
}

#[derive(Serialize)]
struct ReadIndexResponseEntry {
	pk: String,
	entries: i64,
	conflicts: i64,
	values: i64,
	bytes: i64,
}
