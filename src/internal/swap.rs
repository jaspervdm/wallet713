use super::*;
use crate::api::listener::ListenerInterface;
use crate::common::{Keychain, MutexGuard};
use crate::contacts::{parse_address, AddressType};
use crate::wallet::swap::SwapIdentifier;
use crate::wallet::types::{OutputData, OutputStatus, WalletBackend};
use crate::wallet::{Container, ErrorKind};
use colored::Colorize;
use failure::Error;
use grin_core::core::amount_to_hr_string;
use grin_util::to_hex;
use grinswap::{Action, Context, Swap, SwapApi};
use libwallet::NodeClient;

pub fn select_coins<W, C, K>(wallet: &mut W, needed: u64) -> Result<Vec<OutputData>, Error>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	let parent_key_id = wallet.get_parent_key_id();
	let height = wallet.w2n_client().get_chain_height()?;

	let (_, coins) =
		selection::select_coins(wallet, needed, height, 10, 500, false, &parent_key_id);
	let total = coins.iter().map(|c| c.value).sum();
	if total < needed {
		return Err(ErrorKind::NotEnoughFunds {
			available: total,
			available_disp: amount_to_hr_string(total, false),
			needed,
			needed_disp: amount_to_hr_string(needed, false),
		}
		.into());
	}

	Ok(coins)
}

pub fn update_swap<W, C, K>(
	c: &mut MutexGuard<Container<W, C, K>>,
	swap: &mut Swap,
	context: &Context,
) -> Result<Action, Error>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	let address = match &swap.address {
		Some(a) => Some(parse_address(a)?),
		None => None,
	};

	let mut action = c.swap_apis.required_action(swap, context)?;
	loop {
		match &action {
			Action::SendMessage(_) => {
				let message = match c.swap_apis.message(&swap) {
					Ok(m) => m,
					Err(_) => break,
				};

				match &address {
					Some(a) => {
						// Attempt to send
						match a.address_type() {
							AddressType::Grinbox => {
								let listener = match c.listeners.get(&ListenerInterface::Grinbox) {
									Some(l) => l,
									None => break,
								};

								if let Err(e) =
									listener.publish_swap_message(&message, &a.to_string())
								{
									// Sending failed
									println!(
										"{} Unable to publish swap message: {}",
										"ERROR:".bright_red(),
										e
									);
									break;
								}
							}
							_ => {
								// TODO
								unimplemented!();
							}
						}

						// If we got here, sending was successful
						match c.swap_apis.message_sent(swap, context) {
							Ok(a) => action = a,
							Err(e) => {
								println!("{} Unable to update state: {}", "ERROR:".bright_red(), e);
								break;
							}
						}

						println!(
							"Sent swap {} message to {}",
							swap.id.to_string().bright_green(),
							a.stripped().bright_green()
						);
					}
					None => break,
				}
			}
			Action::PublishTx => match c.swap_apis.publish_transaction(swap, context) {
				Ok(a) => action = a,
				Err(e) => {
					println!(
						"{} Unable to publish transaction: {}",
						"ERROR:".bright_red(),
						e
					);
					break;
				}
			},
			Action::PublishTxSecondary => {
				match c.swap_apis.publish_secondary_transaction(swap, context) {
					Ok(a) => action = a,
					Err(e) => {
						println!(
							"{} Unable to publish secondary transaction: {}",
							"ERROR:".bright_red(),
							e
						);
						break;
					}
				}
			}
			Action::Complete => {
				match c.swap_apis.completed(swap, context) {
					Ok(a) => action = a,
					Err(e) => {
						println!("{} Unable to complete: {}", "ERROR:".bright_red(), e);
						break;
					}
				}
				println!("Swap {} completed", swap.id.to_string().bright_green());

				let mut batch = c.backend()?.batch()?;
				if !swap.is_seller() {
					let bcontext = match context.unwrap_buyer() {
						Ok(c) => c,
						Err(e) => {
							println!(
								"{} Unable to find buyer context: {}",
								"ERROR:".bright_red(),
								e
							);
							break;
						}
					};

					let key_id = bcontext.output.clone();
					let n_child = key_id.to_path().last_path_index();
					let (value, commit) = match swap.redeem_output() {
						Ok(Some(x)) => x,
						_ => {
							println!("{} Unable to find redeem output", "ERROR:".bright_red());
							break;
						}
					};

					let output = OutputData {
						root_key_id: key_id.parent_path(),
						key_id,
						n_child,
						commit: Some(to_hex(commit.0.to_vec())),
						mmr_index: None,
						value,
						status: OutputStatus::Unconfirmed,
						height: 0,
						lock_height: 0,
						is_coinbase: false,
						tx_log_entry: None,
					};

					batch.save_output(&output)?;
				}
				batch.delete_swap_context(swap.id)?;
				batch.commit()?;
			}
			Action::Cancel => {
				unimplemented!();
			}
			Action::Refund => {
				unimplemented!();
			}
			Action::None
			| Action::ReceiveMessage
			| Action::DepositSecondary {
				amount: _,
				address: _,
			}
			| Action::Confirmations {
				required: _,
				actual: _,
			}
			| Action::ConfirmationsSecondary {
				required: _,
				actual: _,
			}
			| Action::ConfirmationRedeem
			| Action::ConfirmationRedeemSecondary(_) => break,
		}
	}

	Ok(action)
}
