# TAPIR

**TAPIR** (Trustable Artifacts Parser for Incident Response) is a multi-user, client/server, incident response framework based on the [TAP](https://github.com/tap-ir/) project. 

- It take as input  **file** (can be a disk dump or any kind of files), a **directory** containing different files (from a triage tool), a **disk dump**, or a disk **device**. Use different plugins to virtually extract data and metadata from those files, let you access them in an homogenous way via a REST API, and integrate a search engine [TAP-QUERY](https://github.com/tap-ir/tap-query) that let you create complex query to filter that data and metadata. 

- Server can be accessed remotely or locally via it's REST API, or via :

  - [TAPyR](https://github.com/tap-ir/tapyr) a python binding that can be used to create script to automate your investigation, 
  - [TAPyR-cmd](https://github.com/tap-ir/tapyr-cmd) unix like shell command.
  - [TAPIR-Frontend](https://github.com/tap-ir/tapir-frontend) a web UI.


- It's multiplateform and run on Linux, Mac OS X, and Windows.

***TAPIR is in beta and is not yet ready for production use, in this version SSL is not activated by default, and the local plugin can access any file on the server. We recommend using it on a local or private network, and to change the default API KEY on the config file or on the environment variable.***

## Download & installation 

Debian/Ubuntu package & Windows binary are available [here](https://github.com/tap-ir/tapir/releases)

To install in Debian or Ubuntu :

```
sudo dpkg -i tapir_0.1.0_amd64.deb 
```

## Documentation 

- [User documentation](https://tap-ir.github.io/) : How to interact with **TAPIR** using [TAPyR-cmd](https://github.com/tap-ir/tapyr-cmd) and [TAPIR-Frontend](https://github.com/tap-ir/tapir-frontend)
- [TAP developer documentation](https://tap-ir.github.io/docs/dev/rustdoc/tap) : The "rustdocs" rust documentation for the [TAP](https://github.com/tap-ir/tap) crate used by **TAPIR**
- [REST API documentation](https://tap-ir.github.io/docs/dev/restapi) : The REST API call description

## Building

To compile it you need to have [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) installed.

Then :

`cargo build --release`

The generated binary will be available in :

`target/release/tapir` 


## Build features

**TAPIR** build support different optional features : 

  - yara : add support for the yara plugin
  - device : add support for reading data from disk device
  - frontend : integrate the [TAPIR-Frontend](https://github.com/tap-ir/tapir-frontend) web UI inside the TAPIR binary.

To compile with feature, example with **yara** :

`cargo build --release --features=yara`

To compile with multiple features, example with **yara** and **device**

`cargo build --release --features=yara,device`


## Building with integrated frontend using [TAPIR-Workspace](https://github.com/tap-ir/tapir-ws)

[TAPIR-Workspace](https://github.com/tap-ir/tapir-ws) is a git repository that include all available [TAP](https://github.com/tap-ir/) repository as subproject. 

You will also need to have installed : [cargo](https://doc.rust-lang.org/cargo/getting-started/installation.html) & [npm](https://docs.npmjs.com/downloading-and-installing-node-js-and-npm)


```
git clone https://github.com/tap-ir/tapir-ws.git
cd tapir-ws
git submodule update --init --recursive
git submodule foreach git checkout main
cd tapir-frontend
npm install --legacy-peer-deps
npm run build
cd ..
TAPIR_FRONTEND_BUILD_PATH=$PWD/tapir-frontend/build  cargo run --release --features=frontend --bin tapir
```

The binary with the integrated frontend will be generated in `target/release/tapir`

## Building with integrated frontend 

Checkout [TAPIR-Frontend](https://github.com/tap-ir/tapir-frontend)  in an other directory : 

```
git clone https://github.com/tap-ir/tapir-frontend.git
cd tapir-frontend
npm install --legacy-peer-deps
npm run build
```

Go back to **TAPIR** directory and indicate the path to the [TAPIR-Frontend](https://github.com/tap-ir/tapir-frontend) directory in the `TAPIR_FRONTEND_BUILD_PATH` environment variable

`TAPIR_FRONTEND_BUILD_PATH=path_to_tapir_frontend cargo build --release --features=frontend`

### Generating code documentation

To generate the developer documentation run : 

`cargo doc`

Doc will be generated in `target/doc/tapir`

## Running 

### Running from binary

To run **TAPIR** the configuration file `tapir.toml` should be in the same directory as the binary is run from

### Running from **TAPIR** cloned repository

`cargo run --release`

### Running with logging information 

To display some logging information on the console the environment variable `RUST_LOG` must be set to `warn` or `info` depending of the level of information you want to be displayed.

On Linux or Mac OS X : 

`RUST_LOG=info ./tapir`

Or if running from the source with cargo 

`RUST_LOG=info cargo run --release`

### Usage

```
USAGE:
    tapir [OPTIONS]

FLAGS:
    -h, --help       Prints help information
    -V, --version    Prints version information

OPTIONS:
    -a, --address <ADDRESS>    Listening address & port
    -c, --config <FILE>        Custom config file
    -k, --apikey <APIKEY>      API key
    -u, --upload <UPLOAD>      Path to the upload directory
```


To pass argument for tapir if running with cargo you must pass them after `--` that end the cargo line of command.

`cargo run --release --features=frontend --bin tapir -- --help`


## Configuration 

You can pass the configuration for `TAPIR` with `--config` or `-c` argument.
The configuration file look like this : 

```
address = "0.0.0.0:3583"
upload = "./upload"
api_key = "key"
```

You can specifiy the addresse and port used by the server, the API key used to access the server, the directory where you want the file to be uploaded, and a directory from which file will be loaded by default. 

This variable can also be configured in the environment : 

```
TAPIR_ADDRESS : Listening address & port
TAPIR_UPLOAD : Path to the upload directory
TAPIR_APIKEY : API key
```

**TAPIR** will look first for an environment variable, then if not found for the variable in the config file, then for the default value.

The default value are :

```
config : "tapir.toml"
address : "127.0.0.1:3583"
upload : "./upload"
apikey : "key"
```

## Plugins 

**TAPIR** is part of the [TAP](https://github.com/tap-ir/) project and the file type it support is the same as the tap project. (When new parser plugin is added to [TAP](https://github.com/tap-ir/) **TAPIR** is updated to include the new plugins).

At time of writting this documentation this is the plugin included in **TAPIR**  by default or via the features flag :

| Name | Category | Description |
| ---- | -------- | ----------- |
| local |Input | Load files or directory from the filesystem |
| exif | Metadata | Extract EXIF info from file |
| hash | Metadata | Hash file attribute |
| s3 | Input | Load files from a s3 server |
| merge | Util | Merge files into one file |
| ntfs | File system | Read and parse NTFS filesystem |
| mft | File system	| Read and parse MFT file |
| magic | Metadata | Detect magic and file data compatible with plugins |
| prefetch | Windows | Parse prefetch file |
| partition | Volume | Parse MBR & GPT partition |
| lnk | Windows	| Parse lnk file |
| evtx | Windows | Parse evtx file |
| registry | Windows | Parse registry file |
| clamav | Malware | Scan file content with ClamAV | 
| device | Input | Mount a device |
| yara | Malware | Scan file content with Yara |

## Help

To discuss about the project and ask your questions join our [Discord](https://discord.gg/C8UdFG6K) server !

## License

The contents of this repository is available under GPLv3 license.
