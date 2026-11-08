# work-http-component

Remote-access control UI and action contract, composed with a node's existing
owner. The custom layout, state presentation and request validation live here;
the consuming application only binds `RemoteAccessOwner` and mounts a generic
component panel.

`register()` supplies the same UI bundle used by downloaded packages. The status
snapshot excludes passwords, authorization tickets and model credentials. Read
status is available to authenticated remote clients. Changing sharing policy
requires the local owner, enforced by the host capability registry.

Export a distributable UI bundle:

```sh
cargo run -p work-http-component --bin export-component > component.json
```

This is a private Work UI package. Work binds it to its existing local owner;
it is independent of the rpi HTTP plugin and never starts that plugin as a node
host. Local calls use the same in-process owner as the public Work API. Remote
clients can read status but cannot change host policy or issue owner grants.

The dedicated panel also edits listener IP/port, public origin and a new node
password. Configuration is staged for next startup and never restarts the live
owner. The node stores only the password verifier; read status contains no
password material. Form actions use the component bridge without granting
sandboxed form navigation or direct network access to the document.

The generic Bundle format can describe native artifacts for future consumers.
Work's current component page installs UI and explicit capability grants only;
it does not download or execute a native node host.
