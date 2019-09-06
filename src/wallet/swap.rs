use crate::common::Keychain;
use grin_core::ser;
use grin_keychain::Identifier;
use grinswap::{
	Action, Context, Currency, ErrorKind, Message, OfferUpdate, Role, SecondaryUpdate, Status,
	Swap, SwapApi, Update,
};
use std::collections::HashMap;
use uuid::Uuid;

pub struct SwapApis<K: Keychain> {
	apis: HashMap<Currency, Box<dyn SwapApi<K>>>,
	keychain: Option<K>,
}

impl<K: Keychain> SwapApis<K> {
	pub fn new() -> Self {
		Self {
			apis: HashMap::new(),
			keychain: None,
		}
	}

	pub fn attach(&mut self, currency: Currency, api: Box<dyn SwapApi<K>>) {
		self.apis.insert(currency, api);
	}

	/// Convenience function that opens and closes the wallet with the stored credentials
	fn open_and_close<F, X>(&mut self, currency: &Currency, f: F) -> Result<X, ErrorKind>
	where
		F: FnOnce(&mut dyn SwapApi<K>) -> Result<X, ErrorKind>,
	{
		let keychain = self
			.keychain
			.as_ref()
			.ok_or(ErrorKind::MissingKeychain)?
			.clone();
		let api = self
			.apis
			.get_mut(currency)
			.ok_or(ErrorKind::Generic("API not found".into()))?;

		api.set_keychain(Some(keychain));
		let res = f(api.as_mut());
		api.set_keychain(None);

		res
	}
}

impl<K: Keychain> SwapApi<K> for SwapApis<K> {
	fn set_keychain(&mut self, keychain: Option<K>) {
		self.keychain = keychain;
	}

	fn context_key_count(
		&mut self,
		secondary_currency: Currency,
		is_seller: bool,
	) -> Result<usize, ErrorKind> {
		self.open_and_close(&secondary_currency, move |api| {
			api.context_key_count(secondary_currency, is_seller)
		})
	}

	fn create_context(
		&mut self,
		secondary_currency: Currency,
		is_seller: bool,
		inputs: Option<Vec<(Identifier, u64)>>,
		keys: Vec<Identifier>,
	) -> Result<Context, ErrorKind> {
		self.open_and_close(&secondary_currency, move |api| {
			api.create_context(secondary_currency, is_seller, inputs, keys)
		})
	}

	fn create_swap_offer(
		&mut self,
		context: &Context,
		address: Option<String>,
		primary_amount: u64,
		secondary_amount: u64,
		secondary_currency: Currency,
		secondary_redeem_address: String,
	) -> Result<(Swap, Action), ErrorKind> {
		self.open_and_close(&secondary_currency, move |api| {
			api.create_swap_offer(
				context,
				address,
				primary_amount,
				secondary_amount,
				secondary_currency,
				secondary_redeem_address,
			)
		})
	}

	fn accept_swap_offer(
		&mut self,
		context: &Context,
		address: Option<String>,
		message: Message,
	) -> Result<(Swap, Action), ErrorKind> {
		let (id, offer, sec) = message.unwrap_offer()?;
		let currency = offer.secondary_currency;
		let message = Message::new(id, Update::Offer(offer), sec);

		self.open_and_close(&currency, move |api| {
			api.accept_swap_offer(context, address, message)
		})
	}

	fn completed(&mut self, swap: &mut Swap, context: &Context) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.completed(swap, context))
	}

	fn refunded(&mut self, swap: &mut Swap) -> Result<(), ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.refunded(swap))
	}

	fn cancelled(&mut self, swap: &mut Swap) -> Result<(), ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.cancelled(swap))
	}

	fn required_action(&mut self, swap: &mut Swap, context: &Context) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.required_action(swap, context))
	}

	fn message(&mut self, swap: &Swap) -> Result<Message, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.message(swap))
	}

	fn message_sent(&mut self, swap: &mut Swap, context: &Context) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.message_sent(swap, context))
	}

	fn receive_message(
		&mut self,
		swap: &mut Swap,
		context: &Context,
		message: Message,
	) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.receive_message(swap, context, message))
	}

	fn publish_transaction(
		&mut self,
		swap: &mut Swap,
		context: &Context,
	) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| api.publish_transaction(swap, context))
	}

	fn publish_secondary_transaction(
		&mut self,
		swap: &mut Swap,
		context: &Context,
	) -> Result<Action, ErrorKind> {
		let currency = swap.secondary_currency;
		self.open_and_close(&currency, |api| {
			api.publish_secondary_transaction(swap, context)
		})
	}
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct SwapSummary {
	id: Uuid,
	idx: u32,
	role: Role,
	finalized: bool,
	status: Status,
	primary_amount: u64,
	secondary_amount: u64,
	secondary_currency: Currency,
}

impl From<&Swap> for SwapSummary {
	fn from(swap: &Swap) -> SwapSummary {
		SwapSummary {
			id: swap.id,
			idx: swap.idx,
			role: swap.role.clone(),
			finalized: swap.is_finalized(),
			status: swap.status,
			primary_amount: swap.primary_amount,
			secondary_amount: swap.secondary_amount,
			secondary_currency: swap.secondary_currency,
		}
	}
}

impl ser::Writeable for SwapSummary {
	fn write<W: ser::Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_bytes(&serde_json::to_vec(self).map_err(|_| ser::Error::CorruptedData)?)
	}
}

impl ser::Readable for SwapSummary {
	fn read(reader: &mut dyn ser::Reader) -> Result<SwapSummary, ser::Error> {
		let data = reader.read_bytes_len_prefix()?;
		serde_json::from_slice(&data[..]).map_err(|_| ser::Error::CorruptedData)
	}
}

#[derive(Serialize, Deserialize)]
pub struct SwapOffer {
	pub id: Uuid,
	pub idx: u32,
	pub address: Option<String>,
	pub offer: OfferUpdate,
	pub secondary: SecondaryUpdate,
}

impl From<SwapOffer> for Message {
	fn from(offer: SwapOffer) -> Message {
		Message::new(offer.id, Update::Offer(offer.offer), offer.secondary)
	}
}

impl ser::Writeable for SwapOffer {
	fn write<W: ser::Writer>(&self, writer: &mut W) -> Result<(), ser::Error> {
		writer.write_bytes(&serde_json::to_vec(self).map_err(|_| ser::Error::CorruptedData)?)
	}
}

impl ser::Readable for SwapOffer {
	fn read(reader: &mut dyn ser::Reader) -> Result<SwapOffer, ser::Error> {
		let data = reader.read_bytes_len_prefix()?;
		serde_json::from_slice(&data[..]).map_err(|_| ser::Error::CorruptedData)
	}
}

#[derive(Copy, Clone, Debug)]
pub enum SwapIdentifier {
	Id(Uuid),
	Idx(u32),
}
