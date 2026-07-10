# Auto-update

Vuoom updates itself from its GitHub Releases. On launch it asks the latest release
whether a newer signed build exists; if so, an **Update** pill appears in the top bar.
Clicking it downloads the signed installer, applies it, and relaunches.

## How it works

- `src-tauri/tauri.conf.json` → `plugins.updater` points at
  `https://github.com/Razee4315/Vuoom/releases/latest/download/latest.json` and pins the
  **public** signing key.
- `bundle.createUpdaterArtifacts: true` makes the build emit signed updater artifacts
  (`*.sig`) alongside the installers.
- The `Release` workflow (`.github/workflows/release.yml`) builds on every push to `main`,
  signs the bundles, and uploads `latest.json` (the update manifest) to the release — but the
  release is created as a **draft**. Drafts are excluded from `releases/latest`, so the
  updater does **not** see it. A maintainer must open the draft in **GitHub → Releases** and
  click **Publish release** to make it the latest release and ship the update to all users.
- The desktop app registers `tauri-plugin-updater` + `tauri-plugin-process`; the frontend
  (`src/App.tsx`) calls `check()` on startup and `downloadAndInstall()` + `relaunch()` when
  the user clicks Update.

## One-time setup — add the signing secrets (required)

The release build signs updates with a **private** key that must live in GitHub Actions
secrets (never in the repo). The matching public key is already committed in
`tauri.conf.json`.

The keypair was generated with `tauri signer generate` and saved **outside** the repo at:

```
C:\Users\saqla\.vuoom-keys\vuoom-updater.key       (private — keep secret)
C:\Users\saqla\.vuoom-keys\vuoom-updater.key.pub   (public — already in tauri.conf.json)
```

Add two repository secrets at
**GitHub → repo → Settings → Secrets and variables → Actions → New repository secret**:

| Secret name | Value |
| --- | --- |
| `TAURI_SIGNING_PRIVATE_KEY` | the **entire contents** of `vuoom-updater.key` |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | leave **empty** (the key was generated without a password) |

To copy the private key contents to your clipboard on Windows:

```powershell
Get-Content "$HOME\.vuoom-keys\vuoom-updater.key" -Raw | Set-Clipboard
```

> Without these secrets the release build fails to sign and `latest.json` won't be
> produced, so clients won't see updates. Keep the private key backed up — if it's lost,
> you must generate a new keypair, update the public key in `tauri.conf.json`, and ship a
> release before old clients can update again.

## Testing the flow

1. Add the secrets above.
2. Let the `Release` workflow build a version (say `v0.1.25`), **publish its draft** in
   GitHub → Releases, and install that build.
3. Push another change so the workflow drafts `v0.1.26`; **publish that draft** too.
4. Reopen the installed `v0.1.25` — the **Update** pill should appear; clicking it installs
   `v0.1.26` and relaunches.

## Rotating the key

```bash
pnpm tauri signer generate -w path/to/new.key
```

Put the new `.pub` contents into `tauri.conf.json` → `plugins.updater.pubkey`, update the
`TAURI_SIGNING_PRIVATE_KEY` secret, and ship a release.
