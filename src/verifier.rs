use hex;
use std::convert::TryInto;

use bitcoin::{BlockHash, TxOut, Transaction};
use bitcoin::consensus::deserialize;
use lightning::chain;
use lightning::chain::AccessError;
use lightning_block_sync::http::BinaryResponse;
use lightning_block_sync::rest::RestClient;

use crate::config;

pub(crate) struct ChainVerifier {
	rest_client: RestClient,
}

struct RestBinaryResponse(Vec<u8>);

impl ChainVerifier {
	pub(crate) fn new() -> Self {
		ChainVerifier {
			rest_client: RestClient::new(config::middleware_rest_endpoint()).unwrap(),
		}
	}

	fn retrieve_tx(&self, block_height: u32, transaction_index: u32) -> Result<Transaction, AccessError> {
		tokio::task::block_in_place(move || { tokio::runtime::Handle::current().block_on(async move {
			let uri = format!("getTransaction/{}/{}", block_height, transaction_index);
			let tx_result =
				self.rest_client.request_resource::<BinaryResponse, RestBinaryResponse>(&uri).await;
			let tx_hex_in_bytes: Vec<u8> = tx_result.map_err(|error| {
				eprintln!("Could't find transaction at height {} and pos {}: {}", block_height, transaction_index, error.to_string());
				AccessError::UnknownChain
			})?.0;

			let tx_hex_in_string =
				String::from_utf8(tx_hex_in_bytes)
					.map_err(|non_utf8| String::from_utf8_lossy(non_utf8.as_bytes()).into_owned())
					.unwrap();

			let tx_bytes =
				hex::decode(tx_hex_in_string)
					.map_err(|error| {
						eprintln!("Could't find transaction at height {} and pos {}: {}", block_height, transaction_index, error.to_string());
						AccessError::UnknownChain
					})
					.unwrap();

			let transaction =
				deserialize::<Transaction>(tx_bytes.as_slice())
					.map_err(|error| {
						eprintln!("Could't find transaction at height {} and pos {}: {}", block_height, transaction_index, error.to_string());
						AccessError::UnknownChain
					})
					.unwrap();

			Ok(transaction)
		}) })
	}
}

impl chain::Access for ChainVerifier {
	fn get_utxo(&self, _genesis_hash: &BlockHash, short_channel_id: u64) -> Result<TxOut, AccessError> {
		let block_height = (short_channel_id >> 5 * 8) as u32; // block height is most significant three bytes
		let transaction_index = ((short_channel_id >> 2 * 8) & 0xffffff) as u32;
		let output_index = (short_channel_id & 0xffff) as u16;

		let transaction = self.retrieve_tx(block_height, transaction_index)?;

		let output = transaction.output.get(output_index as usize).ok_or_else(|| {
			eprintln!("Output index {} out of bounds in transaction {}", output_index, transaction.txid().to_string());
			AccessError::UnknownTx
		})?;
		Ok(output.clone())
	}
}

impl TryInto<RestBinaryResponse> for BinaryResponse {
	type Error = std::io::Error;

	fn try_into(self) -> Result<RestBinaryResponse, Self::Error> {
		Ok(RestBinaryResponse(self.0))
	}
}
