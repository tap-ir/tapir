#[macro_use] extern crate rocket;

pub mod server;
pub mod asyncvfile;
#[cfg(feature = "frontend")]
pub mod staticfileserver;
