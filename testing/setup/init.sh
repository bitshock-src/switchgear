#!/bin/sh
set -e

/ln.sh

if [ $# -eq 0 ]; then
    /credentials.sh --cln-address "127.0.0.1:9736" --lnd-address "127.0.0.1:10009"
else
    /credentials.sh "$@"
fi

touch /tmp/setup_complete
sleep infinity