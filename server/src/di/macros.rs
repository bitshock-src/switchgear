// ===== DELEGATION MACROS =====

macro_rules! delegate_to_ln_balancer_variants {
    ($self:expr, $method:ident $(, $arg:expr)*) => {
        match $self {
            Self::RoundRobin(inner) => inner.$method($($arg),*),
            Self::Random(inner) => inner.$method($($arg),*),
            Self::Consistent(inner) => inner.$method($($arg),*),
        }
    };
}

macro_rules! delegate_to_offer_store_variants {
    ($self:expr, $method:ident $(, $arg:expr)*) => {
        match $self {
            Self::Database(inner) => inner.$method($($arg),*),
            Self::Memory(inner) => inner.$method($($arg),*),
            Self::Http(inner) => inner.$method($($arg),*),
        }
    };
}

macro_rules! delegate_to_discovery_store_variants {
    ($self:expr, $method:ident $(, $arg:expr)*) => {
        match $self {
            Self::Database(inner) => inner.$method($($arg),*),
            Self::Memory(inner) => inner.$method($($arg),*),
            Self::Http(inner) => inner.$method($($arg),*),
            Self::File(inner) => inner.$method($($arg),*),
        }
    };
}

// Export macros for use in parent module
pub(crate) use delegate_to_discovery_store_variants;
pub(crate) use delegate_to_ln_balancer_variants;
pub(crate) use delegate_to_offer_store_variants;
