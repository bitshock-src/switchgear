use switchgear_service::components::pool::default_pool::DefaultLnClientPool;

pub mod delegates;
pub mod inject;
pub mod macros;

// ===== TYPE ALIASES =====

type Pool = DefaultLnClientPool<pingora_load_balancing::Backend>;
