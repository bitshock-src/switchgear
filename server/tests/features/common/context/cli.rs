use std::ffi::OsStr;
use std::path::PathBuf;
use std::process::Command;
use std::str::from_utf8;
use tempfile::TempDir;

pub struct CliContext {
    cli: SwgrCli,
    pub temp_dir: TempDir,
    pub public_key_path: PathBuf,
    pub private_key_path: PathBuf,
    pub token_path: PathBuf,
    pub token_stdin: Option<String>,
    pub backend_json_path: PathBuf,
    pub offer_json_path: PathBuf,
    pub metadata_json_path: PathBuf,
}

impl CliContext {
    pub fn create() -> anyhow::Result<Self> {
        let temp_dir = TempDir::new()?;
        let public_key_path = temp_dir.path().join("public.pem");
        let private_key_path = temp_dir.path().join("private.pem");
        let token_path = temp_dir.path().join("token.txt");
        let backend_json_path = temp_dir.path().join("backend.json");
        let offer_json_path = temp_dir.path().join("offer.json");
        let metadata_json_path = temp_dir.path().join("metadata.json");

        Ok(Self {
            cli: SwgrCli::create(log::Level::Info)?,
            temp_dir,
            public_key_path,
            private_key_path,
            token_path,
            token_stdin: None,
            backend_json_path,
            offer_json_path,
            metadata_json_path,
        })
    }

    pub fn reset(&mut self) {
        self.cli.reset()
    }

    pub fn command<EI, K, V, AI, S>(&mut self, envs: EI, args: AI) -> anyhow::Result<()>
    where
        EI: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
        AI: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.cli.command(envs, args)
    }

    pub fn stdout_buffer(&self) -> &[String] {
        self.cli.stdout_buffer()
    }

    pub fn stderr_buffer(&self) -> &[String] {
        self.cli.stderr_buffer()
    }

    pub fn exit_code(&self) -> i32 {
        self.cli.exit_code()
    }
}

struct SwgrCli {
    has_rust_log: bool,
    rust_log: String,
    exit_code: i32,
    stdout_buffer: Vec<String>,
    stderr_buffer: Vec<String>,
}

impl SwgrCli {
    pub fn create(log_level: log::Level) -> anyhow::Result<Self> {
        let rust_log = std::env::var("RUST_LOG")
            .unwrap_or_else(|_| "".to_string())
            .to_lowercase();
        let has_rust_log = !rust_log.is_empty();

        let rust_log = if has_rust_log {
            rust_log
        } else {
            log_level.to_string()
        };

        Ok(Self {
            has_rust_log,
            rust_log,
            exit_code: -1,
            stdout_buffer: vec![],
            stderr_buffer: vec![],
        })
    }

    pub fn reset(&mut self) {
        self.exit_code = -1;
        self.stdout_buffer.clear();
        self.stderr_buffer.clear();
    }

    pub fn command<EI, K, V, AI, S>(&mut self, envs: EI, args: AI) -> anyhow::Result<()>
    where
        EI: IntoIterator<Item = (K, V)>,
        K: AsRef<OsStr>,
        V: AsRef<OsStr>,
        AI: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        let mut command = self.build_command()?;
        command.envs(envs).args(args);
        self.execute_command(&mut command)?;
        Ok(())
    }

    fn build_command(&self) -> anyhow::Result<Command> {
        let binary_path = Self::get_binary_path();
        let mut command = Command::new(&binary_path);
        command
            .env("RUST_LOG", self.rust_log.clone())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        Ok(command)
    }

    fn execute_command(&mut self, command: &mut Command) -> anyhow::Result<()> {
        let output = command.output()?;
        self.exit_code = output.status.code().unwrap_or(-1);

        let stdout: Vec<String> = from_utf8(&output.stdout)?
            .lines()
            .map(|s| s.to_string())
            .collect();
        let stderr: Vec<String> = from_utf8(&output.stderr)?
            .lines()
            .map(|s| s.to_string())
            .collect();

        if self.has_rust_log {
            for line in &stdout {
                println!("[STDOUT:swgr] {line}");
            }
            for line in &stderr {
                println!("[STDERR:swgr] {line}");
            }
        }

        self.stdout_buffer.extend(stdout);
        self.stderr_buffer.extend(stderr);

        Ok(())
    }

    fn get_binary_path() -> PathBuf {
        PathBuf::from(env!("CARGO_BIN_EXE_swgr"))
    }

    pub fn stdout_buffer(&self) -> &[String] {
        self.stdout_buffer.as_slice()
    }

    pub fn stderr_buffer(&self) -> &[String] {
        self.stderr_buffer.as_slice()
    }

    pub fn exit_code(&self) -> i32 {
        self.exit_code
    }
}
