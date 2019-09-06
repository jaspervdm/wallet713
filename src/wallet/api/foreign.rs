// Copyright 2018 The Grin & vault713 Developers
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{check_middleware, VersionInfo};
use crate::common::{Arc, Keychain, Mutex, MutexGuard};
use crate::internal::{swap, tx, updater};
use crate::wallet::swap::SwapOffer;
use crate::wallet::types::{BlockFees, CbData, Slate, SlateVersion, WalletBackend};
use crate::wallet::{Container, ErrorKind};
use colored::Colorize;
use failure::Error;
use grin_core::core::amount_to_hr_string;
use grinswap::{Message as SwapMessage, SwapApi, Update as SwapUpdate};
use libwallet::{NodeClient, NodeVersionInfo};
use std::marker::PhantomData;

const FOREIGN_API_VERSION: u16 = 2;

/// ForeignAPI Middleware Check callback
type ForeignCheckMiddleware =
	fn(ForeignCheckMiddlewareFn, Option<NodeVersionInfo>, Option<&Slate>) -> Result<(), Error>;

pub enum ForeignCheckMiddlewareFn {
	/// check_version
	CheckVersion,
	/// build_coinbase
	BuildCoinbase,
	/// verify_slate_messages
	VerifySlateMessages,
	/// receive_tx
	ReceiveTx,
	/*/// finalize_invoice_tx
	FinalizeInvoiceTx,*/
}

#[derive(StateData)]
pub struct Foreign<W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	container: Arc<Mutex<Container<W, C, K>>>,
	middleware: Option<ForeignCheckMiddleware>,
	phantom_k: PhantomData<K>,
	phantom_c: PhantomData<C>,
}

impl<W, C, K> Foreign<W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	pub fn new(container: Arc<Mutex<Container<W, C, K>>>) -> Self {
		Foreign {
			container,
			middleware: Some(check_middleware),
			phantom_k: PhantomData,
			phantom_c: PhantomData,
		}
	}

	pub fn check_version(&self) -> Result<VersionInfo, Error> {
		let mut c = self.container.lock();
		let w = c.backend()?;

		if let Some(m) = self.middleware.as_ref() {
			m(
				ForeignCheckMiddlewareFn::CheckVersion,
				w.w2n_client().get_version_info(),
				None,
			)?;
		}

		Ok(VersionInfo {
			foreign_api_version: FOREIGN_API_VERSION,
			supported_slate_versions: vec![SlateVersion::V2],
		})
	}

	pub fn build_coinbase(&self, block_fees: &BlockFees) -> Result<CbData, Error> {
		self.open_and_close(|c| {
			let w = c.backend()?;
			if let Some(m) = self.middleware.as_ref() {
				m(
					ForeignCheckMiddlewareFn::BuildCoinbase,
					w.w2n_client().get_version_info(),
					None,
				)?;
			}
			updater::build_coinbase(w, block_fees)
		})
	}

	pub fn verify_slate_messages(&self, slate: &Slate) -> Result<(), Error> {
		let mut c = self.container.lock();
		let w = c.backend()?;

		if let Some(m) = self.middleware.as_ref() {
			m(
				ForeignCheckMiddlewareFn::VerifySlateMessages,
				w.w2n_client().get_version_info(),
				Some(slate),
			)?;
		}

		slate.verify_messages()
	}

	pub fn receive_tx(
		&self,
		slate: &Slate,
		dest_acct_name: Option<&str>,
		address: Option<String>,
		message: Option<String>,
	) -> Result<Slate, Error> {
		self.open_and_close(|c| {
			let w = c.backend()?;

			if let Some(m) = self.middleware.as_ref() {
				m(
					ForeignCheckMiddlewareFn::ReceiveTx,
					w.w2n_client().get_version_info(),
					Some(slate),
				)?;
			}

			let slate = tx::receive_tx(w, slate, dest_acct_name, address.clone(), message)?;

			let from = match address {
				Some(a) => format!(" from {}", a.bright_green()),
				None => String::new(),
			};

			cli_message!(
				"Slate {} for {} grin received{}",
				slate.id.to_string().bright_green(),
				amount_to_hr_string(slate.amount, false).bright_green(),
				from
			);

			Ok(slate)
		})
	}

	/*pub fn finalize_invoice_tx(&self, slate: &Slate) -> Result<Slate, Error> {
		let mut w = self.wallet.lock();
		if let Some(m) = self.middleware.as_ref() {
			m(
				ForeignCheckMiddlewareFn::FinalizeInvoiceTx,
				w.w2n_client().get_version_info(),
				Some(slate),
			)?;
		}
		w.open_with_credentials()?;
		let res = foreign::finalize_invoice_tx(&mut *w, slate);
		w.close()?;
		res
	}*/

	pub fn receive_swap_message(
		&self,
		address: Option<String>,
		message: SwapMessage,
	) -> Result<(), Error> {
		let from = match &address {
			Some(a) => format!(" from {}", a.bright_green()),
			None => String::new(),
		};

		self.swap_open_and_close(|c| {
			match &message.inner {
				SwapUpdate::Offer(_) => {
					let w = c.backend()?;
					if w.get_swap(message.id)?.is_some() || w.get_swap_offer(message.id)?.is_some()
					{
						return Err(ErrorKind::GenericError("Swap already exists".into()).into());
					}

					let (id, offer, secondary) = message.unwrap_offer()?;
					let mut batch = w.batch()?;
					let idx = batch.next_swap_idx()?;
					let offer = SwapOffer {
						id,
						idx,
						address,
						offer,
						secondary,
					};
					batch.store_swap_mapping(idx, id)?;
					batch.store_swap_offer(&offer)?;
					batch.commit()?;

					cli_message!(
						"Swap offer {} received{}",
						id.to_string().bright_green(),
						from
					);
				}
				_ => {
					let id = message.id;
					let w = c.backend()?;
					let context = w.get_swap_context(id)?;
					let mut swap = w.get_swap(id)?.ok_or(ErrorKind::NotFound)?;

					swap::update_swap(c, &mut swap, &context)?;
					c.swap_apis.receive_message(&mut swap, &context, message)?;
					swap::update_swap(c, &mut swap, &context)?;

					let mut batch = c.backend()?.batch()?;
					batch.store_swap(&swap)?;
					batch.commit()?;

					cli_message!(
						"Swap {} message received{}",
						id.to_string().bright_green(),
						from
					);
				}
			}

			Ok(())
		})
	}

	/// Convenience function that opens and closes the wallet with the stored credentials
	fn open_and_close<F, X>(&self, f: F) -> Result<X, Error>
	where
		F: FnOnce(&mut MutexGuard<Container<W, C, K>>) -> Result<X, Error>,
	{
		let mut c = self.container.lock();
		let w = c.backend()?;
		if !w.has_seed()? {
			return Err(ErrorKind::NoSeed.into());
		}
		w.open_with_credentials()?;

		// Execute operation
		let res = f(&mut c);

		// Always try to close wallet
		// Operation still considered successful, even if closing failed
		let w = c.backend();
		if w.is_ok() {
			let _ = w.unwrap().close();
		}

		res
	}

	/// Convenience function that opens and closes the wallet with the stored credentials
	fn swap_open_and_close<F, X>(&self, f: F) -> Result<X, Error>
	where
		F: FnOnce(&mut MutexGuard<Container<W, C, K>>) -> Result<X, Error>,
	{
		let mut c = self.container.lock();
		let w = c.backend()?;
		if !w.has_seed()? {
			return Err(ErrorKind::NoSeed.into());
		}
		w.open_with_credentials()?;
		let keychain = w.keychain().clone();
		c.swap_apis.set_keychain(Some(keychain));

		// Execute operation
		let res = f(&mut c);

		// Always try to close wallet
		// Operation still considered successful, even if closing failed
		let w = c.backend();
		if w.is_ok() {
			let _ = w.unwrap().close();
		}
		c.swap_apis.set_keychain(None);

		res
	}
}

impl<W, C, K> Clone for Foreign<W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	fn clone(&self) -> Self {
		Self {
			container: self.container.clone(),
			middleware: self.middleware.clone(),
			phantom_k: PhantomData,
			phantom_c: PhantomData,
		}
	}
}
