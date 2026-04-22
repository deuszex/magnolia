# Magnolia Go Client

Session-based and HMAC proxy client for the Magnolia API.

## Requirements

- Go 1.21+
- [`github.com/gorilla/websocket`](https://github.com/gorilla/websocket) *(only needed for `ConnectWebSocket()`)*

## Installation

```sh
go get magnolia/client
```

Or add to an existing module:

```sh
go get github.com/gorilla/websocket@v1.5.3
```

## Usage

### Session client

```go
import magnolia "magnolia/client"

c := magnolia.NewClient("https://magnolia.example.com", magnolia.WithTimeout(10*time.Second))

if _, err := c.Login("alice", "hunter2"); err != nil {
    log.Fatal(err)
}

posts, err := c.ListPosts(magnolia.ListPostsParams{Limit: 10})

post, err := c.CreatePost([]magnolia.PostContentRequest{
    {ContentType: "text", DisplayOrder: 0, Content: "Hello world"},
}, true, []string{"intro"})

c.Logout()
```

### HMAC proxy client

```go
p := magnolia.NewHMACClient(
    "https://magnolia.example.com",
    "<proxy-uuid>",
    "<64-char-hex-key>",
    magnolia.WithTimeout(10*time.Second),
)

conv, err := p.GetOrCreateConversation("", "bob") // targetUserID, targetUsername
resp, err := p.SendMessage(conv.ConversationID, "encrypted-payload", nil)
```

The `hmacKey` is the 64-character lowercase hex string stored on the proxy account. It is used as raw UTF-8 key material - do **not** hex-decode it before passing.

## Running the tests

The test suite runs against a live server using Go's standard `testing` package. All network calls use a 10-second timeout.

### Required environment variables

| Variable | Description |
|---|---|
| `MAGNOLIA_BASE_URL` | Server root, e.g. `https://magnolia.example.com` |
| `MAGNOLIA_USERNAME` | Login identifier (username or email) |
| `MAGNOLIA_PASSWORD` | Login password |

### Optional environment variables

Missing optional variables cause the relevant subtests to be **skipped**, not failed.

| Variable | Enables |
|---|---|
| `MAGNOLIA_PROXY_ID` | HMAC proxy subtests (required together with `MAGNOLIA_HMAC_KEY`) |
| `MAGNOLIA_HMAC_KEY` | HMAC proxy subtests (required together with `MAGNOLIA_PROXY_ID`) |
| `MAGNOLIA_MEDIA_FILE` | Media upload, download, chunked upload, and HMAC media upload subtests |
| `MAGNOLIA_TARGET_USER_ID` | Conversation and message subtests |

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

go test -v -run TestIntegration ./...
```

### Run a single subtest

```sh
go test -v -run 'TestIntegration/Posts/Create' ./...
```

### Timeout

The `-timeout` flag controls the overall test binary timeout (default 10 minutes). The per-request HTTP timeout is set inside the test at 10 seconds and is independent.

```sh
go test -v -timeout 2m -run TestIntegration ./...
```
