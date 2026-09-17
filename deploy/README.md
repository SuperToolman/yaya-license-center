# Yaya Operation Center Deployment

`secrets/` must contain `private.pem`, `public.pem`, and `admin-password.txt`. Do not commit these files. When migrating from the legacy license center, `admin-token.txt` can be retained temporarily: the initialization script uses it as the initial `admin` password and uploads it under the new filename.

First deployment uploads the local `secrets/` directory:

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-operation-center -Initialize
```

Later deployments retain the remote secrets and SQLite Docker volume:

```powershell
.\deploy\publish.ps1 -ServerIp 203.0.113.10 -SshUser root -ContainerName yaya-operation-center
```

Use `-WebPort` and `-ApiPort` when the default ports are unavailable. On first initialization, the script creates `WEB_ORIGIN` from the server address; pass `-WebOrigin https://operation.example.com` when the management UI is behind a domain or reverse proxy. Later deployments preserve `WEB_ORIGIN` and `CARGO_REGISTRIES_CRATES_IO_INDEX` from the remote `deploy/.env`. The customer platform must use the API address, for example `http://47.112.107.44:8779`, rather than the management UI address on `8778`.

The default remote directory is `/opt/yaya-operation-center-service`, isolated from the customer delivery platform. The Docker Compose project and default container name are both `yaya-operation-center`.

When migrating an existing deployment, first back up its SQLite volume, then publish with the existing legacy container name so the named volume is retained. The entrypoint copies the legacy `license-center.sqlite3` database to `operation-center.sqlite3` before applying migrations. Legacy `LICENSE_CENTER_*` environment variables and existing JWT issuers remain supported during the transition.

Rust dependencies use the `rsproxy` sparse index by default with two retries after a 45-second network timeout. Set `CARGO_REGISTRIES_CRATES_IO_INDEX` in remote `deploy/.env` to use an internal mirror. Each release replaces the remote application source while retaining `deploy/.env`, `deploy/secrets`, and the SQLite Docker volume.
