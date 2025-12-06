# Testing

Docker-based regtest environment for testing with Lightning Network nodes (CLN, LND), Bitcoin Core, and databases.

## Local Testing

1. **Start services:**
   ```bash
   cd testing
   docker compose --env-file ./testing.env up -d --build --wait 
   ```

2. **Copy environment configuration:**
   ```bash
   cp testing/testing.env ./testing.env
   ```

3. **Edit `testing.env` and change all service names to localhost:**

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

## Docker-in-Docker CI Testing

For running tests inside a container with Docker socket access.

1. **Start services:**
   ```bash
   cd testing
   docker compose --env-file ./testing.env up -d --build --wait 
   ```

2. **Connect container to services network:**
   ```bash
   . testing/testing.env
   docker network connect $SERVICES_NETWORK_NAME $(hostname)
   ```

3. **Copy environment configuration:**
   ```bash
   cp testing/testing.env ./testing.env
   ```

4. **Run tests:**
   ```bash
   cargo test
   ```




