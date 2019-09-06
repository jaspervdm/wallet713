use super::{
	AcctPathMapping, Context, Identifier, Keychain, OutputData, Result, Transaction, TxLogEntry,
};
use crate::wallet::swap::SwapOffer;
use crate::wallet::types::TxProof;
use grinswap::{Context as SwapContext, Swap};
use uuid::Uuid;

pub trait WalletBackendBatch<K>
where
	K: Keychain,
{
	fn keychain(&mut self) -> &mut K;
	fn save_output(&mut self, out: &OutputData) -> Result<()>;
	fn delete_output(&mut self, id: &Identifier, mmr_index: &Option<u64>) -> Result<()>;
	fn lock_output(&mut self, out: &mut OutputData) -> Result<()>;
	fn save_child_index(&mut self, parent_key_id: &Identifier, index: u32) -> Result<()>;
	fn save_last_confirmed_height(&mut self, height: u64) -> Result<()>;
	fn next_tx_log_id(&mut self, parent_key_id: &Identifier) -> Result<u32>;
	fn save_tx_log_entry(&mut self, t: &TxLogEntry) -> Result<()>;
	fn save_acct_path(&mut self, mapping: &AcctPathMapping) -> Result<()>;
	fn save_private_context(
		&mut self,
		slate_id: &[u8],
		participant_id: usize,
		ctx: &Context,
	) -> Result<()>;
	fn delete_private_context(&mut self, slate_id: &[u8], participant_id: usize) -> Result<()>;
	fn store_tx(&self, uuid: &str, tx: &Transaction) -> Result<()>;
	fn store_tx_proof(&self, uuid: &str, tx_proof: &TxProof) -> Result<()>;
	fn commit(&mut self) -> Result<()>;
	fn next_swap_idx(&mut self) -> Result<u32>;
	fn store_swap_mapping(&mut self, idx: u32, id: Uuid) -> Result<()>;
	fn store_swap_offer(&mut self, offer: &SwapOffer) -> Result<()>;
	fn delete_swap_offer(&mut self, id: Uuid) -> Result<()>;
	fn store_swap(&mut self, swap: &Swap) -> Result<()>;
	fn store_swap_context(&mut self, id: Uuid, context: &SwapContext) -> Result<()>;
	fn delete_swap_context(&mut self, id: Uuid) -> Result<()>;
}
