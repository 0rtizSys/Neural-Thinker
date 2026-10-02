# Open-source core vs. paid services

Neural-Thinker is split into two parts that never mix:

| Part | Where | Published | License |
|---|---|---|---|
| Community edition (editor, navigation, graph views, customization) | this repository | yes | PolyForm Noncommercial 1.0.0 |
| Paid services (cloud storage, phone-computer sync for Windows, macOS, Linux, Android and iOS) | `src/private/` | **never** | proprietary, all rights reserved |

## How the split works

- **Dependency direction.** The public crate is a library (`neural_thinker`) plus the
  Community binary. The paid services are a separate crate in `src/private/` that
  depends on the public library and passes its services to `neural_thinker::run`.
  The public crate never refers to the private one, so a fresh clone builds and
  runs without it.
- **Extension seam.** `src/services.rs` defines the `Service` trait (name, status,
  settings UI, per-frame update, file events such as save, rename and delete) and
  the `Services` collection. The Community binary passes `Services::community()`,
  which is empty.
- **Ignored folder.** `/src/private/` is in `.gitignore`. `Cargo.toml` also
  excludes it from the package and sets `publish = false`.

## Guards

`scripts/check-private.sh` fails when:

1. `src/private/` is no longer ignored;
2. any path under `src/private/` is tracked or staged (including `git add -f`);
3. any tracked file contains the marker `NT-PRIVATE-DO-NOT-PUBLISH`, which every
   private file carries on its first line, so a private file copied elsewhere in
   the tree is caught too;
4. public code refers to the private crate (`mod private`, a `path = "...private"`
   dependency, or the private crate's name);
5. with `--history`: any commit reachable from any ref ever contained a private path
   or the marker.

It runs in three places:

- **pre-commit hook** (`.githooks/pre-commit`): checks 1 to 4 before every commit;
- **pre-push hook** (`.githooks/pre-push`): checks 1 to 5 before every push;
- **CI** (`.github/workflows/ci.yml`): checks 1 to 5 on every push and pull request,
  and builds and tests the Community edition on Windows and Linux without the
  private folder.

`build.rs` enables the hooks automatically (`git config core.hooksPath .githooks`)
the first time the project is built in a checkout where no hooks path is set. To
enable them by hand:

```sh
git config core.hooksPath .githooks
```

Never bypass them with `git commit --no-verify` or `git push --no-verify`.

## Working on paid services

The private crate is laid out like this:

```text
src/private/
├── Cargo.toml        # package "neural-thinker-pro", [workspace], depends on ../..
└── src/
    ├── main.rs       # builds Services::new("Pro", vec![...]) and calls neural_thinker::run
    ├── cloud.rs      # impl Service for the cloud storage client
    └── sync.rs       # impl Service for cross-device sync
```

Build and run it from that folder:

```sh
cd src/private
cargo run --release
```

Keep it in its own private repository (or a private backup), never in this one.
If a private file is ever committed by mistake, do not push: remove it with
`git rm -r --cached src/private` and rewrite the unpushed commits. If it was
already pushed, the history must be rewritten (for example with `git filter-repo`)
and any secrets in it rotated.
