# unlockd

`unlockd` is a small Rust-based PAM authentication module that allows a locked `hyprlock` session to be unlocked through an authenticated HTTP request.

The basic flow is:

```text
iPhone Shortcut
      │
      │  HTTP request + X-API-Key
      ▼
 unlockd HTTP server
      │
      │  authentication approved
      ▼
   PAM module
      │
      ▼
   hyprlock
      │
      ▼
   unlocked
```

The HTTP server is **not permanently running**. It is started when PAM invokes `unlockd` during authentication and waits for an authenticated request for a short period of time.

## Build & Install

Build the PAM module:

```bash
cargo build --release
```

Install it:

```bash
sudo cp target/release/libunlockd.so /usr/lib/security/pam_unlockd.so
```

The resulting shared library is installed as:

```text
/usr/lib/security/pam_unlockd.so
```

## Hyprlock Configuration

Add `pam_unlockd.so` to the PAM configuration used by Hyprlock.

For example, `/etc/pam.d/hyprlock`:

```text
# PAM configuration file for hyprlock
# the 'login' configuration file (see /etc/pam.d/login)
auth    sufficient       pam_unlockd.so
auth    include       login

account include       login
password include      login
session include       login
```

The important line is:

```text
auth    sufficient       pam_unlockd.so
```

`unlockd` is configured as a `sufficient` authentication method, so a successful request through `unlockd` is enough to authenticate the session.

## Configuration

On its first invocation, `unlockd` creates its configuration directory and generates a configuration file:

```text
~/.config/unlockd/unlockd.toml
```

If no valid configuration exists, `unlockd` generates a default configuration and prints it to stdout.

The default configuration is:

```toml
bind_server = "0.0.0.0:8892"
api_key = "generated-api-key"
```

The API key is generated automatically when the configuration is first created.

The configuration can then be edited manually at:

```text
~/.config/unlockd/unlockd.toml
```

## Unlocking with an HTTP Request

`unlockd` exposes one endpoint:

```text
/auth_baby
```

The request must contain the API key in the `X-API-Key` header.

Conceptually:

```http
GET /auth_baby
X-API-Key: <your-api-key>
```

A valid API key causes the PAM authentication request to succeed.

Invalid or missing API keys are rejected.

Requests to any other endpoint are rejected.

## The Hyprlock Flow

There is one slightly unintuitive part of the current implementation.

When the screen is locked, **you need to press Enter while the password field is active**.

That causes Hyprlock to submit the PAM authentication request, which invokes `pam_unlockd.so`.

At that point:

```text
Hyprlock
   │
   │ PAM authentication
   ▼
pam_unlockd.so
   │
   │ starts HTTP server
   ▼
HTTP server listening
   │
   │ waits for authenticated request
   ▼
iPhone Shortcut
   │
   │ /auth_baby + X-API-Key
   ▼
unlockd
   │
   │ authentication approved
   ▼
PAM_SUCCESS
   │
   ▼
Hyprlock unlocks
```

The HTTP server therefore only exists while an authentication attempt is in progress.

## iPhone Shortcut

I use it as iPhone Shortcut.

The Shortcut:

1. Sends an HTTP request to the machine running `unlockd`.
2. Calls:

   ```text
   /auth_baby
   ```

3. Includes the configured API key in the `X-API-Key` header.
4. Opens Hyprlock.

The destination IP address can be configured in the Shortcut.

## Authentication Timeout

`unlockd` waits for an authenticated request for **10 seconds**.

If no valid request arrives within that period, PAM authentication fails.

This prevents the authentication attempt from waiting indefinitely.

## Debug Logging

Debug information is written to:

```text
/tmp/pam_auth_baby_debug.log
```

This can be useful when troubleshooting PAM configuration, server startup, or authentication requests.

For example:

```bash
sudo tail -f /tmp/pam_auth_baby_debug.log
```

## Security

`unlockd` is intended for use on a trusted network.

The API key acts as the authentication credential for the HTTP endpoint. Anyone who can successfully send a request containing the correct API key can cause the pending PAM authentication attempt to succeed.

Because the default server configuration binds to:

```text
0.0.0.0:8892
```

the listening socket is available on the machine's network interfaces.

Consider your network environment and firewall configuration accordingly.
