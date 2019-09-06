use std::cell::RefCell;
use std::fs::create_dir_all;
use std::path::Path;

use grin_core::ser::Error as CoreError;
use grin_core::ser::{Readable, Reader, Writeable, Writer};
use grin_store::Store;
use grin_store::{self, to_key};

use super::types::{parse_address, AddressBookBackend, AddressBookBatch, Contact};
use crate::common::Error;

const DB_DIR: &'static str = "contacts";
const CONTACT_PREFIX: u8 = 'X' as u8;

pub struct Backend {
	db: Store,
}

impl Backend {
	pub fn new(data_path: &str) -> Result<Self, Error> {
		let db_path = Path::new(data_path).join(DB_DIR);
		create_dir_all(&db_path)?;

		let store = Store::new(db_path.to_str().unwrap(), None, Some(DB_DIR), None)?;

		let res = Backend { db: store };
		Ok(res)
	}
}

impl AddressBookBackend for Backend {
	fn get_contact(&self, name: &[u8]) -> Result<Option<Contact>, Error> {
		let contact_key = to_key(CONTACT_PREFIX, &mut name.to_vec());
		let contact = self.db.get_ser(&contact_key)?;
		Ok(contact)
	}

	fn contacts(&self) -> Box<dyn Iterator<Item = Contact>> {
		Box::new(self.db.iter(&[CONTACT_PREFIX]).unwrap().map(|x| x.1))
	}

	fn batch<'a>(&'a self) -> Result<Box<dyn AddressBookBatch + 'a>, Error> {
		let batch = self.db.batch()?;
		let batch = Batch {
			_store: self,
			db: RefCell::new(Some(batch)),
		};
		Ok(Box::new(batch))
	}
}

pub struct Batch<'a> {
	_store: &'a Backend,
	db: RefCell<Option<grin_store::Batch<'a>>>,
}

impl<'a> AddressBookBatch for Batch<'a> {
	fn save_contact(&mut self, contact: &Contact) -> Result<(), Error> {
		let mut key = contact.name.to_string().into_bytes();
		let contact_key = to_key(CONTACT_PREFIX, &mut key);
		self.db
			.borrow()
			.as_ref()
			.unwrap()
			.put_ser(&contact_key, contact)?;
		Ok(())
	}

	fn delete_contact(&mut self, name: &[u8]) -> Result<(), Error> {
		let ctx_key = to_key(CONTACT_PREFIX, &mut name.to_vec());
		self.db
			.borrow()
			.as_ref()
			.unwrap()
			.delete(&ctx_key)
			.map_err(|e| e.into())
	}

	fn commit(&mut self) -> Result<(), Error> {
		let db = self.db.replace(None);
		db.unwrap().commit()?;
		Ok(())
	}
}

impl Writeable for Contact {
	fn write<W: Writer>(&self, writer: &mut W) -> Result<(), CoreError> {
		let json = json!({
			"name": self.name,
			"address": self.address.to_string(),
		});
		writer.write_bytes(&json.to_string().as_bytes())
	}
}

impl Readable for Contact {
	fn read(reader: &mut dyn Reader) -> Result<Contact, CoreError> {
		let data = reader.read_bytes_len_prefix()?;
		let data = std::str::from_utf8(&data).map_err(|_| CoreError::CorruptedData)?;

		let json: serde_json::Value =
			serde_json::from_str(&data).map_err(|_| CoreError::CorruptedData)?;

		let address = parse_address(json["address"].as_str().unwrap())
			.map_err(|_| CoreError::CorruptedData)?;

		let contact = Contact::new(json["name"].as_str().unwrap(), address)
			.map_err(|_| CoreError::CorruptedData)?;

		Ok(contact)
	}
}
