# Magnolia
Anti-Social Media Server

## Purpose
The aim is to return the ownership of people's time, data, and privacy to themselves from corporations.
There is absolutely no need for anyone to see promotions of items and services they don't have use for in their lives, from companies on the other end of the planet.

All the while the middleman corporations gather your personal data to profit off you, and train AI on your personal identity.

Returning to the basics, Magnolia presents the basic functionality one would expect:
- messaging with friends, family members, and colleagues,
- sharing posts, images, videos, files, link with each other,
- voice and video calls.

All this in a package that anyone can install on any system to supply themselves and their close ones or associates with these features, without corporations leeching on them.

#### No integrated AI.
No AI systems are running in the server (because there is absolutely no need for them)

#### No ads. No microtransactions. No subscriptions.
Would you be paying it to yourself?

#### No telemetry.
The server is run by you, all data remains with you (as far as the server is concerned).

### Made for humans
Only for communication between humans as far as I am concerned. If you want to talk with bots, you'll have to do that part yourself.

## Advanced
The server instances are by default capable of connecting to other instances, creating what is known as a mesh network.
Currently server-to-server direct connections are working, but relays are also planned. Meaning currently any two servers can talk to each other, and the plan is for servers to be able to pass along messages for other nodes.
Two connecting servers, internally labeled as federation, allows the users of each server to communicate with each other.

## Features
- Basic communication features as per above.
- Encrypted communication between users (message, voice, video).
- Group messages and calls.
- A "global" (server-wide) voice call channel, that any user on the server can join (not shared with federated servers).
- Server to Server communication.
- Encrypted communication between servers.
- WebView Frontend (HTML, CSS, JS) served from the rust backend.
- No React, no daily critical vulnerability update, not breaking on every other build.
- If it builds, it runs.
- Thanks to servers identifying with signatures, they can change network addresses, and can work through VPNs without issue.
- Admin interface built into the frontend (for admin users) to manage and configure overarching settings,
like SMTP, federation, default color scheme, users, registration, etc.
- While your server is offline, the messages sent from other servers are not lost, and when reestablishing connection everyone receives the messages they missed while their server was offline.
- Passphrase protected message encryption key. Key stored on the server, but encrypted, so you can login from anywhere to view your messages, but only if you remember the passphrase. This also means that if someone logs into your account, they cannot read your messages unless they know your passphrase.

## Proposed/imagined server/user flow
1. Server installed by user or someone known to them.
2. User joins the server through:
 a. public registration.
 b. invitation (can be enforced that only the email set by the admin can use the invitation)
 c. application (registration request)
3. User communicates with other users

4. Admin finds someone else with a server
5. Server federates (connects with) other server, one admin sends a request to the other server, who accepts (or rejects it)
6. If user decides, they communicate with users on federated servers. Users can completely opt-out, use whitelist and blacklist modes to decide which servers they want to interact with.

## How to start (READ BEFORE YOU START)
(assuming Linux system, Windows works similarly but currently doesn't have automatic nginx configuration, macos was done completely on theory)
~~1. a) Download server installer binary of your choice (currently Windows and Linux(.deb for debian, .rpm for fedora))~~
1. b) Download source code to build for your system (There is a macos build script setup, but I have no compatible hardware so cannot test, best of luck).
2. Install on your system (it should get most of environment values from you in the process).
2. Addendum (IMPORTANT), during setup you will be prompted to include an admin account, if this doesn't happen or fails, the website should start in setup mode, meaning the first to connect gets to create an admin account through the web interface. If even this fails, there is a binary in the project that allows for inserting an admin user straight into the database.
3. Setup your reverse-proxy (if you have nginx the installer SHOULD add a new entry for the server).
3. Windows addendum: For windows you don't need nginx to run on your machine.
But if you need some extra info on in this direction, server is registered with Windows Service Control Manager, so there should be registry entry at `HKLM\SYSTEM\CurrentControlSet\Services\magnolia_server\Environment`.
4. Don't forget to open a port on your router (this allows the application to be reachable from the interwebz).
5. Setup your subdomain to the open port, and match with the right environment variables.

### E-mail
Admins can configure SMTP to send emails for password resets, and registration invitations.
If you don't configure during installation, or just need to change it, you can do it
through the admin interface. To configure you will generally need to also consult
your SMTP service. This is not hosted in this server, but elsewhere.
- `SMTP_HOST` host address of your SMTP service.
- `SMTP_PORT` same but port.
- `SMTP_USERNAME` self explaining.
- `SMTP_PASSWORD` self explaining.
- `SMTP_FROM` the email address you are using to send the emails.

### Data storage
Data for the server is (hopefully) stored on the machine where installed and running, not in any cloud instance.
Database is SQL schema based, capable of both SQLite and PostgreSQL.
Environment variable `DATABASE_URL` dictates which one is used, and where it can be accessed.

### Network variables
- `HOST` environment variable decides whether to run locally (127.0.0.1) or publically (0.0.0.0).
- `PORT` environment variable is the open port, where you should direct your reverse-proxy or domain.
- `BASE_URL` is the PUBLICLY accessible address for your server. It can be either IPv4(http://127.0.0.1:3000) or URL (http://whatever.mydomain.internet).
- `WEB_ORIGIN` generally the same as `BASE_URL`, used for cors.

#### LOCAL_PORT 
`LOCAL_PORT` optional variable, if present opens a second server port for localhost, in case you want to run the server both publically and locally. Through localhost access your connection doesn't go through the internet, and can be absolutely securely used without TLS. So even without TLS on servers, if you are using your localhost access to your server, and the server is federated with servers, they communicate with eachother encrypted, without exposing your data. 

### Security variables
- `SESSION_DURATION_DAYS` is how many days you stay logged in.
- `RATE_LIMIT_GLOBAL` is the number of calls an address (generally user device) can ask the server for something.
- `RATE_LIMIT_AUTH` is for setting how many times a login can be attempted unsuccessfully.
- `ENCRYPTION_AT_REST_KEY` gives the key for encrypting data that is not currently in memory (is unused at the moment).
- `TRUSTED_PROXY` sets a trusted proxy for the rate-limiters.
- `ENV` if the value is `development`, sets login cookie security header to false (true in all other cases).

### TURN
- `TURN_ENABLED` value `true` to turn on.
- `TURN_LISTEN_ADDR` is your `address:port` where you want to run your TURN server (default value if not given: `0.0.0.0:3478`).
- `TURN_REALM` the shared realm/topic TURN connections are looking for, default value `magnolia`.
- `SESSION_SECRET` used for TURN (it's for a specific type of connection that can be enabled for calls).
- `TURN_EXTERNAL_IP` is (as per name) the ip address where the TURN server can be reached from external sources.

### Logging variables
- `LOG_FORMAT` value `pretty` causes output to be "pretty", meaning contents are separated by spaces and new lines. All other values cause json-blob.
- `LOG_OUTPUT` value `file` prints the logs into file, `both` prints to file and stdout, anything else causes logs to go to only stdout.
- `LOG_FILE_PATH` value is the file where log files will go.
- `LOG_INCLUDE_SOURCE` value `true` or `1` will include code position in the source file to be logged as well.

### Planned
#### Native Desktop User Application
for low hardware requirement use-cases. Sometimes you are okay with running an app with 30MB ram usage and not a whole webview, or you just don't want to run a browser. Maybe you need more free hardware resource for other applications but still want to talk with a colleague/family member. Native applications can run way too many times better than running a whole webview for what? A panel and a text field? That really shouldn't take a whole gigabyte of ram and 4 cpu cores, snatching everything you do and labeling it "telemetry".

#### Mobile Application
I'm planning to build a mobile application that is fully working with one or even more server instances, with operations for a single server, or multiple associated servers.

#### Public content feature
I don't have enough RAM to tell what this is.

#### Convenience features

#### Localizations

#### Better documentation
Currently missing easily understandable documentation for users and developers for extensions and repairs.
Documentation to help connect bots (yeah I know... but this is a pretty standard feature at this point) if you want to get messages from them.

#### More UI styles

#### Frontend needs real-time update.
Some elements like incoming federation requests sometimes only update when refreshing. Needs further testing to find bottlenecks.

### Known limitations/bugs
- Federated servers are currently not sharing media files between eachother, whether in messages or posts.
- If a new server is federated, users have to press the "save" button on their federation settings page, before the newly federated server sees them.
- Servers are currently communicating mostly with POST requests instead of the established web-socket channel. Both are encrypted, it's just a work-in-progress feature.
- When the server was offline and you receive an image/video/file, it is loaded when your server reestablishes connection. If you are logged in in the meantime, you might need to refresh the page for the file to load into the frontend. It will still be visible that there is an attachment, but for for example a video, it will not display even a thumbnail.
- Groups calls between servers can be problematic. Haven't figured out the source of it yet.
- (IMPORTANT) Password reset is not in yet. Not sure how to do it in case no SMTP/email is setup.
- (IMPORTANT) Windows installer is not happy with itself. Firewall "persistence" because the server starts with the system, and if you try to add it to the firewall exceptions, it fails again for "defense avoidance". Yet all of the telemetry in the system is okay to start anytime... Also permissions issues, and cannot do anything if installed. The server binary if you put it into a folder and just start it, runs perfectly, but if you try to be friendly and all, fails on mmultiple levels.

## Testing
Setting up two separate servers (with their own databases, and on separate ports) works, with the caveat that when you are trying voice and/or video calls, and you have one of the servers on localhost (127.0.0.1) and the other on the public net (0.0.0.0) (for example because you want to test mobile login into your server) it will only work if your localhost server is calling the public address; while the other way around will not connect in the browser.

##### CLI command store:
- Run release compiled server on localhost port 3000 with `my.db` as the database file.
```bash
HOST=127.0.0.1 PORT=3000 BASE_URL=http://127.0.0.1:3000/ DATABASE_URL=sqlite:./my.db cargo run --release --bin magnolia_server
```

### Building server from source
Pretty much the same as the command just above.
You'll need `rustc`, try `cargo`. I'm as of now running on rustc 1.92.0.

Take a look at: [Rust](https://rust-lang.org/)

### Building installer from source
Look inside the installer folder, you will find... more folders. Select your target platform.

#### From windows to windows
you'll need inno.

#### From windows to linux
you'll need zig for the cross-compile. build script attached.
Prerequisites:
```
winget install zig.zig
cargo install cargo-zigbuild cargo-deb
```

Usage:
```
cross-build.bat
cross-build.bat --target aarch64-unknown-linux-gnu
```

#### Linux to linux just works.

#### There is also a mac one...
but I've no idea about mac, godspeed.

## FAQ
#### My browser doesn't ask for passphrase.
Your browser probably disallows security for your session (your server is not on HTTPS but only HTTP, and you are not on localhost), it will not allow you to use encryption. So you either have to get a certificate on your address, get the exception into your browser (this will turn your connection insecure, and all your data will go around unencrypted), or you need to switch to the localhost port (if the server is running on your machine and you enabled it with `LOCAL_PORT`).

## What I learned
WebRTC is a pain in the everything. But really! Usually you can just serialize things, encrypt things, send them over the wire, decrypt, deserialize, and you are done. But WebRTC is like nooooooooooooooooooo buddy. Or might just be that browsers are bloated and like to cause issues, you be the judge of that. 

## License

This work is licensed under Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International. To view a copy of this license, visit https://creativecommons.org/licenses/by-nc-sa/4.0/
