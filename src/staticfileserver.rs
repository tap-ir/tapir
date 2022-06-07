use std::io::Cursor;
use rocket::{Request, Data, Response};
use rocket::http::{Method, uri::Segments};
use rocket::route::{Route, Handler, Outcome};
use rocket::response::{self, Responder};
use rocket::http::uri::fmt::Path;
use include_dir::{include_dir, Dir};

static FRONTEND_DIR : Dir = include_dir!("$CARGO_MANIFEST_DIR/../tapir-frontend/build"); //XXX pass as cargo argument

#[derive(Debug, Clone)]
pub struct StaticFileServer
{
  rank: isize,
}

impl StaticFileServer
{
  const DEFAULT_RANK: isize = 10;

  pub fn from() -> Self
  {
    StaticFileServer::new()
  }

  pub fn new() -> Self
  {
    StaticFileServer {  rank: Self::DEFAULT_RANK }
  }

  pub fn rank(mut self, rank: isize) -> Self 
  {
    self.rank = rank;
    self
  }
}

impl Default for StaticFileServer
{
  fn default() -> Self 
  {
    Self::new()
  }
}

impl From<StaticFileServer> for Vec<Route> 
{
  fn from(server: StaticFileServer) -> Self 
  {
    let mut route = Route::ranked(server.rank, Method::Get, "/<path..>", server);
    route.name = Some("frontend".into());
    vec![route]
  }
}

#[rocket::async_trait]
impl Handler for StaticFileServer
{
  async fn handle<'r>(&self, req: &'r Request<'_>, data: Data<'r>) -> Outcome<'r> 
  {
    let path = req.segments::<Segments<'_, Path>>(0..).unwrap().to_path_buf(false).ok();

    let file = match path 
    {
      Some(p) if p.to_str().unwrap() == ""  => FRONTEND_DIR.get_file("index.html"),
      Some(p)  => FRONTEND_DIR.get_file(p),
      None => return Outcome::forward(data),
    };

    match file
    {
      Some(file) => Outcome::from_or_forward(req, data, StaticFile::new(file.contents())),
      None => Outcome::forward(data),
    }
  }
}

pub struct StaticFile
{
  data : &'static [u8],
}

impl StaticFile
{
  pub fn new(data : &'static [u8]) -> Self
  {
    StaticFile{ data }
  }
}

impl<'r> Responder<'r, 'r> for StaticFile 
{
  fn respond_to(self, _: &'r Request<'_>) -> response::Result<'r> 
  {
     Response::build()
       .sized_body(self.data.len(), Cursor::new(self.data))
       .ok()
  }
}
