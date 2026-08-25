use fs4::fs_std::FileExt;
use indexmap::IndexSet;
use rand::Rng;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener};
use std::path::Path;

const GLOBAL_LOCK_FILE: &str = "ports.txt";
const GLOBAL_LOCK_FILE_SIZE: usize = 256;

// Keep test-assigned listener ports out of every platform's ephemeral pool —
// the range the kernel hands out as implicit source ports for outbound client
// connections. Otherwise a freshly-allocated server port races against an
// in-flight ephemeral source port elsewhere in the test process tree and the
// server's bind() fails with EADDRINUSE. Linux's default ephemeral floor
// (net.ipv4.ip_local_port_range) is the lowest at 32768; macOS/Windows/BSD
// start at 49152. Ceiling sits just below the lowest (32768) so we stay clear
// on every OS.
const PORT_RANGE_START: u16 = 16384;
const PORT_RANGE_END: u16 = 32768;
const MAX_TRIES: usize = 4096;

pub struct PortAllocator {}

impl PortAllocator {
    pub fn find_available_port(ports_path: &Path) -> anyhow::Result<u16> {
        let ports_path = ports_path.join(GLOBAL_LOCK_FILE);
        let mut ports_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(ports_path)?;

        ports_file.lock_exclusive()?;

        let mut contents = String::new();
        ports_file.read_to_string(&mut contents)?;

        let mut assigned_ports: IndexSet<u16> = contents
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();

        let mut rng = rand::rng();
        for _ in 0..MAX_TRIES {
            let port = rng.random_range(PORT_RANGE_START..PORT_RANGE_END);
            if assigned_ports.contains(&port) {
                continue;
            }
            // Probe the same wildcard address the server child binds (0.0.0.0),
            // not just loopback: a 0.0.0.0 bind only succeeds if the port is free
            // on every interface, so a successful probe actually guarantees the
            // child's bind will succeed. Probing 127.0.0.1 would pass while some
            // other-interface bind holds the port, letting the child fail with
            // EADDRINUSE. The child binds through the same std/tokio TcpListener
            // path, so this probe mirrors its bind semantics exactly.
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port);
            let Ok(_listener) = TcpListener::bind(addr) else {
                continue;
            };

            assigned_ports.insert(port);
            if assigned_ports.len() > GLOBAL_LOCK_FILE_SIZE {
                assigned_ports.shift_remove_index(0);
            }
            ports_file.seek(SeekFrom::Start(0))?;
            ports_file.set_len(0)?;
            let contents = assigned_ports
                .iter()
                .map(|p| p.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            ports_file.write_all(contents.as_bytes())?;
            ports_file.sync_all()?;

            return Ok(port);
        }

        anyhow::bail!(
            "no free port in [{PORT_RANGE_START}, {PORT_RANGE_END}) after {MAX_TRIES} tries"
        );
    }
}
