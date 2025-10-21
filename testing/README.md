# Testing

Docker-based regtest environment for testing with Lightning Network nodes (CLN, LND), Bitcoin Core, and databases.

## Local Testing

1. **Start services:**
   ```bash
   cd testing
   docker compose up -d --build --wait 
   ```

2. **Copy environment configuration:**
   ```bash
   cp testing/.env .
   ```

3. **Edit `.env` and change all service names to localhost:**

```shell
CLN_HOSTNAME=localhost
CREDENTIALS_SERVER_HOSTNAME=localhost
LND_HOSTNAME=localhost
MYSQL_HOSTNAME=localhost
POSTGRES_HOSTNAME=localhost
```

4. **Run tests:**
   ```bash
   cargo test
   ```

**Skip integration tests with service dependencies:**
```bash
SWGR_SKIP_INTEGRATION_TESTS=true cargo test
```

## Docker-in-Docker CI Testing

For running tests inside a container with Docker socket access.

1. **Start services:**
   ```bash
   cd testing
   docker compose up -d --build --wait 
   ```

2. **Connect container to services network:**
   ```bash
   . testing/.env
   docker network connect $SERVICES_NETWORK_NAME $(hostname)
   ```

3. **Copy environment configuration:**
   ```bash
   cp testing/.env .
   ```

4. **Run tests:**
   ```bash
   cargo test
   ```




