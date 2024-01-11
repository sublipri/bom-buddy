# BOM Buddy

BOM Buddy is an Australian weather CLI utility designed for Linux environments. It might work on other operating systems, but this is untested. All data obtained belongs to the Australian Bureau of Meteorology ([copyright notice](https://reg.bom.gov.au/other/copyright.shtml)). They do not endorse or support this software, and it could stop working without warning if they change their systems.

## Features

- Periodically check the weather and cache it in a local SQLite database
- Output the current weather with customizable formatting (for use in status bars)
- View the 7-day forecast
- View the hourly forecast
- Download radar images and view radar loops in [MPV](https://mpv.io/)

### Possible future features

- Send desktop notifications when weather warnings are issued
- View past observations

## Usage

### Configuration

Run `bom-buddy --help` to display default path locations (XDG spec) and the flags to modify them. Some options in the config file can be overridden by CLI flags. See the help output of each command for more details.

### Initial setup

Run `bom-buddy init` in a terminal and follow the prompts to select your location.

### Displaying the weather

Show the current weather with `bom-buddy current`. The formatting can be modified in the config file or with the `--fstring` flag. Use `--list-keys` to show available fields.

To use in a status bar (or other external program), just capture the output of `current`. By default it only queries the database to ensure a quick response, relying on `bom-buddy monitor` to check for updates. If your status bar updates asynchronously or an occasional delay is acceptable, you can use `bom-buddy current --check` and avoid the `monitor` command. It will only perform a check when an update is due, not on every invocation of the process. See [here](https://github.com/sublipri/subar) for an example of an async status bar.

The `hourly` and `daily` commands will output their respective forecasts formatted as a table.

### Radar

View a radar loop in MPV by running `bom-buddy radar --open-mpv`. With the `--monitor` flag, it will periodically update the loop with new images.
