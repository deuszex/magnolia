# Magnolia C# Client

Session-based and HMAC proxy client for the Magnolia API.

## Requirements

- .NET 8+
- No external runtime dependencies - uses only inbox .NET libraries (`System.Net.Http`, `System.Text.Json`, `System.Security.Cryptography`, `System.Net.WebSockets`)

## Setup

Reference `MagnoliaClient.csproj` from your project:

```xml
<ItemGroup>
  <ProjectReference Include="path/to/MagnoliaClient.csproj" />
</ItemGroup>
```

Or copy `MagnoliaClient.cs` directly into your project - it has no NuGet dependencies.

## Usage

### Session client

```csharp
using Magnolia;

await using var client = new MagnoliaClient("https://magnolia.example.com",
    timeout: TimeSpan.FromSeconds(10));

await client.LoginAsync("alice", "hunter2");

var posts = await client.ListPostsAsync(new(Limit: 10));

var post = await client.CreatePostAsync(
    [new("text", 0, "Hello world")],
    publish: true,
    tags: ["intro"]);

await client.LogoutAsync();
```

### HMAC proxy client

```csharp
using Magnolia;

using var proxy = new MagnoliaHMACClient(
    "https://magnolia.example.com",
    proxyId:  "<proxy-uuid>",
    hmacKey:  "<64-char-hex-key>",
    timeout:  TimeSpan.FromSeconds(10));

var conv = await proxy.GetOrCreateConversationAsync(targetUsername: "bob");
await proxy.SendMessageAsync(conv.ConversationId, "encrypted-payload");
```

The `hmacKey` is the 64-character lowercase hex string stored on the proxy account. It is used as raw UTF-8 key material - do **not** hex-decode it before passing.

Both clients implement `IDisposable` and should be disposed (or used with `using`) when no longer needed.

## Running the tests

The test suite uses [xUnit](https://xunit.net/) and runs against a live server. All network calls use a 10-second timeout.

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

### Run

```sh
cd client/csharp

export MAGNOLIA_BASE_URL=https://magnolia.example.com
export MAGNOLIA_USERNAME=alice
export MAGNOLIA_PASSWORD=hunter2

# Optional:
export MAGNOLIA_PROXY_ID=<proxy-uuid>
export MAGNOLIA_HMAC_KEY=<64-char-hex>
export MAGNOLIA_MEDIA_FILE=/path/to/file.png
export MAGNOLIA_TARGET_USER_ID=<user-uuid>

dotnet test MagnoliaClient.Tests.csproj -v normal
```

### Run a single test class or method

```sh
# All tests in a class:
dotnet test MagnoliaClient.Tests.csproj --filter "FullyQualifiedName~PostTests"

# A specific method:
dotnet test MagnoliaClient.Tests.csproj --filter "FullyQualifiedName~PostTests.FullLifecycle"
```

### Project structure

| File | Purpose |
|---|---|
| `MagnoliaClient.cs` | Client library - no test dependencies |
| `MagnoliaClient.csproj` | Library project (excludes test file) |
| `MagnoliaClient.Tests.cs` | xUnit integration tests |
| `MagnoliaClient.Tests.csproj` | Test project (references the library project) |
