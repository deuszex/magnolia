# Magnolia Python Client

Session-based and HMAC proxy client for the Magnolia API.

## Requirements

- Python 3.10+
- [requests](https://pypi.org/project/requests/)
- [websocket-client](https://pypi.org/project/websocket-client/) *(optional - only needed for `connect_websocket()`)*

## Installation

```sh
pip install requests
# Optional WebSocket support:
pip install websocket-client
```

No build step. Import directly:

```python
from magnolia_client import MagnoliaClient, MagnoliaHMACClient
```

## Usage

### Session client

```python
from magnolia_client import MagnoliaClient

client = MagnoliaClient("https://magnolia.example.com", timeout=10.0)
client.login("alice", "hunter2")

posts = client.list_posts(limit=10)
post  = client.create_post(
    [{"content_type": "text", "display_order": 0, "content": "Hello world"}],
    publish=True,
    tags=["intro"],
)
client.logout()
```

### HMAC proxy client

```python
from magnolia_client import MagnoliaHMACClient

proxy = MagnoliaHMACClient(
    "https://magnolia.example.com",
    proxy_id="<proxy-uuid>",
    hmac_key="<64-char-hex-key>",
    timeout=10.0,
)
conv = proxy.get_or_create_conversation(target_username="bob")
proxy.send_message(conv["conversation_id"], "encrypted-payload")
```

The `hmac_key` is the 64-character lowercase hex string stored on the proxy account. It is used as raw UTF-8 key material - do **not** hex-decode it before passing.

## Running the tests

The test suite runs against a live server. All network calls use a 10-second timeout.

### Required environment variables

| Variable | Description |
|---|---|
| `MAGNOLIA_BASE_URL` | Server root, e.g. `https://magnolia.example.com` |
| `MAGNOLIA_USERNAME` | Login identifier (username or email) |
| `MAGNOLIA_PASSWORD` | Login password |

### Optional environment variables

Missing optional variables cause the relevant tests to be **skipped**, not failed.

| Variable | Enables |
|---|---|
| `MAGNOLIA_PROXY_ID` | HMAC proxy tests (required together with `MAGNOLIA_HMAC_KEY`) |
| `MAGNOLIA_HMAC_KEY` | HMAC proxy tests (required together with `MAGNOLIA_PROXY_ID`) |
| `MAGNOLIA_MEDIA_FILE` | Media upload, download, chunked upload, and HMAC media upload tests |
| `MAGNOLIA_TARGET_USER_ID` | Conversation and message tests |

### Install test dependency

```sh
pip install pytest
```

### Run

```sh
export MAGNOLIA_BASE_URL=https://magnolia.example.com
export MAGNOLIA_USERNAME=alice
export MAGNOLIA_PASSWORD=hunter2

# Optional:
export MAGNOLIA_PROXY_ID=<proxy-uuid>
export MAGNOLIA_HMAC_KEY=<64-char-hex>
export MAGNOLIA_MEDIA_FILE=/path/to/file.png
export MAGNOLIA_TARGET_USER_ID=<user-uuid>

pytest test_magnolia.py -v
```

### Run a single test

```sh
pytest test_magnolia.py::test_posts_full_lifecycle -v
```
