@echo off
setlocal

echo Resetting Magnolia database...

:: Delete SQLite database and WAL files
if exist magnolia.db (
 del /f magnolia.db
 echo Deleted magnolia.db
)
if exist magnolia.db-shm (
 del /f magnolia.db-shm
 echo Deleted magnolia.db-shm
)
if exist magnolia.db-wal (
 del /f magnolia.db-wal
 echo Deleted magnolia.db-wal
)

echo.
echo Database files removed. Tables will be recreated on next server start.
echo.
echo To also clear uploaded media, run:
echo rmdir /s /q media_storage
echo.
echo Start the server with:
echo cargo run -p magnolia_server
