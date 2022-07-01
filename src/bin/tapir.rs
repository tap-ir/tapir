//! tapir is a server that let you run different kind of binary parser (plugin of the rustruct library)
//! on data then output attributes and data to a tree, that can be accessed by a distant client trough a REST API

use std::env;
use std::fs::File;
use std::net::SocketAddr;
use std::io::{self, Read};

use tap::session::{Session};

use tapir::server::{serve, Arguments};

use log::warn;
use dotenv::dotenv;
use serde_derive::Deserialize;
use clap::{crate_authors, crate_description, crate_name, crate_version, Arg, App};

#[derive(Deserialize,Clone)]
struct Config 
{
  address : SocketAddr,
  upload : String,
  api_key : String,
}

/// We first check argument in this order if not found : command line, environment, config file, then default value 
fn usage() -> Arguments
{
  let matches = App::new(crate_name!())
    .version(crate_version!())
    .author(crate_authors!())
    .about(crate_description!())
    .arg(Arg::with_name("config")
      .short("c")
      .long("config")
      .value_name("FILE")
      .help("Custom config file")
      .takes_value(true))
    .arg(Arg::with_name("address")
      .short("a")
      .long("address")
      .value_name("ADDRESS")
      .help("Listening address & port")
      .takes_value(true))
    .arg(Arg::with_name("upload")
      .short("u")
      .long("upload")
      .value_name("UPLOAD")
      .help("Path to the upload directory")
      .takes_value(true))
    .arg(Arg::with_name("key")
      .short("k")
      .long("apikey")
      .value_name("APIKEY")
      .help("API key")
      .takes_value(true))
    .get_matches();

  let config_file = matches.value_of("config")
    .map(|s| s.to_owned())
    .or_else(|| Some(String::from("tapir.toml"))).unwrap();

  let config = File::open(config_file)
    .and_then(|mut file| 
    {
      let mut buffer = String::new();
      file.read_to_string(&mut buffer)?;
      Ok(buffer)
    })
    .and_then(|buffer| 
       toml::from_str::<Config>(&buffer)
       .map_err(|err| io::Error::new(io::ErrorKind::Other, err)))
    .map_err(|err| warn!("Can't read config file: {}", err))
    .ok();

  let address = matches.value_of("address")
    .map(|s| s.to_owned())
    .or_else(|| env::var("TAPIR_ADDRESS").ok())
    .and_then(|addr| addr.parse().ok())
    .or_else(|| config.clone().map(|config| config.address))
    .or_else(|| Some(([127, 0, 0, 1], 3583).into()))
    .unwrap();

  let upload = matches.value_of("upload")
    .map(|s| s.to_owned())
    .or_else(|| env::var("TAPIR_UPLOAD").ok())
    .or_else(|| config.clone().map(|config| config.upload))
    .or_else(|| Some(String::from("./upload"))).unwrap();

  let api_key = matches.value_of("apikey")
    .map(|s| s.to_owned())
    .or_else(|| env::var("TAPIR_APIKEY").ok())
    .or_else(|| config.clone().map(|config| config.api_key))
    .or_else(|| Some(String::from("key"))).unwrap();

  Arguments{address, upload, api_key}
}

/// register different plugins that will be available from the server
/// /!\ local let user load any file on the filesystem remotely,
/// it must be used only for test or in a sandboxed env
fn register_plugins(session :&mut Session)
{
  session.plugins_db.register(Box::new(tap_plugin_local::Plugin::new())); // /!\ dangerous if not sandboxed XXX
  session.plugins_db.register(Box::new(tap_plugin_exif::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_hash::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_s3::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_merge::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_ntfs::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_mft::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_magic::magic::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_prefetch::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_partition::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_lnk::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_evtx::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_registry::Plugin::new())); 
  session.plugins_db.register(Box::new(tap_plugin_clamav::Plugin::new()));
  #[cfg(feature = "device")]
  session.plugins_db.register(Box::new(tap_plugin_device::Plugin::new())); 
  #[cfg(feature = "yara")]
  session.plugins_db.register(Box::new(tap_plugin_yara::Plugin::new())); 
}

#[rocket::main]
async fn main() 
{
  dotenv().ok();
  pretty_env_logger::init();
  let arguments = usage();
  let mut session = Session::new();
  register_plugins(&mut session);

  if let Err(e) = serve(arguments, session).await
  { 
    eprintln!("server error : {}", e); 
  };
}
