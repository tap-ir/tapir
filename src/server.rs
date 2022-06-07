use std::error::Error;
use std::path::PathBuf;
use std::net::SocketAddr;
use std::sync::Arc;
use std::collections::HashMap;
use std::io::Read;
use std::io::SeekFrom;
use std::io::Write;

use tap::session::Session;
use tap::tree::TreeNodeId;
use tap::task_scheduler::{TaskId,TaskState};
use tap::node::Node;
use ::tap_save::Save;
use ::tap_query::filter::Filter;
use ::tap_query::timeline as query_timeline;
use ::tap_query::attribute::attribute_count as query_attribute_count;
use tap_plugin_magic::{datatypes, plugins_datatype};

use crate::asyncvfile::AsyncVFile;
#[cfg(feature = "frontend")]
use crate::staticfileserver::StaticFileServer;
#[cfg(feature = "frontend")]
use webbrowser;

use log::info;
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use json_value_merge::Merge;
use serde::ser::{SerializeSeq, Serializer};

use rocket::State;
use rocket::shield::Shield;
use rocket::config::Config;
#[cfg(feature = "tls")]
use rocket::config::TlsConfig;
use rocket::{Request, Response};
use rocket::http::{self, Status, Header};
use rocket::response::status::{BadRequest, Custom};
use rocket::serde::json::{Json,json,Value};
use rocket::fairing::{Fairing, Info, Kind};
use rocket::data::{Data, Limits, ToByteUnit};
#[cfg(feature = "frontend-dev")]
use rocket::fs::FileServer;
use rocket::request::{Outcome, FromRequest};

macro_rules! spawn_thread
{
  ($closure:expr) => 
  {
    rocket::tokio::task::spawn_blocking(move || {
      $closure
    }).await.unwrap()
  };
}

#[derive(Clone)]
pub struct Arguments
{
  pub address : SocketAddr,
  pub preload : Option<String>,
  pub upload : String,
  pub plugins_types : HashMap<String, Vec<String>>,
  pub api_key : String,
}

pub type ArcSession = Arc<Session>;

#[derive(Serialize)]
pub struct PluginInfo
{
  pub name : String,
  pub category : String,
  pub description : String,
  pub config : Option<Value>,
}

///Return list of available plugins. 
#[get("/plugins")]
async fn plugins(_key : ApiKey<'_>, session : &State<ArcSession>) -> Json<Vec<PluginInfo>> 
{
  let session = session.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {
  let plugins : Vec<PluginInfo> = session.plugins_db
                                         .iter()
                                         .map(|plugin| PluginInfo{ name : plugin.name().into(),
                                              category : plugin.category().into(),
                                              description : plugin.help().into(),
                                              config : serde_json::from_str(&plugin.config().unwrap()).unwrap(), })
                                         .collect();
  Json(plugins)
  }).await.unwrap()
}


///Return plugin configuration schema.
#[get("/plugin/<plugin_name>", format = "json")] 
async fn plugin(_key : ApiKey<'_>, session : &State<ArcSession>, plugin_name : String) -> Json<Option<PluginInfo>>
{
  let session = session.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {
  let plugin_info = session.plugins_db.find(&plugin_name).map(|plugin| PluginInfo{
     name : plugin.name().into(),
     category : plugin.category().into(),
     description : plugin.help().into(),
     config : serde_json::from_str(&plugin.config().ok().unwrap()).unwrap(), 
  });
  
  Json(plugin_info)
  }).await.unwrap()
}

#[derive(Deserialize)]
pub struct NodeIdOption
{
  pub node_id : TreeNodeId,
  pub option : NodeOption,
}

impl NodeIdOption
{
  fn to_json(&self, session : &Session) -> Result<Value, BadRequest<String>>
  {
    node_option_to_json(session, &self.node_id, &self.option)
  }
}

#[derive(Deserialize)]
pub struct NodesIdOption 
{
  pub nodes_id : Vec<TreeNodeId>,
  pub option : NodeOption,
}

impl NodesIdOption
{
  fn to_json<W>(&self, session : &Session, writer : W) -> W 
    where W: Write, 
  {
    let mut ser = serde_json::Serializer::new(writer);   
    let mut seq = ser.serialize_seq(None).unwrap();
    for node_id in self.nodes_id.iter()
    {
      seq.serialize_element(&node_option_to_json(session, node_id, &self.option).unwrap()).unwrap(); 
    }
     seq.end().unwrap(); 
     ser.into_inner()
  }
}

#[derive(Deserialize)]
pub struct NodeOption
{
  pub name : bool,
  pub path : bool,
  pub attributes : bool,
  pub children : bool,
}

fn node_option_to_json(session : &Session, node_id : &TreeNodeId, option : &NodeOption) -> Result<Value, BadRequest<String>>
{
  let node = match session.tree.get_node_from_id(*node_id)
  {
    Some(node) => node,
    None => return Err(BadRequest(Some("Node didn't exist".to_string()))),
  };
  let name = match option.name
  {
    true => Some(node.name()),
    false => None,
  };
  let path = match option.path
  {
    true => Some(session.tree.node_path(*node_id)),
    false => None,
  };
  let attributes : Option<&Node> = match option.attributes
  {
    true => Some(&node),
    false => None,
  };
  let children = match option.children
  {
    true => Some(session.tree.children_id_name(*node_id)),
    false => None,
  };
  
  let has_children = session.tree.has_children(*node_id); 

  Ok(json!({"id" : node_id, "name" : name, "path" : path, "attributes" : attributes , 
           "children" : children, "has_children" : has_children }))
}


///Return a node from a node id.
#[post("/node", data = "<node_option>", format = "json")]
async fn node(_key : ApiKey<'_>, session : &State<ArcSession>, node_option : Json<NodeIdOption>) -> Result<Value, BadRequest<String>>
{
  let session = session.inner().clone();
  spawn_thread!(node_option.to_json(&session))
}

///Return root node.
#[get("/root")]
async fn root(_key : ApiKey<'_>,  session : &State<ArcSession>) -> Result<Value, BadRequest<String>>
{
  let session = session.inner().clone();

  rocket::tokio::task::spawn_blocking(move || {
  let node_id = session.tree.root_id;
  let option = NodeOption{ name : true, path : false, attributes : true, children : true};
  node_option_to_json(&session, &node_id, &option)
  }).await.unwrap()
}

///Return a node from a path.
#[get("/root/<path..>")]
async fn node_by_path(_key : ApiKey<'_>, session : &State<ArcSession>, path : PathBuf) -> Result<Value, BadRequest<String>>
{
  let session = session.inner().clone();

  rocket::tokio::task::spawn_blocking(move || {
  let path = match path.to_str()
  {
    Some(path) => path,
    None => return Err(BadRequest(Some("Can't convert path to str".into()))),
  };

  let path = path.replace('\\', "/"); //temp fix for windows
  let node_id = match session.tree.get_node_id(&("/root/".to_owned() + &path))
  {
    Some(node_id) => node_id,
    None => return Err(BadRequest(Some("Node id not found".into()))),
  };
  
  let option = NodeOption{ name : true, path : false, attributes : true, children : true};
  node_option_to_json(&session, &node_id, &option)
  }).await.unwrap()
}

///Return a list of nodes attributes from a vector of node_id.
#[post("/nodes", data = "<request>", format = "json")]
async fn nodes(_key : ApiKey<'_>, session : &State<ArcSession>, request: Json<NodesIdOption>) -> Vec<u8> 
{
  let session = session.inner().clone();

  rocket::tokio::task::spawn_blocking(move || {

  let writer = Vec::new();
  request.to_json(&session, writer)

  }).await.unwrap()
}

///Remove node and descendants.
#[post("/delete", data="<node_id>")]
async fn delete(_key : ApiKey<'_>, session : &State<ArcSession>, node_id : Json<TreeNodeId>)
{
  let node_id : TreeNodeId = *node_id;

  let session = session.inner().clone();
  spawn_thread!(session.tree.remove(node_id))
}

/*#[get("/clear")]
async fn clear(_key : ApiKey<'_>, session : &State<ArcSession>)
{
  let session = session.inner().clone();
  spawn_thread!(session.write().unwrap().clear());
}*/

///Return a path from a node id.
#[post("/path", data = "<node_id>")]
async fn path(_key : ApiKey<'_>, session : &State<ArcSession>, node_id : Json<TreeNodeId>) -> Result<String, BadRequest<String>>
{
  let node_id : TreeNodeId = *node_id;

  let session = session.inner().clone();
  spawn_thread!(
    match session.tree.node_path(node_id)
    {
      Some(path) => Ok(path),
      None => Err(BadRequest(Some("Can't find node".into()))),
    })
}

///Return parent id from node id.
#[post("/parent_id", data = "<node_id>", format = "json")]
async fn parent_id(_key : ApiKey<'_>, session : &State<ArcSession>, node_id : Json<TreeNodeId>) -> Json<Option<TreeNodeId>>
{
  let node_id : TreeNodeId = *node_id;

  let session = session.inner().clone();
  spawn_thread!(Json(session.tree.parent_id(node_id)))
}

#[derive(Deserialize)]
struct PluginArgs
{
  name : String,
  arguments : String,
  relaunch : bool, 
}

///Run a task and block until task end and return task result.
#[post("/run", data = "<plugin>", format = "json")]
async fn run(_key : ApiKey<'_>, session : &State<ArcSession>, plugin : Json<PluginArgs>) -> Value
{
  info!("run : {} {}", plugin.name, plugin.arguments);
  let session = session.inner().clone();
  
  let result = spawn_thread!(session.run(&plugin.name, plugin.arguments.clone(), plugin.relaunch));

  match result 
  {
    Ok(result) => json!({"result" : result}), 
    Err(err) => json!({"error" : err.to_string()}), 
  }
}

///Schedule a task to be run on the server and return the created task state id or an error.
#[post("/schedule", data = "<plugin>", format = "json")] 
async fn schedule(_key : ApiKey<'_>, session : &State<ArcSession>, plugin : Json<PluginArgs>) -> Result<Json<TaskId>, BadRequest<String>>
{
  info!("Scheduling : {} {}", plugin.name, plugin.arguments);
  
  let session = session.inner().clone();
  
  let result = spawn_thread!(session.schedule(&plugin.name, plugin.arguments.clone(), plugin.relaunch));
  info!("Result : {:?}", result);

  match result
  {
    Ok(id) => Ok(Json(id)), 
    Err(err) => Err(BadRequest(Some(err.to_string()))),
  }
}


/// Wait that all tasks are finished.
#[post("/join")]
async fn join(_key : ApiKey<'_>, session : &State<ArcSession>)
{
  info!("joining on task");
  let session = session.inner().clone();
  spawn_thread!(session.join());
}

/// Return the coutn of task.
#[post("/task_count")]
async fn task_count(_key : ApiKey<'_>, session : &State<ArcSession>) -> Value
{
  let session = session.inner().clone();
  spawn_thread!(json!(session.task_scheduler.task_count()))
}

#[derive(Deserialize)]
pub struct TasksParameters
{
  ids : Vec<u32>,
}

/// Return task state and task info.
#[post("/tasks", data="<parameters>", format = "json")] 
async fn tasks(_key : ApiKey<'_>, session : &State<ArcSession>, parameters : Json<TasksParameters>) -> Json<Vec<Value>> 
{
  let session = session.inner().clone();
  let parameters = parameters.into_inner();

  rocket::tokio::task::spawn_blocking(move || {

  let tasks_state = session.task_scheduler.tasks(parameters.ids);
  let mut response = Vec::new();

  for task_state in tasks_state
  {
    let (task, result) = match task_state
    {
      TaskState::Waiting(task) => { response.push(json!({"state" : "waiting",  "id" : task.id, "plugin": task.plugin_name, "argument" : serde_json::from_str::<Value>(&task.argument).unwrap() })); break; },
      TaskState::Launched(task) => { response.push(json!({"state" : "running", "id" : task.id, "plugin": task.plugin_name, "argument" : serde_json::from_str::<Value>(&task.argument).unwrap() })); break; },
      TaskState::Finished(task, result) => (task, result),
    };

    let result = match result
    {
      Ok(result) => json!({"result" : result, "state" : "finished", "id" : task.id, "plugin": task.plugin_name, "argument" : serde_json::from_str::<Value>(&task.argument).unwrap() }), 
      Err(err) => json!({"error" : err.to_string(), "state" : "finished", "id" : task.id, "plugin": task.plugin_name, "argument" : serde_json::from_str::<Value>(&task.argument).unwrap() }),
    };
    response.push(result);
  }  
  Json(response)

  }).await.unwrap()
}

/// Return task state and task info.
#[post("/task?<task_id>")] 
async fn task(_key : ApiKey<'_>, session : &State<ArcSession>, task_id : u32) -> Option<Value> 
{
  let session = session.inner().clone();
  let task_state = spawn_thread!(session.task_scheduler.task(task_id))?;

  let (task, result) = match task_state
  {
    TaskState::Waiting(task) => return Some(json!({"state" : "waiting", "task" : task})),
    TaskState::Launched(task) => return Some(json!({"state" : "running", "task" : task})),
    TaskState::Finished(task, result) => (task, result),
  };

  match result
  {
    Ok(result) => Some(json!({"result" : result, "state" : "finished", "task" : task })), 
    Err(err) => Some(json!({"error" : err.to_string(), "state" : "finished", "task" : task})), 
  }
}

#[derive(Deserialize, Debug)]
struct AttributeInfo
{
  node_id : TreeNodeId,
  name : String,    
  //value : Value, 
  value : tap::value::Value, 
  description : Option<String>,
}


/// Add an attribute to a node (don't support dotted notation yet).
#[post("/attribute", data = "<attribute>", format = "json")]
async fn attribute(_key : ApiKey<'_>, session : &State<ArcSession>, attribute : Json<AttributeInfo>) -> Option<()> 
{
  let session = session.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {
  let node = session.tree.get_node_from_id(attribute.node_id)?; 

  match &attribute.description
  {
    Some(descr) => node.value().add_attribute(attribute.name.clone(), attribute.value.clone(), Some(descr.clone())),
    None => node.value().add_attribute(attribute.name.clone(), attribute.value.clone(), None),
  }
  Some(())
  }).await.unwrap()

}

#[derive(Deserialize, Debug)]
pub struct QueryInfo 
{
  pub query : String,
  pub root : String, 
}

/// Execute a query and return a node list.
#[post("/query", data = "<query_info>", format = "json")] 
async fn query(_key : ApiKey<'_>, session : &State<ArcSession>, query_info : Json<QueryInfo>) -> Result<Json<Vec<TreeNodeId>>, BadRequest<String>>
{
  //XXX use filter_path or  filter_nodes
  //XXX check if path is "" or "/" -> search for "/root"
  let session = session.inner().clone();

  rocket::tokio::task::spawn_blocking(move || {
  let path  = &query_info.root;
  let query = &query_info.query;
  info!("executing query {} on {}", query, path);

  match Filter::path(&session.tree, query, path)
  {
    Ok(res) => Ok(Json(res)),
    Err(err) => Err(BadRequest(Some(err.to_string()))),
  }
  }).await.unwrap()
}


/// Scan server and execute plugin, this is deprecated.
#[post("/scan")]
async fn scan(_key : ApiKey<'_>, session : &State<ArcSession>, plugins_types : &State<HashMap<String, Vec<String>>>) -> Json<Vec<(TreeNodeId, String)>>
{
  let session = session.inner().clone();
  let plugins_types = plugins_types.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {

  let _nodes = datatypes(&session.tree);
  Json(plugins_datatype(&session.tree, &plugins_types))
  }).await.unwrap()
}


/// Upload a file to the server to be processed later.
#[post("/upload?<name>", data = "<data>")]
async fn upload(_key : ApiKey<'_>, upload_dir : &State<String>, name : String, data : Data<'_>) -> Result<Json<u64>, BadRequest<String>>
{
  let file_path = upload_dir.to_string() + "/" + &name;
  info!("Uploading file to : {}", file_path);

  let file = data.open(4096.gibibytes()).into_file(file_path).await;
  let file = match file
  {
    Ok(file) => file,
    Err(err) => return Err(BadRequest(Some(err.to_string()))),
  };
  if !file.is_complete() 
  {
    return Err(BadRequest(Some("File uploaded is not complete".to_string())));
  }

  Ok(Json(file.n.written as u64))
}

/// Download a node data attribute.
#[post("/download", data = "<node_id>", format = "json")]
async fn download(_key : ApiKey<'_>, session : &State<ArcSession>, node_id : Json<TreeNodeId>) -> Result<AsyncVFile, BadRequest<String>>
{
  let session = session.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {

  let node = session.tree.get_node_from_id(*node_id).ok_or_else(|| BadRequest(Some("Invalid NodeId".to_string())))?;
  let attr = node.value().get_value("data").ok_or_else(|| BadRequest(Some("No data attribute on node".to_string())))?;
  let builder = attr.as_vfile_builder();
  let file = builder.open().map_err(|err| BadRequest(Some(err.to_string())))?;
  let stream = AsyncVFile::new(Box::new(file), Some((node.name(), builder.size())));

  Ok(stream) 
  }).await.unwrap()
}

#[derive(Debug, PartialEq, FromForm)]
struct FromNodeId 
{
  index1: usize,
  stamp: usize,
}

#[get("/download_id?<apikey>&<node_id>")] 
async fn download_id(session : &State<ArcSession>, api_key : &State<ConfigApiKey>, apikey : &'_ str, node_id : FromNodeId) -> Result<AsyncVFile, Custom<String>>
{
  if api_key.inner().0 != apikey
  {
    return Err(Custom(Status::Unauthorized, "Invalid API key".into()))
  }

  let session = session.inner().clone();
  rocket::tokio::task::spawn_blocking(move || {

  let node_id_str = json!({"index1":  node_id.index1, "stamp" : node_id.stamp}).to_string();
  let node_id : TreeNodeId = serde_json::from_str(&node_id_str).unwrap();
  let node = session.tree.get_node_from_id(node_id).ok_or_else(|| Custom(Status::BadRequest, "Invalid NodeId".into()))?;
  let attr = node.value().get_value("data").ok_or_else(|| Custom(Status::BadRequest, "No data attribute on node".into()))?;
  let builder = attr.as_vfile_builder();
  let file = builder.open().map_err(|err| Custom(Status::BadRequest, err.to_string()))?;
  let stream = AsyncVFile::new(Box::new(file), Some((node.name(), builder.size())));
  
  Ok(stream) 
  }).await.unwrap()
}

#[derive(Deserialize)]
pub struct ReadInfo 
{
  pub node_id : TreeNodeId,
  pub offset : u64,
  pub size : u64, 
}

/// Read from a node data attribute.
#[post("/read", data = "<data>", format = "json")]
async fn read(_key : ApiKey<'_>,  session : &State<ArcSession>, data : Json<ReadInfo>) -> Result<AsyncVFile, BadRequest<String>>
{
  let node = session.tree.get_node_from_id(data.node_id).ok_or_else(|| BadRequest(Some("Invalid NodeId".to_string())))?;

  let attr = node.value().get_value("data").ok_or_else(|| BadRequest(Some("No data attribute on node".to_string())))?;
  let builder = attr.as_vfile_builder();
  let mut file = builder.open().map_err(|err| BadRequest(Some(err.to_string())))?;

  if data.offset != 0 {
    file.seek(SeekFrom::Start(data.offset)).map_err(|err| BadRequest(Some(err.to_string())))?;  }

  let handler = file.take(data.size);
  
  Ok(AsyncVFile::new(Box::new(handler), None))
}

#[derive(Deserialize)]
struct SaveFile
{
  pub file_name : String,
}

/// Save server task list.
#[post("/save", data = "<data>", format = "json")]
async fn save(_key : ApiKey<'_>,  session : &State<ArcSession>, data : Json<SaveFile>) -> Option<()> 
{
  let session = session.inner().clone();
  let saver = Save::Replay;
  
  spawn_thread!(saver.to_file(data.file_name.clone(), &session));
  Some(())
}

/// Load server task list.
#[post("/load", data = "<data>", format = "json")]
async fn load(_key : ApiKey<'_>,  session : &State<ArcSession>, data : Json<SaveFile>) -> Option<()> 
{
  let session = session.inner().clone();
  let loader = Save::Replay;

  spawn_thread!(loader.from_file(data.file_name.clone(), &session));
  Some(())
}

/// Return total node count in the tree.
#[get("/node_count")]
async fn node_count(_key : ApiKey<'_>,  session : &State<ArcSession>) -> Json<usize>
{
  let session = session.inner().clone();

  spawn_thread!(Json(session.tree.count()))
}

/// Return total attribute count in the tree.
#[get("/attribute_count")]
async fn attribute_count(_key : ApiKey<'_>,   session : &State<ArcSession>) -> Json<u64>
{
  let session = session.inner().clone();

  spawn_thread!(Json(query_attribute_count(&session.tree)))
}

#[derive(Deserialize)]
pub struct TimeRange 
{
  after : String,
  before : String,
  option : Option<NodeOption>,
}

/// Create a timeline from the attributes.
#[post("/timeline", data = "<time_range>", format = "json")]
async fn timeline(_key : ApiKey<'_>,  session : &State<ArcSession>, time_range : Json<TimeRange>) -> Result<Vec<u8>, BadRequest<String>>
{
  let after = match DateTime::parse_from_rfc3339(&time_range.after)
  {
    Ok(after) => after.with_timezone(&Utc),
    Err(err) => return Err(BadRequest(Some(err.to_string()))),
  };

  let before = match DateTime::parse_from_rfc3339(&time_range.before)
  {
    Ok(before) => before.with_timezone(&Utc),
    Err(err) => return Err(BadRequest(Some(err.to_string()))),
  };

  let session = session.inner().clone();

  rocket::tokio::task::spawn_blocking(move || {
    let time_infos = query_timeline::Timeline::tree(&session.tree, &after, &before);
  
    let mut ser = serde_json::Serializer::new(Vec::new());   
    let mut seq = ser.serialize_seq(None).unwrap();
    for time_info in time_infos
    {
      let mut time_info_json = json!({"id" : time_info.id, 
                                      "attribute_name" : time_info.attribute_name, 
                                       "time" :  time_info.time});

      if let Some(option) = &time_range.option
      {
         let option_json = node_option_to_json(&session, &time_info.id, option)?;
         time_info_json.merge(option_json);
      }

      seq.serialize_element(&time_info_json).unwrap();
    }
    seq.end().unwrap();
    Ok(ser.into_inner())
  }).await.unwrap()
}

pub struct CORS;

#[rocket::async_trait]
impl Fairing for CORS 
{
  fn info(&self) -> Info 
  {
    Info 
    {
      name: "Attaching CORS headers to responses",
      kind: Kind::Response
    }
  }

  async fn on_response<'r>(&self, request: &'r Request<'_>, response: &mut Response<'r>) 
  {
    response.set_header(Header::new("Access-Control-Allow-Origin", "*"));
    response.set_header(Header::new("Access-Control-Allow-Methods", "POST, GET, PATCH, OPTIONS"));
    response.set_header(Header::new("Access-Control-Allow-Headers", "*"));
    response.set_header(Header::new("Access-Control-Allow-Credentials", "true"));

    if request.method() == http::Method::Options && request.route().is_none() 
    {
      response.set_status(Status::NoContent);
      let _ = response.body_mut().take();
    }
  }
}

struct ConfigApiKey(String);
struct ApiKey<'r>(&'r str);

#[derive(Debug)]
enum ApiKeyError 
{
    Missing,
    Invalid,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ApiKey<'r> 
{
  type Error = ApiKeyError;

  async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> 
  {
    /// Returns true if `key` is a valid API key string.
    fn is_valid(api_key : &str, key : &str) -> bool 
    {
      api_key == key
    }

    let api_key = &req.guard::<&State<ConfigApiKey>>().await.succeeded().unwrap().inner().0;

    match req.headers().get_one("x-api-key") 
    {
      Some(key) if is_valid(api_key, key) => Outcome::Success(ApiKey(key)),
      None => Outcome::Failure((Status::Unauthorized, ApiKeyError::Missing)),
      Some(_) => Outcome::Failure((Status::Unauthorized, ApiKeyError::Invalid)),
    }
  }
}


/// Launch the server.
pub async fn serve(args : Arguments, session : Session) -> Result<(), Box<dyn Error>>
{
  let session = Arc::new(session);
  let config_plugins_types = args.plugins_types; 
  let upload_dir = args.upload;
  let config = Config { port: args.address.port(), 
                        address: args.address.ip(), 
                        limits: Limits::default().limit("json", 10000.mebibytes()), 
                        ..Default::default() 
                      };
  //config.limits = Limits::default().limit("bytes", 10000.mebibytes());
  //config.limits = Limits::default().limit("string", 10000.mebibytes());

  //pass cert name in config
  //config.tls = Some(TlsConfig::from_paths("cert.pem", "priv.pem"));
  warn!("Using API key : {}", args.api_key);
  let api_key = ConfigApiKey(args.api_key); 

  let rocket = rocket::custom(config)
          .attach(Shield::new()) 
          .attach(CORS)
          .manage(session)
          .manage(config_plugins_types)
          .manage(upload_dir)
          .manage(api_key)
          .mount("/api", routes![plugins, plugin, root, node, nodes, node_by_path, path, parent_id, run,
                 task_count, join, task, tasks, attribute, scan, save, load, node_count, attribute_count, 
                 schedule, query, timeline, upload, download, read, download_id, delete]);

  #[cfg(feature = "frontend-dev")]
  let rocket = rocket.mount("/", FileServer::from("tapir-frontend/build"));
  #[cfg(feature = "frontend")]
  let rocket = rocket.mount("/", StaticFileServer::from());

  #[cfg(feature = "frontend")]
  webbrowser::open(&("http://".to_owned() + &args.address.to_string())).unwrap();//+s for https 
  let _ = rocket.launch().await.unwrap();

  Ok(())
}
