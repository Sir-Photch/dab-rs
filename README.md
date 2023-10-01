<p align="center">
  <img width="200" height="200" alt="dab-rs icon" src="logo.png"/>
</p>
<h1 align="center">dab-rs</h1>

### *Discord Announcement Bot,* implemented in Rust
A bot that lets users set a chime that is played every time they connect to a channel. Reminiscing that good old "*User joined your channel*" from Teamspeak-times; But this time, customizable!

## Features

### Slash-commands
After setting the name of the base command, the following commands will be available:
```
/base set url       # sets chime of user to given url (that links to an audio-file)
/base set file      # sets chime of user to given attachment
/base clear         # clears chime of user, if present
/base admin forbid  # sets role whose user's chimes are not played
```
### Behaviour
If some user connects to a channel, the bot will join that channel and play the chime of the user, if configured. The bot will leave after a configured timespan, if no other user joins.

When multiple users connect at the same time, their chimes will be queued and played in FCFS-order, also considering different channels in the same guild.

### Localization
This bot is implemented to have full support for localization. This is achieved by using [.ftl](https://projectfluent.org/) resources with [fluent-rs](https://github.com/projectfluent/fluent-rs).

## How to get started
### Compilation
Ensure that `ffmpeg` and `opus` are installed. The package names to install them on your distro my differ. For arch-based distros, the following commands will get you started:

```console
$ sudo pacman -S opus ffmpeg cargo && git clone https://github.com/Sir-Photch/dab-rs.git && cd dab-rs && cargo build --release
```

#### Cross-compile Raspberry Pi 
If you want to run the bot on a raspberry, you I suggest using the [Cross](https://github.com/cross-rs/cross) toolchain to cross-compile. You can set it up as follows:
##### Docker
If you haven't already set up docker, you can do it as follows:
```console
$ sudo pacman -S docker && sudo groupadd docker && sudo usermod -aG docker $USER
```
You may need to reboot for changes to apply. After that, verify a working docker installation with `docker run hello-world`.
##### Cross
Assuming you have the rust toolchain installed.
```console
$ cargo install cross
```
##### Compilation
Ensure that `docker.service` is running:
```console
$ systemctl start docker
```
Then, depending on your raspberry installation, run 
```console
$ cross build --release --target <target>
```
For a RPI4 with a 64-bit kernel, use `aarch64-unknown-linux-gnu`. Note that for cross-compilation, dab-rs statically links to openssl; Updates to your systems `libssl` are not incorporated unless recompiled.

##### Further reading
- [Docker post-installation](https://docs.docker.com/engine/install/linux-postinstall/)
- [Cross on github](https://github.com/cross-rs/cross)

### Database
This bot needs a database connection to run. Create a database that is to be used and specify the connection details in `Settings.toml`. You do not need to create any tables, this will be ensured at runtime.

You need to setup a postgres-database though:

```sql
create database dab_rs;
create user INSERT_USERNAME_HERE with password 'INSERT_PASSWORD_HERE';
grant all privileges on database dab_rs to INSERT_USERNAME_HERE;
alter database dab_rs owner to INSERT_USERNAME_HERE;
```

### Configuration
There needs to be a `Settings.toml` inside the directory of the executable. Consider the following template:
```toml
USERDATA_DIR = "/path/to/userdata/dir"
API_TOKEN = "foo bar baz"
BUS_SIZE = 200
COMMAND_ROOT = "dab"
CHIME_DURATION_MAX_MS = 3000
FILE_SIZE_LIMIT_KILOBYTES = 5000
CONNECTION_TIMEOUT_MILLISECONDS = 10000
RESOURCE_DIR = "/path/to/resource/dir"
DEFAULT_LOCALE = "en-US"
DB_HOSTNAME = "localhost"
DB_USERNAME = "your username"
DB_PASSWORD = "your password"
DB_NAME = "dab_rs"
LOG_PATH = "/path/to/log.file"
```
- `USERDATA_DIR` specifies the path where the chimes will be saved.
- `API_TOKEN` is your unique token from discord.
- `BUS_SIZE` is the queuesize for joins, globally.
- `COMMAND_ROOT` is the name of the base command. This may be reconfigured, depending on other bots in your guild(s).
- `CHIME_DURATION_MAX_MS` is the maximum duration of a users chime, in milliseconds.
- `FILE_SIZE_LIMIT_KILOBYTES` is the maximum size of a users chime on disk, in KB.
- `CONNECTION_TIMEOUT_MILLISECONDS` is the duration that the bot will remain connected to a channel, after no other user joins a channel in the guild, in milliseconds.
- `RESOURCE_DIR` is the path to the directory containing the folder structure for localization.
- `DEFAULT_LOCALE` is the fallback locale that is to be used when translations for a users locale are not available.
- `DB_*` are the credentials and connection details to the postgresql-database that dab-rs will use.
- `LOG_PATH` is the file where logs will be saved to.

#### Commandline options

- `-c`, `--config`: specifies path to configuration file 
- `-v`, `--verbose`: enables verbose logging in stdout
- `-b`, `--beats`: explicitly enables verbose heartbeat logging

### Localization
By default, this repository contains translations in [resources](./resources/). To be able to use them, reference this folder in the configuration for your setup. Localizations are dynamically loaded at startup, as long as the folder names obey the [Unicode Language Identifier](https://unicode.org/reports/tr35/tr35.html#Unicode_language_identifier) standards, e.g. `en-US` or `de`.

### Systemd service
Consider this unit as an example for a systemd-service. Depending on your distro, you may place it in `/etc/systemd/system`:
```console
dab-rs.service:
--------------------------------------
[Unit]
Description=dab-rs Discord Bot
Wants=network-online.target
Requires=postgresql.service
After=network-online.target postgresql.service
[Service]
WorkingDirectory=/path/to/working/directory
ExecStart=/path/to/working/directory/dab-rs
[Install]
WantedBy=multi-user.target
```
Note that you may need to modify `WorkingDirectory` and `ExecStart` based on your setup.

## Known issues

Won't work on systems with only one CPU-core.
