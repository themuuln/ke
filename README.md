# ke — macOS Keychain secrets manager

A fast, minimal TUI + CLI for managing project secrets via the macOS Keychain.

```
ke                 # interactive TUI
ke ls              # list all projects
ke ls personal     # list keys for a project
ke get zer DB_URL  # print a secret
ke cp zer DB_URL   # copy to clipboard
ke run zer -- next dev  # run command with secrets as env vars
```

## Features

- **macOS Keychain** — secrets encrypted at rest, protected by Touch ID / login password
- **Dual interface** — full-featured TUI (Ratatui) + complete CLI
- **iCloud sync** — key names sync across Macs via iCloud Drive; secret values stay local per machine
- **Project organization** — group secrets by project; browse, add, delete, copy
- **No config files** — no `.env` files to manage (unless you want them)
- **Drop-in replacement** — backward compatible with bash version's data format

## Install

Build from source:

```bash
cargo install --git https://github.com/themuuln/ke
```

Or download a binary from the [releases page](https://github.com/themuuln/ke/releases).

## Usage

### CLI

```
ke set <project> <key> [value]   Store a secret (prompts if no value)
ke get <project> <key>           Print a secret to stdout
ke ls [project]                  List projects or keys in a project
ke cat <project>                 Print all secrets in .env format
ke cp <project> <key>            Copy a secret to clipboard
ke delete <project> <key>        Remove a single secret
ke pull <project>                Write .env.local from Keychain
ke push <project> [file]         Read .env file into Keychain
ke rm <project>                  Remove all secrets for a project
ke run <project> -- <cmd>        Run a command with secrets as env vars

ke status                        Show sync status and missing values
ke init --icloud                 Enable iCloud Drive sync for key names
```

### TUI

Run `ke` with no arguments. Keyboard shortcuts:

| Key | Action |
|-----|--------|
| `↑` `↓` `j` `k` | Navigate |
| `Tab` | Switch between project list and key list |
| `Enter` | Select / copy |
| `a` | Add a secret |
| `d` | Delete selected key |
| `c` | Copy to clipboard |
| `q` / `Esc` | Quit |
| `D` | Delete entire project |

## iCloud Sync

Key names sync across Macs; secret values stay local per machine.

```bash
ke init --icloud     # set up sync (only needed once per machine)
ke status            # see which values are missing on this machine
ke set project key value  # set missing values on each machine
```

## Architecture

```
ke/
├── src/
│   ├── main.rs      Entry point, CLI dispatch
│   ├── app.rs       TUI state machine (Ratatui)
│   ├── config.rs    Project index management, iCloud sync
│   ├── keychain.rs  macOS `security` CLI wrapper
│   └── ui.rs        TUI rendering (3-pane layout, modals)
```

Key names are stored as flat files in `~/.config/ke/`. Secret values live in the
macOS Keychain under the service prefix `keychain-env-*`.

## Limitations

- **macOS only** — relies on the `security` CLI
- **Local values only** — each machine's Keychain is independent; only key names sync via iCloud
