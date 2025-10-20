#!/bin/bash

MAX_SECONDS=${1:-60}
ELAPSED=0

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"

if [ ! -f "$COMPOSE_FILE" ]; then
    echo "Error: docker-compose.yml not found at $COMPOSE_FILE"
    exit 1
fi

echo "Waiting for credentials-server to be healthy (timeout: ${MAX_SECONDS}s)..."

while [ $ELAPSED -lt $MAX_SECONDS ]; do
    if ! docker inspect credentials-server >/dev/null 2>&1; then
        echo "⏳ [$ELAPSED/${MAX_SECONDS}s] credentials-server container not found, waiting..."
        sleep 1
        ELAPSED=$((ELAPSED + 1))
        continue
    fi

    CONTAINER_STATE=$(docker inspect --format='{{.State.Status}}' credentials-server 2>/dev/null)

    if [ "$CONTAINER_STATE" != "running" ]; then
        echo "⏳ [$ELAPSED/${MAX_SECONDS}s] credentials-server is not running (state: $CONTAINER_STATE), waiting..."
        sleep 1
        ELAPSED=$((ELAPSED + 1))
        continue
    fi

    HEALTH_STATUS=$(docker inspect --format='{{if .State.Health}}{{.State.Health.Status}}{{else}}no_healthcheck{{end}}' credentials-server 2>/dev/null)

    case "$HEALTH_STATUS" in
        "healthy")
            echo "✓ credentials-server is healthy (after ${ELAPSED}s)"
            exit 0
            ;;
        "starting")
            echo "⏳ [$ELAPSED/${MAX_SECONDS}s] credentials-server is still starting..."
            ;;
        "unhealthy")
            echo "⏳ [$ELAPSED/${MAX_SECONDS}s] credentials-server is unhealthy, waiting..."
            ;;
        "no_healthcheck")
            echo "✗ credentials-server has no healthcheck configured"
            exit 1
            ;;
        *)
            echo "⏳ [$ELAPSED/${MAX_SECONDS}s] credentials-server health status: $HEALTH_STATUS, waiting..."
            ;;
    esac

    sleep 1
    ELAPSED=$((ELAPSED + 1))
done

echo "✗ Timeout reached after ${MAX_SECONDS}s - credentials-server is not healthy"
exit 1
