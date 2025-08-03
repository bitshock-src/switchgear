use pingora_core::protocols::l4::socket::SocketAddr;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::net::{Ipv6Addr, SocketAddrV6};
use switchgear_service::api::discovery::DiscoveryBackendAddress;

pub trait IntoPingoraSocketAddr {
    fn into_pingora_socket_addr(self) -> SocketAddr;
    fn as_pingora_socket_addr(&self) -> SocketAddr;
}

impl IntoPingoraSocketAddr for DiscoveryBackendAddress {
    fn into_pingora_socket_addr(self) -> SocketAddr {
        let mut hasher = DefaultHasher::new();
        match self {
            DiscoveryBackendAddress::PublicKey(h) => h.hash(&mut hasher),
            DiscoveryBackendAddress::Url(h) => h.hash(&mut hasher),
        }
        // 👍
        let addr = Ipv6Addr::from_bits(hasher.finish() as u128);
        SocketAddr::Inet(SocketAddrV6::new(addr, 1, 0, 0).into())
    }

    fn as_pingora_socket_addr(&self) -> SocketAddr {
        self.clone().into_pingora_socket_addr()
    }
}
