use super::Adapter;
use crate::api::listener::ListenerInterface;
use crate::common::{Arc, Keychain, Mutex};
use crate::wallet::types::{VersionedSlate, WalletBackend};
use crate::wallet::Container;
use failure::Error;
use libwallet::NodeClient;

/// Grinbox 'plugin' implementation

#[derive(Clone)]
pub struct GrinboxAdapter<'a, W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	container: &'a Arc<Mutex<Container<W, C, K>>>,
}

impl<'a, W, C, K> GrinboxAdapter<'a, W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	/// Create
	pub fn new(container: &'a Arc<Mutex<Container<W, C, K>>>) -> Box<Self> {
		Box::new(Self { container })
	}
}

impl<'a, W, C, K> Adapter for GrinboxAdapter<'a, W, C, K>
where
	W: WalletBackend<C, K>,
	C: NodeClient,
	K: Keychain,
{
	fn supports_sync(&self) -> bool {
		false
	}

	fn send_tx_sync(&self, _dest: &str, _slate: &VersionedSlate) -> Result<VersionedSlate, Error> {
		unimplemented!();
	}

	fn send_tx_async(&self, dest: &str, slate: &VersionedSlate) -> Result<(), Error> {
		let c = self.container.lock();
		c.listener(ListenerInterface::Grinbox)?
			.publish_slate(slate, &dest.to_owned())
	}
}
