//! User interface implementation

use cfg_if::cfg_if;


cfg_if! {
 if #[cfg(feature = "windowed")] {
    mod windowed;
    pub use windowed::start;
 } else {
    mod ipc;
    pub use ipc::start;
 }
}
