# LNURL Balancer Testing Environment

This directory contains the Docker-based regtest environment for testing Switchgear with Lightning Network nodes, plus the `switchgear-testing` Rust crate for accessing test credentials and port allocation.

## Architecture

The regtest environment provides:
- **Bitcoin Core** (regtest mode) - Local Bitcoin network for testing
- **Core Lightning (CLN)** - Lightning Network node implementation  
- **LND** - Lightning Network Daemon implementation
- **PostgreSQL** - Database for testing
- **MySQL** - Alternative database for testing
- **Setup Container** - Automated initialization and credential extraction
- **Testing Crate** - Rust library for reading Lightning node credentials and dynamic port allocation

## Quick Start

1. **Start the environment:**
   ```bash
   docker-compose up --build -d
   ```

2. **Run integration tests:**
   ```bash
   cargo test --release
   ```

## Components

### Docker Services

- **bitcoin** (port 18443) - Bitcoin Core in regtest mode
- **cln** (ports 9735, 9736) - Core Lightning node with gRPC
- **lnd** (ports 8080, 9734, 10009) - LND node with REST and gRPC APIs
- **postgres** (port 5432) - PostgreSQL database
- **mysql** (port 3306) - MySQL database
- **setup** - Initialization container that:
  - Generates initial Bitcoin blocks
  - Funds Lightning node wallets
  - Creates bidirectional payment channels
  - Extracts credentials and external addresses to shared volume

### Credential Management

The setup container automatically extracts Lightning node credentials and external addresses to a mounted directory (`./shared-credentials/credentials`) containing:

**CLN Credentials:**
- `cln*/node_id` - Node public key (hex-encoded)
- `cln*/address.txt` - External gRPC address (host:port format)
- `cln*/ca.pem` - CA certificate
- `cln*/client.pem` - Client certificate  
- `cln*/client-key.pem` - Client private key
- `cln*/access.rune` - Access rune for authentication

**LND Credentials:**
- `lnd*/node_id` - Node public key (hex-encoded)
- `lnd*/address.txt` - External gRPC address (host:port format)
- `lnd*/tls.cert` - TLS certificate
- `lnd*/admin.macaroon` - Admin authentication macaroon (binary)

Note: Multiple nodes of each type may be present with numbered suffixes (e.g., cln1, cln2, lnd1, lnd2)

## Testing Crate (`switchgear-testing`)

The testing crate provides a Rust library for accessing Lightning node credentials and dynamic port allocation for integration tests.

### API

The crate exposes functionality for credential management and port allocation:

#### Credentials API

```rust
use switchgear_testing::credentials::{get_backends, RegTestLnNode, RegTestLnNodeAddress};

// Get all backends
let backends = get_backends()?;

// Match on node types
for backend in backends {
    match backend {
        RegTestLnNode::Cln(cln) => {
            println!("CLN Node ID: {:?}", cln.public_key);
            println!("CLN Address: {:?}", cln.address);
            println!("CLN SNI: {}", cln.sni);
            // Access paths: cln.ca_cert_path, cln.client_cert_path, cln.client_key_path
        }
        RegTestLnNode::Lnd(lnd) => {
            println!("LND Node ID: {:?}", lnd.public_key);
            println!("LND Address: {:?}", lnd.address);
            // Access paths: lnd.tls_cert_path, lnd.macaroon_path
        }
    }
}
```

#### Port Allocation API

```rust
use switchgear_testing::ports::PortAllocator;
use std::path::PathBuf;

// Find an available port (with file-based locking to prevent conflicts)
let ports_path = PathBuf::from("/tmp/test_ports");
let port = PortAllocator::find_available_port(&ports_path)?;
```

### Credential Types

The crate uses strongly-typed credential structures:

```rust
pub enum RegTestLnNode {
    Cln(ClnRegTestLnNode),
    Lnd(LndRegTestLnNode),
}

pub struct ClnRegTestLnNode {
    pub public_key: PublicKey,
    pub address: RegTestLnNodeAddress,
    pub ca_cert_path: PathBuf,
    pub client_cert_path: PathBuf,
    pub client_key_path: PathBuf,
    pub sni: String,  // Server Name Indication (default: "localhost")
}

pub struct LndRegTestLnNode {
    pub public_key: PublicKey,
    pub address: RegTestLnNodeAddress,
    pub tls_cert_path: PathBuf,
    pub macaroon_path: PathBuf,
}
```

All credentials are provided as file paths for maximum flexibility and security.

### Data Structures

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RegTestLnNodeAddress {
    Inet(SocketAddr),    // TCP address (standard)
    Path(Vec<u8>),       // Unix socket path (not currently used)
}
```

The `RegTestLnNode` enum provides convenience methods:
- `public_key()` - Get the node's public key
- `address()` - Get the node's address
- `kind()` - Get the node type ("cln" or "lnd")

### Environment Configuration

Set `LNURL_BALANCER_CREDENTIALS_PATH` to specify the credentials directory:

```bash
export LNURL_BALANCER_CREDENTIALS_PATH=/path/to/credentials
cargo test
```

To skip integration tests:

```bash
export LNURL_SKIP_INTEGRATION_TESTS=true
cargo test
```

### Integration Testing

Integration tests use the testing crate to access credentials:

```rust
use switchgear_testing::credentials::get_backends;

// Get all available backends
let backends = get_backends()?;

// Filter by type if needed
let cln_nodes: Vec<_> = backends.iter()
    .filter(|node| node.kind() == "cln")
    .collect();
```

**Test Environment Requirements:**
- Set `LNURL_BALANCER_CREDENTIALS_PATH` environment variable
- Or set `LNURL_SKIP_INTEGRATION_TESTS=true` to skip integration tests

## Configuration

### Environment Variables

- `LNURL_BALANCER_CREDENTIALS_PATH` - Path to credentials directory (required for tests)
  - Example: `/Users/user/dev/lnurl-balancer/testing/shared-credentials/credentials`
  - Used by `get_backends()` to locate credential files
  - Must be set when running integration tests
- `LNURL_SKIP_INTEGRATION_TESTS` - Set to `true` to skip integration tests
  - Useful for CI/CD environments without Docker
  - Returns empty backend list when set

### Ports

- `18443` - Bitcoin Core RPC
- `18444` - Bitcoin P2P
- `8080` - LND REST API  
- `9734` - LND P2P
- `9735` - CLN P2P
- `9736` - CLN gRPC (external address used in tests)
- `10009` - LND gRPC (external address used in tests)
- `28332-28333` - Bitcoin Core ZMQ
- `5432` - PostgreSQL
- `3306` - MySQL

External addresses for Lightning nodes are automatically configured:
- CLN: `127.0.0.1:9736`
- LND: `127.0.0.1:10009`

## Troubleshooting

### Setup Issues

Check the setup container logs:
```bash
docker-compose logs setup
```

Common issues:
- **Wallet creation failures** - Usually resolved by restarting: `docker-compose down && docker-compose up -d`
- **Channel creation timeouts** - Lightning nodes may need more time to sync
- **Block generation errors** - Bitcoin Core may need time to start

### Integration Test Issues

1. **Credentials not found** - Ensure setup completed successfully and check `./shared-credentials/credentials/`
2. **Connection failures** - Verify Docker containers are running: `docker-compose ps`
3. **Permission errors** - Check file permissions in credentials directory
4. **Address parsing errors** - Verify `address.txt` files contain valid `host:port` format
5. **Missing testing crate** - Ensure `switchgear-testing` is added as a dev-dependency
6. **Port conflicts** - The PortAllocator uses file-based locking to prevent conflicts
7. **Environment not set** - Remember to set `LNURL_BALANCER_CREDENTIALS_PATH` or `LNURL_SKIP_INTEGRATION_TESTS`

### Clean Restart

To completely reset the environment:
```bash
docker-compose down -v  # Remove volumes
docker-compose up --build -d
```

## Development Workflow

1. **Start environment:** `docker-compose up -d`
2. **Wait for setup:** `docker-compose logs -f setup`
3. **Set credentials path:** `export LNURL_BALANCER_CREDENTIALS_PATH=$(pwd)/shared-credentials/credentials`
4. **Run tests:** `cargo test --release`
5. **Iterate:** Make changes and re-run tests
6. **Clean up:** `docker-compose down` (or `down -v` for full reset)

