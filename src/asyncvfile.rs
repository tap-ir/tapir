use std::io::Read;

use futures::task::Context;
use rocket::http::ContentType;
use rocket::{Request, Response};
use rocket::response::{self, Responder};
use rocket::tokio::io::{self, ReadBuf, AsyncRead};
use rocket::tokio::macros::support::{Pin, Poll};

/**
 * Wrap VFile and implem AsyncRead.
 */
pub struct AsyncVFile
{
  file : Box<dyn Read + Sync + Send>,
  info : Option<(String, u64)>,
}

impl AsyncVFile
{
  pub fn new(file : Box<dyn Read + Sync + Send>, info : Option<(String , u64)>) -> AsyncVFile
  {
    AsyncVFile{ file, info }
  }
}

impl Unpin for AsyncVFile 
{
}

/// AsyncRead implem for AsyncVFile wrapper.
impl AsyncRead for AsyncVFile 
{
  #[allow(clippy::unit_arg)]
  fn poll_read(mut self: Pin<&mut Self>, _: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<io::Result<()>>
  {
    Poll::Ready(Ok(loop
    {
     let mut read_buff  = vec![0u8; buf.remaining()];
      match self.file.read(&mut read_buff) 
      {
        Ok(e) => 
        {
          buf.put_slice(&read_buff[0..e]);
          break ;
        }
        Err(ref e) if e.kind() == ::std::io::ErrorKind::Interrupted => 
        {
          continue;
        }
        Err(e) => 
        {
          return Poll::Ready(Err(e));
        }
      }
    }))
  }
}

impl<'r> Responder<'r, 'r> for AsyncVFile 
{
  fn respond_to(self, _: &'r Request<'_>) -> response::Result<'r> 
  {
    match &self.info
    {
      Some(info) => {
        Response::build()
            .header(ContentType::Binary)
            .raw_header("Content-Disposition", "attachment; filename=\"".to_owned() + &info.0+ "\"")
            .raw_header("Content-Length", info.1.to_string())
            .streamed_body(self)
            .ok()
      }
      None => {
        Response::build()
            .header(ContentType::Binary)
            .streamed_body(self)
            .ok()
      }
    }
  }
}
