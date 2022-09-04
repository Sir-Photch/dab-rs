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
/base set url  # sets chime of user to given url (that links to a audio-file)
/base set file  # sets chime of user to given attachment
/base clear    # clears chime of user, if present
```
### Behaviour
If some user connects to a channel, the bot will join that channel and play the chime of the user, if configured. The bot will leave after a configured timespan, if no other user joins.

When multiple users connect at the same time, their chimes will be queued and played in FCFS-order, also considering different channels in the same guild.

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

### Configuration
There needs to be a `Settings.toml` inside the directory of the executable. Consider the following template:
```toml
USERDATA_DIR = "./userdata"
API_TOKEN = "foo bar baz"
BUS_SIZE = 200
COMMAND_ROOT = "dab"
CHIME_DURATION_MAX_MS = 3000
FILE_SIZE_LIMIT_KILOBYTES = 5000
CONNECTION_TIMEOUT_MILLISECONDS = 10000
```
- `USERDATA_DIR` specifies the path where the chimes will be saved.
- `API_TOKEN` is your unique token from discord.
- `BUS_SIZE` is the queuesize for joins, globally.
- `COMMAND_ROOT` is the name of the base command. This may be reconfigured, depending on other bots in your guild(s).
- `CHIME_DURATION_MAX_MS` is the maximum duration of a users chime, in milliseconds.
- `FILE_SIZE_LIMIT_KILOBYTES` is the maximum size of a users chime on disk, in KB.
- `CONNECTION_TIMEOUT_MILLISECONDS` is the duration that the bot will remain connected to a channel, after no other user joins a channel in the guild.
