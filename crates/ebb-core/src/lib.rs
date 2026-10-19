//! ebb-core: tarama, katalog, plan/uygula/geri al. CLI, MCP ve GUI bu çekirdeği paylaşır.
pub mod agents;
pub mod catalog;
pub mod connect;
pub mod dupes;
pub mod error;
pub mod guard;
pub mod journal;
pub mod ops;
pub mod optimize;
pub mod plan;
pub mod platform;
pub mod scan;
pub mod snapshot;

pub use error::{Error, Result};

/// Veri klasörü: `EBB_DATA_DIR` ya da işletim sisteminin uygulama veri dizini.
pub fn data_dir() -> std::path::PathBuf {
    std::env::var_os("EBB_DATA_DIR")
        .map(Into::into)
        .unwrap_or_else(|| {
            dirs::data_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join("ebb")
        })
}

pub fn db_path() -> std::path::PathBuf {
    data_dir().join("ebb.db")
}
