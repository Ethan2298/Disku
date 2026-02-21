#[cfg(target_os = "macos")]
pub mod mac_scanner;
#[cfg(windows)]
pub mod mft_scanner;
pub mod scanner;
pub mod tree;
pub mod utils;
