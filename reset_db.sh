#!/usr/bin/env bash
set -e

echo "Resetting Magnolia database..."

rm -f magnolia.db magnolia.db-shm magnolia.db-wal
echo " Database files removed."

echo ""
echo "Tables will be recreated on next server start."
echo ""
echo "To also clear uploaded media, run:"
echo " rm -rf media_storage"
echo ""
echo "Start the server with:"
echo " cargo run -p magnolia_server"
