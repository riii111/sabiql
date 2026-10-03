# Password storage and recovery

New or changed passwords use macOS Keychain, Windows Credential Manager, or
Secret Service on Linux/FreeBSD. Secret Service needs an available store such as
GNOME Keyring, KWallet, or KeePassXC and a working login/session bus.
Source builds on FreeBSD also need system `dbus` and `pkgconf`.
Android/Termux cannot save passwords. Failed secure storage never silently falls
back to plaintext.

## Alternative authentication

If secure storage is unavailable, PostgreSQL can use an empty saved password
with `.pgpass` or a PostgreSQL service. PostgreSQL and MySQL can also use
`--connection-env`. MySQL does not read the usual option/login-path files;
see [MySQL support](mysql.md).

## Recovery

A password-store error does not mean saved connections were deleted. Check the
store's lock, permissions, service, and login session, then restart sabiql.
If a credential is missing, set a new password. Editing unrelated fields keeps
its existing reference. Copying configuration to another machine does not copy
OS-store credentials.

A cleanup warning means the configuration change was saved but removal of the
old credential could not be confirmed. A save keeps its new password; a deleted
connection stays deleted. The old credential may remain, or deletion may have
completed without acknowledgement. Verify it before removing it manually.

## Upgrading and rollback

Back up configuration before upgrading. Saving an OS password reference uses
configuration version 4, which sabiql v3.0.1 and earlier cannot read. Use a
compatible binary or restore the matching backup; do not delete configuration
to resolve a version mismatch.

Legacy plaintext passwords remain readable and migrate when that connection is
edited and saved. Even renaming such a connection requires secure storage, or
clearing its password first when using alternative authentication.

## URI passwords

Percent-encode `?` and `#` in URI userinfo as `%3F` and `%23`. PostgreSQL query
password values accept a literal `?`; encode `#` as `%23`.
