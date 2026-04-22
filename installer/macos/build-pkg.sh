#!/bin/bash
# Build macOS .pkg installer for magnolia
#
# Usage:
# VERSION=1.0.0 ./build-pkg.sh
#
# Prerequisites: Xcode Command Line Tools (pkgbuild, productbuild)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$SCRIPT_DIR/build"
PKG_ROOT="$BUILD_DIR/pkg-root"
VERSION="${VERSION:-1.0.0}"
IDENTIFIER="com.magnolia.server"
PLIST_DEST="Library/LaunchDaemons/com.magnolia.server.plist"

echo "Building magnolia macOS installer v$VERSION"
echo ""

# Clean previous build 
rm -rf "$BUILD_DIR"
mkdir -p "$PKG_ROOT/usr/local/bin"
mkdir -p "$PKG_ROOT/$PLIST_DEST"
rmdir "$PKG_ROOT/$PLIST_DEST" # ensure parent exists, not the leaf
mkdir -p "$BUILD_DIR/scripts"
mkdir -p "$BUILD_DIR/resources"

# Build release binaries 
echo "Building release binaries..."
cd "$PROJECT_ROOT/backend"
cargo build --release \
 --bin magnolia_server \
 --bin service_ctl \
 --bin create_admin

# Copy binaries 
cp "$PROJECT_ROOT/target/release/magnolia_server" "$PKG_ROOT/usr/local/bin/magnolia_server"
cp "$PROJECT_ROOT/target/release/service_ctl" "$PKG_ROOT/usr/local/bin/magnolia_server-ctl"
cp "$PROJECT_ROOT/target/release/create_admin" "$PKG_ROOT/usr/local/bin/magnolia-create-admin"
chmod 755 "$PKG_ROOT/usr/local/bin/magnolia_server"
chmod 755 "$PKG_ROOT/usr/local/bin/magnolia_server-ctl"
chmod 755 "$PKG_ROOT/usr/local/bin/magnolia-create-admin"

# Launcher script (sources env file, then exec's the server) 
# The LaunchDaemon plist points here. Users edit magnolia.env; reloading the
# daemon picks up new values without touching the plist XML.
cat > "$PKG_ROOT/usr/local/bin/magnolia-launcher" << 'LAUNCHER'
#!/bin/sh
set -a
CONF="/usr/local/etc/magnolia/magnolia.env"
[ -r "$CONF" ] && . "$CONF"
set +a
exec /usr/local/bin/magnolia_server
LAUNCHER
chmod 755 "$PKG_ROOT/usr/local/bin/magnolia-launcher"

# LaunchDaemon plist 
cp "$SCRIPT_DIR/com.magnolia.server.plist" "$PKG_ROOT/Library/LaunchDaemons/com.magnolia.server.plist"
chmod 644 "$PKG_ROOT/Library/LaunchDaemons/com.magnolia.server.plist"

# preinstall: stop any running service 
cat > "$BUILD_DIR/scripts/preinstall" << 'EOF'
#!/bin/bash
launchctl unload /Library/LaunchDaemons/com.magnolia.server.plist 2>/dev/null || true
exit 0
EOF
chmod 755 "$BUILD_DIR/scripts/preinstall"

# postinstall 
cat > "$BUILD_DIR/scripts/postinstall" << 'EOF'
#!/bin/bash
set -e

CONF_DIR="/usr/local/etc/magnolia"
CONF_FILE="$CONF_DIR/magnolia.env"
DATA_DIR="/usr/local/var/magnolia"
LOG_DIR="/usr/local/var/log/magnolia"
PLIST="/Library/LaunchDaemons/com.magnolia.server.plist"

# Directories 
mkdir -p "$DATA_DIR/uploads/images"
mkdir -p "$LOG_DIR"
mkdir -p "$CONF_DIR"

chmod 755 "$DATA_DIR"
chmod 755 "$LOG_DIR"
chmod 750 "$CONF_DIR"

# Detect upgrade vs fresh install 
if [ -f "$CONF_FILE" ]; then
 # Upgrade: preserve config, restart service 
 launchctl load "$PLIST" 2>/dev/null || true

 echo ""
 echo "============================================"
 echo " magnolia upgraded to $(sw_vers -productVersion 2>/dev/null || echo 'new version')"
 echo "============================================"
 echo ""
 echo "Configuration preserved at $CONF_FILE"
 echo "Service restarted."
 echo "View logs: tail -f $LOG_DIR/magnolia.log"
 echo ""
 exit 0
fi

# Fresh install: generate config
cat > "$CONF_FILE" << ENVEOF
# Magnolia Server Configuration
# Generated at install time — edit as needed, then reload the service:
#
#   sudo launchctl unload /Library/LaunchDaemons/com.magnolia.server.plist
#   sudo launchctl load  /Library/LaunchDaemons/com.magnolia.server.plist

# Application environment (production / development)
ENV=production

# Database connection URL
# SQLite: sqlite:///usr/local/var/magnolia/magnolia.db
# PostgreSQL: postgres://user:password@localhost/magnolia
DATABASE_URL=sqlite:///usr/local/var/magnolia/magnolia.db

# Server binding
HOST=0.0.0.0
PORT=3000

# Optional: open a second listener on localhost only (secure without TLS)
# LOCAL_PORT=3001

# Public base URL — UPDATE THIS before starting the service
# Example: https://magnolia.example.com
BASE_URL=http://localhost:3000

# CORS allowed origin — must match the URL used in the browser.
# Usually identical to BASE_URL. The server panics on startup if this is missing.
# UPDATE THIS along with BASE_URL.
WEB_ORIGIN=http://localhost:3000

# How many days a login session lasts (default: 7)
# SESSION_DURATION_DAYS=7

# Logging
# LOG_FORMAT=pretty          # "pretty" for human-readable; default is JSON
# LOG_OUTPUT=stdout          # "file", "both", or "stdout" (default)
# LOG_FILE_PATH=/usr/local/var/log/magnolia/magnolia.log
# LOG_INCLUDE_SOURCE=false   # "true" or "1" to include source file positions

# Rate limiting
# RATE_LIMIT_GLOBAL=100      # max requests per IP per window
# RATE_LIMIT_AUTH=5          # max failed login attempts per IP per window
# TRUSTED_PROXY=             # IP of your reverse proxy for X-Forwarded-For

# SMTP — leave commented to disable email features
# SMTP_HOST=smtp.example.com
# SMTP_PORT=587
# SMTP_USERNAME=user@example.com
# SMTP_PASSWORD=your-password
# SMTP_FROM=noreply@example.com

# Encryption at rest (optional — 64 hex chars = 32-byte AES-256 key)
# Generate with: openssl rand -hex 32
# ENCRYPTION_AT_REST_KEY=

# TURN server — disabled by default.
# To enable, set all four values and reload the service.
# SESSION_SECRET must be a random hex string (openssl rand -hex 32).
# TURN_ENABLED=true
# TURN_LISTEN_ADDR=0.0.0.0:3478
# TURN_REALM=magnolia
# TURN_EXTERNAL_IP=
# SESSION_SECRET=
ENVEOF

chmod 600 "$CONF_FILE"

# Load the service once now so it starts immediately, but do NOT use -w
# (which would permanently enable it). The user can enable autostart separately.
launchctl load "$PLIST" 2>/dev/null || true

echo ""
echo "============================================"
echo " Magnolia Server Installation Complete"
echo "============================================"
echo ""
echo "Configuration file: $CONF_FILE"
echo "Data directory: $DATA_DIR"
echo "Logs: $LOG_DIR"
echo ""
echo "NEXT STEPS:"
echo ""
echo "1. Edit $CONF_FILE"
echo ", set BASE_URL and WEB_ORIGIN to your domain"
echo ", configure SMTP if you need email features"
echo ""
echo "2. Reload the service to apply your changes:"
echo " sudo launchctl unload $PLIST"
echo " sudo launchctl load $PLIST"
echo ""
echo "3. Create the initial administrator account:"
echo " sudo magnolia-create-admin --email admin@example.com"
echo ""
echo "4. Open http://localhost:3000 in your browser"
echo ""
echo "To view logs:"
echo " tail -f $LOG_DIR/magnolia.log"
echo ""
EOF
chmod 755 "$BUILD_DIR/scripts/postinstall"

# Welcome and conclusion HTML 
cat > "$BUILD_DIR/resources/welcome.html" << 'EOF'
<!DOCTYPE html>
<html>
<head>
 <meta charset="utf-8">
 <title>Magnolia Server</title>
</head>
<body>
 <h1>Magnolia Server</h1>
 <p>This installer will set up the magnolia self-hosted social platform on your Mac.</p>
 <p>The service will be installed but <strong>not started automatically</strong> at boot by default.
 You can enable autostart after installation if you wish.</p>
 <h3>What will be installed:</h3>
 <ul>
 <li><code>/usr/local/bin/magnolia_server</code> — main server binary</li>
 <li><code>/usr/local/bin/magnolia_server-ctl</code> — service control utility</li>
 <li><code>/usr/local/bin/magnolia-create-admin</code> — admin account creation tool</li>
 <li><code>/usr/local/bin/magnolia-launcher</code> — service launcher (sources config)</li>
 <li><code>/Library/LaunchDaemons/com.magnolia.server.plist</code> — boot service</li>
 <li><code>/usr/local/etc/magnolia/magnolia.env</code> — configuration file (generated)</li>
 <li><code>/usr/local/var/magnolia/</code> — data directory</li>
 <li><code>/usr/local/var/log/magnolia/</code> — log directory</li>
 </ul>
 <p><strong>You will need to edit the configuration file and create an admin account after installation.</strong></p>
</body>
</html>
EOF

cat > "$BUILD_DIR/resources/conclusion.html" << 'EOF'
<!DOCTYPE html>
<html>
<head>
 <meta charset="utf-8">
 <title>Installation Complete</title>
</head>
<body>
 <h1>Installation Complete</h1>
 <p>Magnolia Server has been installed and the service has been loaded.</p>

 <h3>Required: complete setup in Terminal</h3>
 <ol>
 <li>
 <strong>Edit the configuration file:</strong><br>
 <code>sudo nano /usr/local/etc/magnolia/magnolia.env</code><br>
 Set <code>BASE_URL</code> and <code>WEB_ORIGIN</code> to your domain.
 </li>
 <li>
 <strong>Reload the service to apply changes:</strong><br>
 <code>sudo launchctl unload /Library/LaunchDaemons/com.magnolia.server.plist</code><br>
 <code>sudo launchctl load &nbsp; /Library/LaunchDaemons/com.magnolia.server.plist</code>
 </li>
 <li>
 <strong>Create the initial administrator account:</strong><br>
 <code>sudo magnolia-create-admin --email admin@example.com</code>
 </li>
 <li>
 <strong>Open the web interface:</strong><br>
 <a href="http://localhost:3000">http://localhost:3000</a>
 </li>
 </ol>

 <h3>Autostart at boot (optional)</h3>
 <p>The service started once after installation but is <strong>not enabled to run at every boot</strong> by default.</p>
 <ul>
 <li>Enable autostart: <code>sudo launchctl load -w /Library/LaunchDaemons/com.magnolia.server.plist</code></li>
 <li>Disable autostart: <code>sudo launchctl unload -w /Library/LaunchDaemons/com.magnolia.server.plist</code></li>
 </ul>

 <h3>Useful commands</h3>
 <ul>
 <li>View logs: <code>tail -f /usr/local/var/log/magnolia/magnolia.log</code></li>
 <li>Stop service: <code>sudo launchctl unload /Library/LaunchDaemons/com.magnolia.server.plist</code></li>
 <li>Start service: <code>sudo launchctl load /Library/LaunchDaemons/com.magnolia.server.plist</code></li>
 </ul>
</body>
</html>
EOF

# Build component package 
echo "Building component package..."
pkgbuild \
 --root "$PKG_ROOT" \
 --scripts "$BUILD_DIR/scripts" \
 --identifier "$IDENTIFIER" \
 --version "$VERSION" \
 --install-location "/" \
 "$BUILD_DIR/magnolia-component.pkg"

# Distribution.xml 
cat > "$BUILD_DIR/Distribution.xml" << EOF
<?xml version="1.0" encoding="utf-8"?>
<installer-gui-script minSpecVersion="1">
 <title>Magnolia Server $VERSION</title>
 <organization>com.magnolia</organization>
 <domains enable_localSystem="true"/>
 <options customize="never" require-scripts="true" rootVolumeOnly="true"/>
 <welcome file="welcome.html"/>
 <conclusion file="conclusion.html"/>
 <pkg-ref id="$IDENTIFIER"/>
 <choices-outline>
 <line choice="default">
 <line choice="$IDENTIFIER"/>
 </line>
 </choices-outline>
 <choice id="default"/>
 <choice id="$IDENTIFIER" visible="false">
 <pkg-ref id="$IDENTIFIER"/>
 </choice>
 <pkg-ref id="$IDENTIFIER" version="$VERSION" onConclusion="none">magnolia-component.pkg</pkg-ref>
</installer-gui-script>
EOF

# Final product archive 
echo "Building final installer package..."
productbuild \
 --distribution "$BUILD_DIR/Distribution.xml" \
 --resources "$BUILD_DIR/resources" \
 --package-path "$BUILD_DIR" \
 "$SCRIPT_DIR/magnolia-$VERSION.pkg"

echo ""
echo "Build complete: $SCRIPT_DIR/magnolia-$VERSION.pkg"
echo ""
echo "To install: open magnolia-$VERSION.pkg"
echo "Or via CLI: sudo installer -pkg magnolia-$VERSION.pkg -target /"
