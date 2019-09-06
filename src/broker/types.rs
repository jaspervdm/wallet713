use crate::cli_message;
use crate::common::{Arc, Error, Keychain, Mutex};
use crate::contacts::{Address, AddressType, GrinboxAddress};
use crate::wallet::api::{Foreign, Owner};
use crate::wallet::types::{Slate, TxProof, VersionedSlate, WalletBackend};
use crate::wallet::Container;
use colored::Colorize;
use grinswap::Message as SwapMessage;
use libwallet::NodeClient;
use serde::Serialize;
use std::marker::Send;

pub enum CloseReason {
	Normal,
	Abnormal(Error),
}

pub trait Publisher: Send {
	fn post<T: Serialize>(&self, payload: &T, to: &dyn Address) -> Result<(), Error>;
}

pub trait Subscriber {
	fn start<W, C, K, P>(&mut self, handler: Controller<W, C, K, P>) -> Result<(), Error>
	where
		W: WalletBackend<C, K>,
		C: NodeClient,
		K: Keychain,
		P: Publisher;
	fn stop(&self);
	fn is_running(&self) -> bool;
}

pub trait SubscriptionHandler: Send {
	fn on_open(&self);
	fn on_slate(&self, from: &dyn Address, slate: &VersionedSlate, proof: Option<&mut TxProof>);
	fn on_swap_message(&self, from: &dyn Address, message: SwapMessage);
	fn on_close(&self, result: CloseReason);
	fn on_dropped(&self);
	fn on_reestablished(&self);
}

pub struct Controller<W, C, K, P>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
	P: Publisher,
{
	name: String,
	owner: Owner<W, C, K>,
	foreign: Foreign<W, C, K>,
	publisher: P,
}

impl<W, C, K, P> Controller<W, C, K, P>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
	P: Publisher,
{
	pub fn new(
		name: &str,
		container: Arc<Mutex<Container<W, C, K>>>,
		publisher: P,
	) -> Result<Self, Error> {
		Ok(Self {
			name: name.to_string(),
			owner: Owner::new(container.clone()),
			foreign: Foreign::new(container),
			publisher,
		})
	}

	fn process_incoming_slate(
		&self,
		address: Option<String>,
		slate: &mut Slate,
		tx_proof: Option<&mut TxProof>,
	) -> Result<bool, Error> {
		if slate.num_participants > slate.participant_data.len() {
			if slate.tx.inputs().len() == 0 {
				// TODO: invoicing
			} else {
				*slate = self.foreign.receive_tx(slate, None, address, None)?;
			}
			Ok(false)
		} else {
			self.owner.finalize_tx(slate, tx_proof)?;
			Ok(true)
		}
	}
}

impl<W, C, K, P> SubscriptionHandler for Controller<W, C, K, P>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
	P: Publisher,
{
	fn on_open(&self) {
		//        cli_message!("Listener for {} started", self.name.bright_green());
	}

	fn on_slate(&self, from: &dyn Address, slate: &VersionedSlate, tx_proof: Option<&mut TxProof>) {
		let version = slate.version();
		let mut slate: Slate = slate.clone().into();

		/*if slate.num_participants > slate.participant_data.len() {
			cli_message!(
				"Slate [{}] received from [{}] for [{}] grins",
				slate.id.to_string().bright_green(),
				display_from.bright_green(),
				amount_to_hr_string(slate.amount, false).bright_green()
			);
		} else {
			cli_message!(
				"Slate [{}] received back from [{}] for [{}] grins",
				slate.id.to_string().bright_green(),
				display_from.bright_green(),
				amount_to_hr_string(slate.amount, false).bright_green()
			);
		};*/

		if from.address_type() == AddressType::Grinbox {
			GrinboxAddress::from_str(&from.to_string()).expect("invalid grinbox address");
		}

		let result = self
			.process_incoming_slate(Some(from.to_string()), &mut slate, tx_proof)
			.and_then(|is_finalized| {
				if !is_finalized {
					let id = slate.id.clone();
					let slate = VersionedSlate::into_version(slate, version);

					let _ = self
						.publisher
						.post(&slate, from)
						.map_err(|e| {
							cli_message!("{}: {}", "ERROR".bright_red(), e);
							e
						})
						.map(|_| {
							cli_message!(
								"Slate {} sent back to {} successfully",
								id.to_string().bright_green(),
								from.stripped().bright_green()
							);
						});
				}
				/*else {
					cli_message!(
						"Slate [{}] finalized successfully",
						slate.id.to_string().bright_green()
					);
				}*/
				Ok(())
			});

		match result {
			Ok(()) => {}
			Err(e) => cli_message!("{}", e),
		}
	}

	fn on_swap_message(&self, from: &dyn Address, message: SwapMessage) {
		let _ = self
			.foreign
			.receive_swap_message(Some(format!("{}", from)), message)
			.map_err(|e| {
				println!("{}: {}", "ERROR".bright_red(), e);
			});
	}

	fn on_close(&self, reason: CloseReason) {
		match reason {
			CloseReason::Normal => {
				//println!("Listener for {} stopped", self.name.bright_green())
			}
			CloseReason::Abnormal(_) => {
				cli_message!("Listener {} stopped unexpectedly", self.name.bright_green())
			}
		}
	}

	fn on_dropped(&self) {
		cli_message!("Listener {} lost connection. it will keep trying to restore connection in the background.", self.name.bright_green())
	}

	fn on_reestablished(&self) {
		cli_message!(
			"Listener {} reestablished connection.",
			self.name.bright_green()
		)
	}
}

#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum EncryptedPayload {
	Tx(VersionedSlate),
	Swap(SwapMessage),
}
