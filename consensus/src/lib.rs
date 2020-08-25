// This file is part of Frontier.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use std::sync::Arc;
use std::collections::HashMap;
use std::marker::PhantomData;
use frontier_consensus_primitives::{FRONTIER_ENGINE_ID, ConsensusLog};
use sc_client_api::{BlockOf, backend::AuxStore};
use sp_blockchain::{HeaderBackend, ProvideCache, well_known_cache_keys::Id as CacheKeyId};
use sp_block_builder::BlockBuilder as BlockBuilderApi;
use sp_runtime::generic::OpaqueDigestItemId;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_api::ProvideRuntimeApi;
use sp_consensus::{
	BlockImportParams, Error as ConsensusError, BlockImport,
	BlockCheckParams, ImportResult,
};
use log::*;
use sc_client_api;

#[derive(derive_more::Display, Debug)]
pub enum Error {
	#[display(fmt = "Multiple post-runtime Ethereum blocks, rejecting!")]
	MultiplePostRuntimeBlocks,
	#[display(fmt = "Post-runtime Ethereum block not found, rejecting!")]
	NoPostRuntimeBlock,
}

pub struct FrontierBlockImport<B: BlockT, I, C> {
	inner: I,
	client: Arc<C>,
	_marker: PhantomData<B>,
}

impl<B, I, C> FrontierBlockImport<B, I, C> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
{
	pub fn new(
		inner: I,
		client: Arc<C>,
	) -> Self {
		Self {
			inner,
			client,
			_marker: PhantomData,
		}
	}
}

impl<B, I, C> BlockImport<B> for FrontierBlockImport<B, I, C> where
	B: BlockT,
	I: BlockImport<B, Transaction = sp_api::TransactionFor<C, B>> + Send + Sync,
	I::Error: Into<ConsensusError>,
	C: ProvideRuntimeApi<B> + Send + Sync + HeaderBackend<B> + AuxStore + ProvideCache<B> + BlockOf,
	C::Api: BlockBuilderApi<B, Error = sp_blockchain::Error>,
{
	type Error = ConsensusError;
	type Transaction = sp_api::TransactionFor<C, B>;

	fn check_block(
		&mut self,
		block: BlockCheckParams<B>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).map_err(Into::into)
	}

	fn import_block(
		&mut self,
		block: BlockImportParams<B, Self::Transaction>,
		new_cache: HashMap<CacheKeyId, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.import_block(block, new_cache).map_err(Into::into)
	}
}

fn find_ethereum_block<B: BlockT>(
	header: &B::Header,
) -> Result<ethereum::Block, Error> {
	let mut ethereum_block: Option<_> = None;
	for log in header.digest().logs() {
		trace!(target: "frontier-consensus", "Checking log {:?}, looking for ethereum block.", log);
		let log = log.try_to::<ConsensusLog>(OpaqueDigestItemId::Consensus(&FRONTIER_ENGINE_ID));
		match (log, ethereum_block.is_some()) {
			(Some(ConsensusLog::Block(_)), true) =>
				return Err(Error::MultiplePostRuntimeBlocks),
			(Some(ConsensusLog::Block(block)), false) => ethereum_block = Some(block),
			_ => trace!(target: "frontier-consensus", "Ignoring digest not meant for us"),
		}
	}

	Ok(ethereum_block.ok_or(Error::NoPostRuntimeBlock)?)
}
