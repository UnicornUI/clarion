//! Toolkit-independent desktop configuration: KDL editing, validation, the
//! common-settings model and the lossless write path used by the native
//! GPUI settings applications.

pub mod appearance;
pub mod common_settings;
pub mod config_manager;
pub mod ipc;
pub mod kdl_edit;
pub mod output_preview;
pub mod output_settings;
pub mod socket_io;
pub mod storage;

pub use ipc::{config_reload_action, ConfigIpc, Connection};
