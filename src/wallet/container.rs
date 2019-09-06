use super::ErrorKind;
use crate::api::listener::{Listener, ListenerInterface};
use crate::common::config::Wallet713Config;
use crate::common::{Arc, Keychain, Mutex};
use crate::contacts::AddressBook;
use crate::wallet::backend::Backend;
use crate::wallet::swap::SwapApis;
use crate::wallet::types::{HTTPNodeClient, WalletBackend};
use failure::Error;
use grin_core::global::is_floonet;
use grin_keychain::ExtKeychain;
use grinswap::{BtcSwapApi, Currency, ElectrumNodeClient};
use libwallet::NodeClient;
use std::collections::HashMap;
use std::marker::PhantomData;

pub struct Container<W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	pub config: Wallet713Config,
	backend: W,
	pub address_book: AddressBook,
	pub account: String,
	pub swap_apis: SwapApis<K>,
	pub listeners: HashMap<ListenerInterface, Box<dyn Listener>>,
	phantom_c: PhantomData<C>,
	phantom_k: PhantomData<K>,
}

impl<W, C, K> Container<W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	pub fn new(
		config: Wallet713Config,
		backend: W,
		address_book: AddressBook,
	) -> Result<Arc<Mutex<Self>>, Error> {
		let mut container = Self {
			config,
			backend,
			address_book,
			account: String::from("default"),
			swap_apis: SwapApis::new(),
			listeners: HashMap::with_capacity(4),
			phantom_c: PhantomData,
			phantom_k: PhantomData,
		};
		container.attach_swap_apis()?;

		Ok(Arc::new(Mutex::new(container)))
	}

	pub fn raw_backend(&mut self) -> &mut W {
		&mut self.backend
	}

	pub fn backend(&mut self) -> Result<&mut W, Error> {
		if !self.backend.connected()? {
			return Err(ErrorKind::NoBackend.into());
		}
		Ok(&mut self.backend)
	}

	fn attach_swap_apis(&mut self) -> Result<(), Error> {
		let swap_config = match &self.config.swap {
			Some(c) => c,
			None => return Ok(()),
		};

		for (currency, currency_config) in &swap_config.currencies {
			match currency.as_ref() {
				"btc" => {
					let address = currency_config
						.get("electrum_node_uri")
						.ok_or(ErrorKind::GenericError(
							"Missing BTC Electrum node uri".into(),
						))?
						.as_str()
						.ok_or(ErrorKind::GenericError(
							"Invalid BTC Electrum node uri".into(),
						))?
						.to_owned();

					let btc_node_client = ElectrumNodeClient::new(address, is_floonet());
					let api = BtcSwapApi::<K, _, _>::new(
						None,
						self.backend.w2n_client().clone(),
						btc_node_client,
					);
					self.swap_apis.attach(Currency::Btc, Box::new(api));
				}
				_ => continue,
			}
		}

		Ok(())
	}

	pub fn listener(&self, interface: ListenerInterface) -> Result<&Box<dyn Listener>, ErrorKind> {
		self.listeners
			.get(&interface)
			.ok_or(ErrorKind::NoListener(format!("{}", interface)))
	}
}

pub fn create_container(
	config: Wallet713Config,
	address_book: AddressBook,
) -> Result<
	Arc<Mutex<Container<Backend<HTTPNodeClient, ExtKeychain>, HTTPNodeClient, ExtKeychain>>>,
	Error,
> {
	let wallet_config = config.as_wallet_config()?;
	let client = HTTPNodeClient::new(
		&wallet_config.check_node_api_http_addr,
		config.grin_node_secret().clone(),
	);
	let backend = Backend::new(&wallet_config, client)?;
	let container = Container::new(config, backend, address_book)?;
	Ok(container)
}
