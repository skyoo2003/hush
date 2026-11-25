# Installing Locite with no network

Locite needs no network to index and search. The only thing it ever downloads is the embedding model, and every release also ships that model as a separate file, the models kit. With the DMG and the kit on a USB drive, you can install Locite on a Mac that never goes online.

## What to download

From the same release on <https://github.com/skyoo2003/locite/releases>, on a computer with a network:

| File | What it is |
| --- | --- |
| `Locite_X.Y.Z_aarch64.dmg` | The app (Apple silicon only). |
| `models-kit-X.Y.Z.zip` | The embedding models whose licenses allow redistribution, each with its license text, plus `manifest.json` and `SHA256SUMS`. About 620 MB. |
| `SHA256SUMS` | Checksums of every file in the release. |

With the GitHub CLI:

```sh
gh release download vX.Y.Z --repo skyoo2003/locite \
  --pattern '*.dmg' --pattern 'models-kit-*.zip' --pattern SHA256SUMS
```

## Check the files

Run this in the folder that holds the three files, once after downloading and again on the offline Mac after copying them from the USB drive:

```sh
shasum -a 256 -c SHA256SUMS --ignore-missing
```

Each file you have must print `OK`. `--ignore-missing` skips the release files you did not download. If a line prints `FAILED`, download that file again and do not install it.

On the connected computer you can also check that the release workflow built the files (needs the GitHub CLI and a network):

```sh
gh attestation verify models-kit-X.Y.Z.zip --repo skyoo2003/locite
gh attestation verify Locite_X.Y.Z_aarch64.dmg --repo skyoo2003/locite
```

The kit has its own checksums of its contents, if you want to inspect it unpacked:

```sh
unzip models-kit-X.Y.Z.zip
cd models-kit-X.Y.Z && shasum -a 256 -c SHA256SUMS
```

You do not need to unpack it to install it.

## Install from a USB drive

1. Copy the three files from the USB drive to a folder on the offline Mac, for example `~/Downloads`, and run the checksum command above.
2. Open the DMG and drag Locite to Applications.
3. Open Locite. Releases are not notarized, so macOS blocks the first launch. Open **System Settings > Privacy & Security**, click **Open Anyway**, and confirm.
4. Air-Gap mode is on by default, and the app reports that no embedding model is installed. In **Security > Models**, click **Import a model file or kit…** and pick `models-kit-X.Y.Z.zip`.
5. Locite reads each model file in the kit, checks its SHA-256 against the values built into the app, and installs only the files that match. It does not trust the kit's own `manifest.json` or `SHA256SUMS`. When it finishes it shows "bge-small-en-v1.5 is installed." for each model.
6. Add a folder. Indexing and search start right away.

If the import reports that the file does not match the expected checksum, the kit is damaged: copy it again and recheck it. If it reports that the file is not one of the models Locite knows, the kit comes from a release whose model catalog differs from your app's; use the kit from the same release as the DMG.

Importing a kit twice is harmless; it checks and reinstalls the same files. You can still import single model files instead, as the Models section lists them.

## Updating with no network

**Check for updates** needs a network, so an offline Mac is updated by hand:

1. On the connected computer, download the new DMG and `SHA256SUMS` of the new release, and its models kit if the release notes say the models changed. Check them with `shasum -a 256 -c SHA256SUMS --ignore-missing`.
2. On the offline Mac, check them again, quit Locite, open the new DMG, and drag Locite to Applications, replacing the old copy. Your database, settings, and installed models are kept.
3. Open Locite. If the Models section reports a model as missing, import the new kit.

Every release is signed with the same certificate, so the login keychain should keep trusting the app. If it asks for your login password to release the database key, enter it once.

## Licenses

Only models marked `redistributable` in `models/catalog.json` go in the kit. Each model folder in it holds `LICENSE.txt`, copied from `models/licenses/` in this repository: the model's source and revision, and the license text. `manifest.json` lists each model's id, kind, license, license file, and the name, size, and SHA-256 of each file.

## Verifying an offline install

Before a release is published, a maintainer checks the offline path on a Mac:

1. Create a new standard user account, so no database, keychain item, or model exists yet, and log in to it.
2. Turn Wi-Fi off and unplug Ethernet.
3. Copy the DMG, the kit, and `SHA256SUMS` from a USB drive; follow "Install from a USB drive" above.
4. Add a folder with a few text and PDF files, wait for indexing to finish, and search for a word in one of them and for a phrase that only matches its meaning. Both must return that file.

A test runs the same import, indexing, and search with the network denied by the macOS sandbox, from the kit alone:

```sh
sh scripts/make-models-kit.sh 0.0.0 /tmp/kit
cd src-tauri && cargo test --lib --no-run
bin=$(ls -t target/debug/deps/locite_lib-* | grep -v '\.d$' | head -1)
sandbox-exec -p '(version 1)(allow default)(deny network*)' \
  env LOCITE_MODELS_KIT=/tmp/kit/models-kit-0.0.0.zip \
  "$bin" --ignored a_real_models_kit_alone_is_enough_to_index_and_search
```
