# Magnolia Client documentation

Some help to create your own client application/connection for easier fulfillment of your use-cases, in the direction of automation and bot creation.

To clarify this is not a guide on server to server communication, but for your other applications to be able to connect as a "proxy user" to an existing and running server, or to make your own client side application.

As such the base assumption is that you have knowledge of a running server, have it's address, and have login credentials.

## Session

As mentioned sessions are the regular operation that users are directing using from the front-end UI. Same endpoints, same API, and using the same sessions and credentials. The sole caveat for here for proxies is they use a different login and logout endpoint compared to regular users.

[Session doc](client/session.md)


## HMAC one-shot

The one-shot endpoints are specifically made for quick and easy (at least simpler) operations for creating posts, and sending messages. Get a key created and save it on the server while in session as a user or admin. That key can then be used to sign the message.

[HMAC one-shot](client/hmac.md)


## Client codebases
Client code (and tests) are available in the following programming languages here:

- C++ (planned)
- C# (in progress)
- Go (in progress)
- Java (planned)
- Lisp (planned)
- Ocaml (planned)
- Python (Done and tested)
- Rust (planned)
- Scala (planned)
- Typescript (in progress)
- Zig (planned)