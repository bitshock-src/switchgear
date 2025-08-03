use fs4::fs_std::FileExt;
use indexmap::IndexSet;
use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::net::TcpListener;
use std::path::Path;
use std::thread::sleep;
use std::time::Duration;

const GLOBAL_LOCK_FILE: &str = "ports.txt";
const GLOBAL_LOCK_FILE_SIZE: usize = 256;

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

        // Read assigned ports from file
        let mut contents = String::new();
        ports_file.read_to_string(&mut contents)?;

        let mut assigned_ports: IndexSet<u16> = contents
            .lines()
            .filter_map(|line| line.trim().parse().ok())
            .collect();

        loop {
            let listener = TcpListener::bind("127.0.0.1:0")?;
            let port = listener.local_addr()?.port();

            {
                if !assigned_ports.contains(&port) {
                    assigned_ports.insert(port);
                    if assigned_ports.len() > GLOBAL_LOCK_FILE_SIZE {
                        assigned_ports.shift_remove_index(0);
                    }
                    // Write updated ports list to file
                    ports_file.seek(SeekFrom::Start(0))?;
                    ports_file.set_len(0)?; // Truncate file
                    let contents = assigned_ports
                        .iter()
                        .map(|p| p.to_string())
                        .collect::<Vec<_>>()
                        .join("\n");
                    ports_file.write_all(contents.as_bytes())?;
                    ports_file.sync_all()?;

                    return Ok(port);
                }
            }
            sleep(Duration::from_millis(10));
        }
    }
}
