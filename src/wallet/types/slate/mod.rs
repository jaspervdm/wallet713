pub mod slate;
pub mod versions;

pub use self::slate::{CompatKernelFeatures, Slate};
pub use self::versions::{
	SlateVersion, VersionedSlate, CURRENT_SLATE_VERSION, GRIN_BLOCK_HEADER_VERSION,
};
