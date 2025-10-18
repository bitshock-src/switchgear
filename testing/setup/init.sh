#!/bin/sh
set -e

touch /shared/setup_started

/ln.sh

if [ $# -eq 0 ]; then
    /credentials.sh --cln-address "127.0.0.1:9736" --lnd-address "127.0.0.1:10009"
else
    /credentials.sh "$@"
fi

touch /tmp/setup_complete
touch /shared/credentials/setup_complete
sleep infinity